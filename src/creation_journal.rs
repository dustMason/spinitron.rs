//! Acknowledged, account-bound creation records on a dedicated Git branch.
//!
//! The branch is intentionally separate from the website/catalog commit. Each
//! checkpoint is pushed before the next Spotify mutation. A rejected or uncertain
//! push poisons this writer; restart from the remote rather than guessing.
#[cfg(test)]
use crate::catalog::atomic_json;
use crate::catalog::{Catalog, Entry, State};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const BRANCH: &str = "spinitron-creation-journal";
const FILE: &str = "creation-journal.json";

/// GitHub recommends at most six pushes/minute/repository. Leave twelve
/// seconds after each acknowledged push, including between playlist stages.
/// The workflow's concurrency group serializes archive writers.
#[derive(Debug)]
struct PushPacer {
    interval: Duration,
    next_push: Option<Instant>,
}

impl Default for PushPacer {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(12),
            next_push: None,
        }
    }
}

impl PushPacer {
    fn remaining(&self, now: Instant) -> Duration {
        self.next_push
            .map(|next| next.saturating_duration_since(now))
            .unwrap_or_default()
    }

    fn wait(&self) {
        std::thread::sleep(self.remaining(Instant::now()));
    }

    fn acknowledged(&mut self, now: Instant) {
        self.next_push = Some(now + self.interval);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub entry: Entry,
    /// Old Prepared records cannot prove that a previous runner never sent POST.
    pub needs_reconciliation: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Data {
    version: u32,
    owner_id: String,
    records: BTreeMap<String, Record>,
}

#[derive(Debug)]
pub struct CreationJournal {
    data: Data,
    store: Store,
    initialized: bool,
    failure: Option<String>,
}

#[derive(Debug)]
enum Store {
    Git {
        repo: PathBuf,
        head: Option<String>,
        pacer: PushPacer,
    },
    #[cfg(test)]
    File {
        path: PathBuf,
        writes_left: Option<usize>,
        lose_ack: bool,
    },
}

impl CreationJournal {
    /// Uses origin's configured authentication without copying credentials or
    /// switching branches, touching the working tree, or staging source files.
    pub fn open(repo: &Path, owner: &str, initialize: bool) -> Result<Self> {
        let reference = format!("refs/heads/{BRANCH}");
        let output = git_output(
            repo,
            &["ls-remote", "--exit-code", "origin", &reference],
            None,
        )?;
        let head = if output.status.success() {
            // Fetch an immutable object, then read FETCH_HEAD once. A later
            // writer can only cause our next push to fail, not authorize a POST.
            git(repo, &["fetch", "--no-tags", "origin", &reference], None)?;
            Some(
                git(repo, &["rev-parse", "FETCH_HEAD"], None)?
                    .trim()
                    .to_owned(),
            )
        } else if output.status.code() == Some(2) {
            if !initialize {
                bail!("Creation journal branch is missing. For the first deployment only, run with --initialize-creation-journal; restore a lost journal from backup instead of reinitializing it");
            }
            None
        } else {
            bail!("Could not read the creation journal branch; check origin authentication/connectivity");
        };
        if initialize && head.is_some() {
            bail!("Creation journal already exists; omit --initialize-creation-journal");
        }
        let data = match &head {
            Some(oid) => parse(
                &git(repo, &["show", &format!("{oid}:{FILE}")], None)?,
                owner,
            )?,
            None => Data {
                version: 1,
                owner_id: owner.into(),
                records: BTreeMap::new(),
            },
        };
        Ok(Self {
            initialized: head.is_some(),
            data,
            store: Store::Git {
                repo: repo.into(),
                head,
                pacer: PushPacer::default(),
            },
            failure: None,
        })
    }

    pub fn records(&self) -> &BTreeMap<String, Record> {
        &self.data.records
    }

    pub fn check_health(&self) -> Result<()> {
        if let Some(reason) = &self.failure {
            bail!("Creation journal checkpoint failed: {reason}; restart from the remote before further Spotify writes");
        }
        Ok(())
    }

    pub fn needs_recovery(&self, key: &str) -> bool {
        self.data
            .records
            .get(key)
            .is_none_or(|r| r.needs_reconciliation)
    }

    /// Spend the pacing wait before POST so the returned ID can be pushed
    /// immediately, without deliberately extending the lost-response window.
    pub fn pace_creation(&self) -> Result<()> {
        self.check_health()?;
        match &self.store {
            Store::Git { pacer, .. } => pacer.wait(),
            #[cfg(test)]
            Store::File { .. } => (),
        }
        Ok(())
    }

    /// Register legacy pending work and retain any IDs/completions that reached
    /// the catalog even if an earlier journal acknowledgement was lost.
    pub fn reconcile_catalog(&mut self, entries: &BTreeMap<String, Entry>) -> Result<()> {
        self.check_health()?;
        let before = serde_json::to_vec(&self.data)?;
        for (key, entry) in entries {
            if let Some(record) = self.data.records.get_mut(key) {
                if entry.playlist_id.is_some() {
                    record.entry = entry.clone();
                    record.needs_reconciliation = false;
                }
            } else if matches!(
                entry.state,
                State::Prepared | State::Creating | State::Filling
            ) {
                self.data.records.insert(
                    key.clone(),
                    Record {
                        entry: entry.clone(),
                        needs_reconciliation: entry.playlist_id.is_none(),
                    },
                );
            }
        }
        validate(&self.data, &self.data.owner_id)?;
        if !self.initialized || serde_json::to_vec(&self.data)? != before {
            self.flush()?;
        }
        Ok(())
    }

    pub fn record(&mut self, key: &str, entry: &Entry) -> Result<()> {
        self.check_health()?;
        // This method is called only after reconciliation or before an actual
        // creation attempt. Mere refreshes of old Prepared names never call it.
        self.data.records.insert(
            key.into(),
            Record {
                entry: entry.clone(),
                needs_reconciliation: false,
            },
        );
        validate(&self.data, &self.data.owner_id)?;
        self.flush()
    }

    fn flush(&mut self) -> Result<()> {
        self.check_health()?;
        let bytes = serde_json::to_vec_pretty(&self.data)?;
        let result = match &mut self.store {
            Store::Git { repo, head, pacer } => (|| {
                let blob = git(repo, &["hash-object", "-w", "--stdin"], Some(&bytes))?;
                let tree = git(
                    repo,
                    &["mktree"],
                    Some(format!("100644 blob {}\t{FILE}\n", blob.trim()).as_bytes()),
                )?;
                // The nonce is essential: simultaneous claims with identical
                // data/parent/time MUST have distinct commit IDs, so only one
                // non-fast-forward-protected push can succeed.
                let message = format!("Checkpoint Spotify creations {}", uuid::Uuid::new_v4());
                let mut args = vec![
                    "-c",
                    "user.name=Spinitron Archive",
                    "-c",
                    "user.email=action@github.com",
                    "-c",
                    "commit.gpgsign=false",
                    "commit-tree",
                    tree.trim(),
                    "-m",
                    &message,
                ];
                if let Some(parent) = head.as_deref() {
                    args.extend(["-p", parent]);
                }
                let commit = git(repo, &args, None)?.trim().to_owned();
                pacer.wait();
                git(repo, &["push", "origin", &format!("{commit}:refs/heads/{BRANCH}")], None)
                    .context("Creation journal push was not acknowledged; no further Spotify writes are allowed")?;
                pacer.acknowledged(Instant::now());
                *head = Some(commit);
                Ok(())
            })(),
            #[cfg(test)]
            Store::File {
                path,
                writes_left,
                lose_ack,
            } => {
                if *writes_left == Some(0) {
                    if *lose_ack {
                        atomic_json(path, &self.data)?;
                    }
                    Err(anyhow::anyhow!("Injected journal write failure"))
                } else {
                    if let Some(left) = writes_left {
                        *left -= 1;
                    }
                    atomic_json(path, &self.data)
                }
            }
        };
        if let Err(error) = &result {
            // Keep the original sanitized cause: the archive's final health
            // check must not replace it with an uninformative generic error.
            self.failure = Some(format!("{error:#}"));
        } else {
            self.initialized = true;
        }
        result
    }

    #[cfg(test)]
    pub fn for_test(path: &Path, owner: &str) -> Self {
        let initialized = path.exists();
        let data = if initialized {
            parse(&std::fs::read_to_string(path).unwrap(), owner).unwrap()
        } else {
            Data {
                version: 1,
                owner_id: owner.into(),
                records: BTreeMap::new(),
            }
        };
        Self {
            data,
            initialized,
            failure: None,
            store: Store::File {
                path: path.into(),
                writes_left: None,
                lose_ack: false,
            },
        }
    }

    #[cfg(test)]
    pub fn lose_ack_after(&mut self, writes: usize) {
        self.fail_after(writes);
        if let Store::File { lose_ack, .. } = &mut self.store {
            *lose_ack = true;
        }
    }

    #[cfg(test)]
    pub fn fail_after(&mut self, writes: usize) {
        if let Store::File { writes_left, .. } = &mut self.store {
            *writes_left = Some(writes);
        }
    }
}

fn parse(text: &str, owner: &str) -> Result<Data> {
    let data: Data = serde_json::from_str(text)
        .context("Invalid creation journal; restore it instead of resetting it")?;
    validate(&data, owner)?;
    Ok(data)
}

fn validate(data: &Data, owner: &str) -> Result<()> {
    if data.version != 1 || data.owner_id != owner || owner.is_empty() {
        bail!("Unsupported creation journal version or Spotify owner mismatch");
    }
    let mut ids = std::collections::HashSet::new();
    for (key, record) in &data.records {
        let entry = &record.entry;
        let show = entry
            .broadcast
            .as_ref()
            .context("Creation record is missing its broadcast")?;
        if Catalog::key(&entry.station, show) != *key
            || entry.owner_id.as_deref() != Some(owner)
            || entry.desired_uris.is_empty()
            || !matches!(
                entry.state,
                State::Prepared
                    | State::Creating
                    | State::Filling
                    | State::Ready
                    | State::Released
                    | State::ReleaseAttempted
            )
            || matches!(entry.state, State::Prepared | State::Creating)
                != entry.playlist_id.is_none()
            || (record.needs_reconciliation && entry.playlist_id.is_some())
        {
            bail!("Invalid creation record for {key}");
        }
        if let Some(id) = &entry.playlist_id {
            if id.is_empty() || !ids.insert(id) {
                bail!("Duplicate or empty playlist ID in creation journal");
            }
        }
    }
    Ok(())
}

fn git_output(repo: &Path, args: &[&str], input: Option<&[u8]>) -> Result<std::process::Output> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not start Git for the creation journal")?;
    if let Some(bytes) = input {
        child.stdin.take().unwrap().write_all(bytes)?;
    }
    Ok(child.wait_with_output()?)
}

fn git(repo: &Path, args: &[&str], input: Option<&[u8]>) -> Result<String> {
    let result = git_output(repo, args, input)?;
    if !result.status.success() {
        // Git diagnostics can contain credential-bearing remote URLs. Keep the
        // error actionable without echoing config, URLs, stderr or HTTP headers.
        let operation = args
            .iter()
            .find(|arg| {
                matches!(
                    **arg,
                    "ls-remote"
                        | "fetch"
                        | "show"
                        | "rev-parse"
                        | "hash-object"
                        | "mktree"
                        | "commit-tree"
                        | "push"
                )
            })
            .copied()
            .unwrap_or("operation");
        bail!(
            "Creation journal Git {operation} failed (exit {:?}): {}",
            result.status.code(),
            git_failure_category(&result.stderr)
        );
    }
    String::from_utf8(result.stdout).context("Invalid UTF-8 in creation journal Git output")
}

/// Classify known failure signatures without printing credential-bearing URLs,
/// HTTP headers, or arbitrary hook/server output.
fn git_failure_category(stderr: &[u8]) -> &'static str {
    let text = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    if [
        "too many requests",
        "rate limit",
        "throttl",
        "http 429",
        "error: 429",
        "abuse",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        "remote throttling/rate limit; allow a cooldown before restarting"
    } else if ["non-fast-forward", "fetch first", "cannot lock ref"]
        .iter()
        .any(|s| text.contains(s))
    {
        "remote ref changed; reload journal state before restarting"
    } else if ["protected branch", "gh006", "gh013"]
        .iter()
        .any(|s| text.contains(s))
    {
        "remote branch protection rejected the update"
    } else if [
        "authentication failed",
        "permission denied",
        "http 401",
        "http 403",
        "error: 401",
        "error: 403",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        "origin authentication or permissions rejected the operation"
    } else if (500..=599).any(|status| {
        text.contains(&format!("http {status}")) || text.contains(&format!("error: {status}"))
    }) || text.contains("internal server error")
    {
        "remote server error"
    } else if [
        "could not resolve host",
        "failed to connect",
        "connection reset",
        "timed out",
        "unexpected disconnect",
        "remote end hung up",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        "network/connection failure"
    } else {
        "unclassified error (raw output withheld to protect credentials)"
    }
}

#[cfg(test)]
#[path = "creation_journal_tests.rs"]
mod tests;
