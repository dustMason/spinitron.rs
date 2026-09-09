use super::*;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

#[derive(Default)]
struct FakeSpotify {
    playlists: RefCell<BTreeMap<String, (RemotePlaylist, Vec<String>)>>,
    library: RefCell<HashSet<String>>,
    creates: usize,
    fills: Cell<usize>,
    removals: Cell<usize>,
    fail_fill: Cell<bool>,
    fail_remove: Cell<bool>,
    uncertain_create: bool,
}

impl ArchiveSpotify for FakeSpotify {
    fn owner_id(&self) -> &str {
        "owner"
    }
    async fn resolve(&mut self, tracks: &[Track]) -> Result<Vec<String>> {
        Ok(tracks
            .iter()
            .map(|t| format!("spotify:track:{}", t.song))
            .collect())
    }
    async fn find_archive(&mut self, marker: &str) -> Result<Option<RemotePlaylist>> {
        Ok(self
            .playlists
            .borrow()
            .values()
            .find(|(p, _)| p.description.lines().any(|line| line == marker))
            .map(|(p, _)| p.clone()))
    }
    async fn create_archive(&mut self, name: &str, description: &str) -> Result<RemotePlaylist> {
        self.creates += 1;
        let p = RemotePlaylist {
            id: format!("p{}", self.creates),
            owner_id: "owner".into(),
            name: name.into(),
            description: description.into(),
        };
        self.playlists
            .borrow_mut()
            .insert(p.id.clone(), (p.clone(), Vec::new()));
        self.library.borrow_mut().insert(p.id.clone());
        if self.uncertain_create {
            self.uncertain_create = false;
            bail!("Connection lost after creation");
        }
        Ok(p)
    }
    async fn replace_draft(&self, id: &str, uris: &[String]) -> Result<()> {
        self.fills.set(self.fills.get() + 1);
        self.playlists.borrow_mut().get_mut(id).unwrap().1 = uris.to_vec();
        if self.fail_fill.replace(false) {
            self.playlists
                .borrow_mut()
                .get_mut(id)
                .unwrap()
                .1
                .truncate(1);
            bail!("Second batch failed");
        }
        Ok(())
    }
    async fn inspect(&self, id: &str) -> Result<RemotePlaylist> {
        Ok(self.playlists.borrow()[id].0.clone())
    }
    async fn track_uris(&self, id: &str) -> Result<Vec<String>> {
        Ok(self.playlists.borrow()[id].1.clone())
    }
    async fn preview(&self, _: &str) -> Result<Value> {
        Ok(serde_json::json!([]))
    }
    async fn remove_from_library(&self, id: &str) -> Result<()> {
        self.removals.set(self.removals.get() + 1);
        if self.fail_remove.get() {
            bail!("Uncertain removal");
        }
        self.library.borrow_mut().remove(id);
        Ok(())
    }
}

struct Fixture {
    root: PathBuf,
    catalog: Catalog,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("spinitron-catalog-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let fixture = Self {
            root,
            catalog: Catalog {
                version: 1,
                entries: BTreeMap::new(),
            },
        };
        fixture.catalog.save(&fixture.path()).unwrap();
        fixture
    }
    fn path(&self) -> PathBuf {
        self.root.join("catalog.json")
    }
    fn plan(&self) -> PathBuf {
        self.root.join("release.json")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn show(id: u64) -> Show {
    Show {
        id,
        title: "Don't Stop — 音楽".into(),
        url: format!("https://spinitron.com/KALX/pl/{id}"),
        start_time: "2026-01-01T12:00:00-0800".into(),
        end_time: "2026-01-01T14:00:00-0800".into(),
    }
}
fn tracks() -> Vec<Track> {
    ["first", "second", "first"]
        .into_iter()
        .map(|song| Track {
            artist: "Artist".into(),
            song: song.into(),
            album: String::new(),
            label: None,
            time: None,
        })
        .collect()
}

#[tokio::test]
async fn broadcasts_are_distinct_immutable_and_preserve_repeated_tracks() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify::default();
    for id in [1, 2] {
        f.catalog
            .archive(&f.path(), &mut spotify, "KALX", &show(id), &tracks())
            .await
            .unwrap();
    }
    let mut renamed = show(1);
    renamed.title = "Changed episode title".into();
    assert!(!f
        .catalog
        .archive(&f.path(), &mut spotify, "KALX", &renamed, &[])
        .await
        .unwrap());
    assert_eq!(spotify.creates, 2);
    assert_eq!(spotify.fills.get(), 2);
    assert_eq!(
        spotify.track_uris("p1").await.unwrap(),
        vec![
            "spotify:track:first",
            "spotify:track:second",
            "spotify:track:first"
        ]
    );
    assert!(f.catalog.entries["KALX:1"].listing["name"]
        .as_str()
        .unwrap()
        .contains("音楽"));
}

#[tokio::test]
async fn saving_an_archived_playlist_survives_future_runs() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify::default();
    f.catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .unwrap();
    let plan = f.catalog.prepare_release(&f.path(), "owner").unwrap();
    assert_eq!(
        Catalog::load(&f.path()).unwrap().entries["KALX:1"].state,
        State::ReleaseAttempted
    );
    atomic_json(&f.plan(), &plan).unwrap();
    f.catalog
        .apply_release(&f.path(), &f.plan(), &spotify)
        .await
        .unwrap();
    assert!(!spotify.library.borrow().contains("p1"));
    // Simulate the user saving the broadcast from the website.
    spotify.library.borrow_mut().insert("p1".into());
    f.catalog = Catalog::load(&f.path()).unwrap();
    f.catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .unwrap();
    assert!(f
        .catalog
        .prepare_release(&f.path(), "owner")
        .unwrap()
        .playlists
        .is_empty());
    assert!(spotify.library.borrow().contains("p1"));
    assert_eq!(spotify.removals.get(), 1);
    assert_eq!(spotify.creates, 1);
    assert_eq!(f.catalog.listings().len(), 1);
}

#[tokio::test]
async fn partial_draft_recovers_without_creating_another_playlist() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify::default();
    spotify.fail_fill.set(true);
    assert!(f
        .catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .is_err());
    assert!(f.catalog.listings().is_empty());
    assert!(f
        .catalog
        .prepare_release(&f.path(), "owner")
        .unwrap()
        .playlists
        .is_empty());
    f.catalog = Catalog::load(&f.path()).unwrap();
    f.catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .unwrap();
    assert_eq!(spotify.creates, 1);
    assert_eq!(spotify.track_uris("p1").await.unwrap().len(), 3);
}

#[tokio::test]
async fn uncertain_creation_is_adopted_by_exact_marker() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify {
        uncertain_create: true,
        ..Default::default()
    };
    assert!(f
        .catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .is_err());
    f.catalog = Catalog::load(&f.path()).unwrap();
    f.catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .unwrap();
    assert_eq!(spotify.creates, 1);
}

#[tokio::test]
async fn lost_creation_response_without_recovery_match_does_not_repeat_post() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify {
        uncertain_create: true,
        ..Default::default()
    };
    assert!(f
        .catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .is_err());
    spotify.playlists.borrow_mut().clear();
    assert!(f
        .catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .is_err());
    assert_eq!(spotify.creates, 1);
}

#[tokio::test]
async fn uncertain_removal_is_not_retried_even_if_the_user_saves_it() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify::default();
    f.catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .unwrap();
    let plan = f.catalog.prepare_release(&f.path(), "owner").unwrap();
    atomic_json(&f.plan(), &plan).unwrap();
    spotify.fail_remove.set(true);
    assert!(f
        .catalog
        .apply_release(&f.path(), &f.plan(), &spotify)
        .await
        .is_err());
    assert!(!f.plan().exists());
    assert!(f
        .catalog
        .prepare_release(&f.path(), "owner")
        .unwrap()
        .playlists
        .is_empty());
    atomic_json(&f.plan(), &plan).unwrap();
    assert!(f
        .catalog
        .apply_release(&f.path(), &f.plan(), &spotify)
        .await
        .is_err());
    assert_eq!(spotify.removals.get(), 1);
}

#[tokio::test]
async fn empty_or_unfinished_broadcast_does_not_create_a_playlist() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify::default();
    assert!(f
        .catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &[])
        .await
        .is_err());
    let mut current = show(2);
    current.end_time = (Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    assert!(f
        .catalog
        .archive(&f.path(), &mut spotify, "KALX", &current, &tracks())
        .await
        .is_err());
    assert_eq!(spotify.creates, 0);
    assert!(f.catalog.entries.is_empty());
}

#[tokio::test]
async fn legacy_playlists_remain_listed_and_are_never_automatically_removed() {
    let mut f = Fixture::new();
    f.catalog.entries.insert("legacy:existing".into(),Entry { station:"KALX".into(),broadcast:None,state:State::Legacy,owner_id:None,playlist_id:Some("existing".into()),desired_uris:Vec::new(),listing:serde_json::json!({"name":"Old archive","url":"https://open.spotify.com/playlist/existing","track_count":17}),release_attempt:None });
    assert_eq!(f.catalog.listings().len(), 1);
    assert!(f
        .catalog
        .prepare_release(&f.path(), "owner")
        .unwrap()
        .playlists
        .is_empty());
}

#[test]
fn missing_or_corrupt_catalog_fails_closed() {
    let f = Fixture::new();
    fs::write(f.path(), "{bad json").unwrap();
    assert!(Catalog::load(&f.path()).is_err());
    fs::remove_file(f.path()).unwrap();
    assert!(Catalog::load(&f.path()).is_err());
}

#[test]
fn spinitron_compact_offsets_and_rfc3339_offsets_are_equivalent() {
    assert_eq!(
        broadcast_time("2026-09-08T14:00:00-0700").unwrap(),
        broadcast_time("2026-09-08T14:00:00-07:00").unwrap()
    );
    assert!(broadcast_time("not a time").is_err());
}
