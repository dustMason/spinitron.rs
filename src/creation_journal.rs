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
};

const BRANCH: &str = "spinitron-creation-journal";
const FILE: &str = "creation-journal.json";

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
    failed: bool,
}

#[derive(Debug)]
enum Store {
    Git {
        repo: PathBuf,
        head: Option<String>,
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
            },
            failed: false,
        })
    }

    pub fn records(&self) -> &BTreeMap<String, Record> {
        &self.data.records
    }

    pub fn check_health(&self) -> Result<()> {
        if self.failed {
            bail!("Creation journal checkpoint failed; restart from the remote before further Spotify writes");
        }
        Ok(())
    }

    pub fn needs_recovery(&self, key: &str) -> bool {
        self.data
            .records
            .get(key)
            .is_none_or(|r| r.needs_reconciliation)
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
            Store::Git { repo, head } => (|| {
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
                git(repo, &["push", "origin", &format!("{commit}:refs/heads/{BRANCH}")], None)
                    .context("Creation journal push was not acknowledged (possible concurrent writer); no further Spotify writes are allowed")?;
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
        if result.is_err() {
            self.failed = true;
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
            failed: false,
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
        bail!("Creation journal Git operation failed (exit {:?}); check origin access and concurrent writers", result.status.code());
    }
    String::from_utf8(result.stdout).context("Invalid UTF-8 in creation journal Git output")
}

#[cfg(test)]
#[path = "creation_journal_tests.rs"]
mod tests;
