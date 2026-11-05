use super::*;
use crate::models::Show;
use serde_json::json;
use std::fs;

struct Repo {
    root: PathBuf,
    client: PathBuf,
}
impl Repo {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("spinitron-journal-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "--bare", "remote.git"], None).unwrap();
        let client = root.join("client");
        fs::create_dir(&client).unwrap();
        git(&client, &["init"], None).unwrap();
        git(
            &client,
            &[
                "remote",
                "add",
                "origin",
                root.join("remote.git").to_str().unwrap(),
            ],
            None,
        )
        .unwrap();
        Self { root, client }
    }
    fn journal(&self, initialize: bool) -> Result<CreationJournal> {
        let mut journal = CreationJournal::open(&self.client, "owner", initialize)?;
        // Bare-repository tests do not contact GitHub. Test the production
        // twelve-second policy with a simulated clock below instead of sleeping.
        if let Store::Git { pacer, .. } = &mut journal.store {
            pacer.interval = Duration::ZERO;
        }
        Ok(journal)
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn entry() -> Entry {
    Entry {
        station: "KALX".into(),
        broadcast: Some(Show {
            id: 1,
            title: "Radio".into(),
            url: "https://spinitron.com/KALX/pl/1".into(),
            start_time: "2026-01-01T12:00:00-0800".into(),
            end_time: "2026-01-01T14:00:00-0800".into(),
        }),
        state: State::Creating,
        owner_id: Some("owner".into()),
        playlist_id: None,
        desired_uris: vec!["spotify:track:a".into()],
        listing: json!({"name": "Radio"}),
        release_attempt: None,
    }
}

#[test]
fn git_journal_survives_lost_catalog_without_touching_source_branch_or_index() {
    let repo = Repo::new();
    fs::write(repo.client.join("source.txt"), "unsaved source work").unwrap();
    git(&repo.client, &["add", "source.txt"], None).unwrap();
    let status = git(&repo.client, &["status", "--porcelain"], None).unwrap();
    let branch = git(&repo.client, &["symbolic-ref", "HEAD"], None).unwrap();
    let mut journal = repo.journal(true).unwrap();
    journal.reconcile_catalog(&BTreeMap::new()).unwrap();
    journal.record("KALX:1", &entry()).unwrap();
    let mut ready = entry();
    ready.state = State::Ready;
    ready.playlist_id = Some("spotify-id".into());
    journal.record("KALX:1", &ready).unwrap();
    drop(journal);
    let path = repo.root.join("stale-catalog.json");
    fs::write(&path, r#"{"version":1,"entries":{}}"#).unwrap();
    let mut catalog = Catalog::load(&path).unwrap();
    catalog
        .attach_journal(&path, repo.journal(false).unwrap())
        .unwrap();
    assert!(catalog.finished("KALX:1"));
    assert_eq!(
        catalog.entries["KALX:1"].playlist_id.as_deref(),
        Some("spotify-id")
    );
    assert_eq!(
        git(&repo.client, &["status", "--porcelain"], None).unwrap(),
        status
    );
    assert_eq!(
        git(&repo.client, &["symbolic-ref", "HEAD"], None).unwrap(),
        branch
    );
    assert_eq!(
        fs::read_to_string(repo.client.join("source.txt")).unwrap(),
        "unsaved source work"
    );
    let remote = repo.root.join("remote.git");
    assert_eq!(
        git(&remote, &["ls-tree", "--name-only", BRANCH], None)
            .unwrap()
            .trim(),
        FILE
    );
}

#[test]
fn competing_identical_claims_cannot_both_be_acknowledged() {
    let repo = Repo::new();
    repo.journal(true)
        .unwrap()
        .reconcile_catalog(&BTreeMap::new())
        .unwrap();
    let mut first = repo.journal(false).unwrap();
    let mut second = repo.journal(false).unwrap();
    first.record("KALX:1", &entry()).unwrap();
    let error = second.record("KALX:1", &entry()).unwrap_err();
    assert!(format!("{error:#}").contains("remote ref changed"));
    let health = second.check_health().unwrap_err().to_string();
    assert!(health.contains("Git push failed"));
    assert!(health.contains("remote ref changed"));
    assert_eq!(
        second.record("KALX:1", &entry()).unwrap_err().to_string(),
        health
    );
    assert_eq!(
        repo.journal(false).unwrap().records()["KALX:1"].entry.state,
        State::Creating
    );
}

#[test]
fn missing_journal_requires_explicit_initialization_and_existing_one_cannot_be_reset() {
    let repo = Repo::new();
    assert!(repo.journal(false).is_err());
    repo.journal(true)
        .unwrap()
        .reconcile_catalog(&BTreeMap::new())
        .unwrap();
    assert!(repo.journal(true).is_err());
    assert!(CreationJournal::open(&repo.client, "someone-else", false).is_err());
    git(
        &repo.root.join("remote.git"),
        &["update-ref", "-d", &format!("refs/heads/{BRANCH}")],
        None,
    )
    .unwrap();
    assert!(repo.journal(false).is_err());
}

#[test]
fn malformed_or_conflicting_records_fail_closed() {
    let mut data = Data {
        version: 1,
        owner_id: "owner".into(),
        records: BTreeMap::new(),
    };
    data.records.insert(
        "KALX:1".into(),
        Record {
            entry: entry(),
            needs_reconciliation: false,
        },
    );
    let valid = serde_json::to_value(&data).unwrap();
    for scenario in ["version", "owner", "key", "tracks", "id", "state"] {
        let mut bad = valid.clone();
        match scenario {
            "version" => bad["version"] = 9.into(),
            "owner" => bad["records"]["KALX:1"]["entry"]["owner_id"] = "other".into(),
            "key" => bad["records"]["KALX:1"]["entry"]["broadcast"]["id"] = 2.into(),
            "tracks" => bad["records"]["KALX:1"]["entry"]["desired_uris"] = json!([]),
            "id" => bad["records"]["KALX:1"]["entry"]["playlist_id"] = "p1".into(),
            "state" => bad["records"]["KALX:1"]["entry"]["state"] = "ready".into(),
            _ => unreachable!(),
        }
        assert!(parse(&bad.to_string(), "owner").is_err(), "{scenario}");
    }
    assert!(parse("not JSON", "owner").is_err());
}

#[test]
fn push_pacing_waits_between_acknowledged_checkpoints_without_clock_drift() {
    let mut pacer = PushPacer::default();
    let start = Instant::now();
    assert_eq!(pacer.remaining(start), Duration::ZERO);
    pacer.acknowledged(start);
    assert_eq!(pacer.remaining(start), Duration::from_secs(12));
    assert_eq!(
        pacer.remaining(start + Duration::from_secs(5)),
        Duration::from_secs(7)
    );
    assert_eq!(
        pacer.remaining(start + Duration::from_secs(12)),
        Duration::ZERO
    );
    assert_eq!(
        pacer.remaining(start + Duration::from_secs(90)),
        Duration::ZERO
    );
    // Measure from completion, including slow requests. A long Git request
    // cannot cause following pushes to bunch up while catching up to a schedule.
    pacer.acknowledged(start + Duration::from_secs(90));
    assert_eq!(
        pacer.remaining(start + Duration::from_secs(90)),
        Duration::from_secs(12)
    );
    assert_eq!(
        pacer.remaining(start + Duration::from_secs(102)),
        Duration::ZERO
    );
}

#[test]
fn git_failure_categories_never_expose_credentials_or_arbitrary_remote_output() {
    for (stderr, expected) in [
        (
            "remote: Too many requests https://SECRET@github.com/repo",
            "remote throttling/rate limit",
        ),
        (
            "RPC failed; HTTP 429 Authorization: Bearer SECRET",
            "remote throttling/rate limit",
        ),
        (
            "[rejected] branch -> branch (non-fast-forward) SECRET",
            "remote ref changed",
        ),
        (
            "remote: error: GH013: protected branch; SECRET",
            "remote branch protection",
        ),
        (
            "Authentication failed for https://SECRET@github.com/repo",
            "origin authentication or permissions",
        ),
        (
            "The requested URL returned error: 503 SECRET",
            "remote server error",
        ),
        (
            "fatal: unable to access https://SECRET@github.com: Connection reset by peer",
            "network/connection failure",
        ),
        ("custom hook output contains SECRET", "unclassified error"),
    ] {
        let reason = git_failure_category(stderr.as_bytes());
        assert!(reason.starts_with(expected), "{reason}");
        assert!(!reason.contains("SECRET"));
        assert!(!reason.contains("github.com"));
    }
}

#[test]
fn original_checkpoint_failure_survives_final_health_check() {
    let root = std::env::temp_dir().join(format!(
        "spinitron-journal-error-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir(&root).unwrap();
    let mut journal = CreationJournal::for_test(&root.join("journal.json"), "owner");
    journal.fail_after(0);
    let original = journal.record("KALX:1", &entry()).unwrap_err();
    let final_error = journal.check_health().unwrap_err().to_string();
    assert!(final_error.contains(&original.to_string()));
    assert!(final_error.contains("restart from the remote"));
    fs::remove_dir_all(root).unwrap();
}
