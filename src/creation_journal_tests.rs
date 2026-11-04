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
        CreationJournal::open(&self.client, "owner", initialize)
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
    assert!(second.record("KALX:1", &entry()).is_err());
    assert!(second.check_health().is_err());
    assert!(second.record("KALX:1", &entry()).is_err());
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
