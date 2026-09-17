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
    renames: Cell<usize>,
    uncertain_rename: Cell<bool>,
    fail_fill: Cell<bool>,
    fail_remove: Cell<bool>,
    unavailable_when_unsaved: bool,
    fail_save: bool,
    saves: Cell<usize>,
    uncertain_create: bool,
    reject_create: bool,
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
            .find(|(p, _)| p.matches_marker(marker))
            .map(|(p, _)| p.clone()))
    }
    async fn create_archive(&mut self, name: &str, description: &str) -> Result<RemotePlaylist> {
        self.creates += 1;
        if description.contains(['\n', '\r']) {
            return Err(CreationRejected(
                "Spotify rejects multiline descriptions (HTTP 400)".into(),
            )
            .into());
        }
        if self.reject_create {
            self.reject_create = false;
            return Err(
                CreationRejected("Spotify rejected the description (HTTP 400)".into()).into(),
            );
        }
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
    async fn rename_archive(&self, id: &str, name: &str) -> Result<()> {
        self.renames.set(self.renames.get() + 1);
        self.playlists.borrow_mut().get_mut(id).unwrap().0.name = name.into();
        if self.uncertain_rename.replace(false) {
            bail!("Lost rename response");
        }
        Ok(())
    }
    async fn track_uris(&self, id: &str) -> Result<Vec<String>> {
        if self.unavailable_when_unsaved && !self.library.borrow().contains(id) {
            bail!("Resource not found");
        }
        Ok(self.playlists.borrow()[id].1.clone())
    }
    async fn preview(&self, _: &str) -> Result<Value> {
        Ok(serde_json::json!([]))
    }
    async fn is_saved(&self, id: &str) -> Result<bool> {
        Ok(self.library.borrow().contains(id))
    }
    async fn save_to_library(&self, id: &str) -> Result<()> {
        self.saves.set(self.saves.get() + 1);
        if self.fail_save {
            bail!("Recovery request failed");
        }
        self.library.borrow_mut().insert(id.into());
        Ok(())
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
async fn names_only_add_times_for_collisions_and_rename_earlier_broadcasts_once() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify::default();
    let mut first = show(1);
    first.title = "FREEFORM".into();
    first.start_time = "2026-01-01T00:00:00-0800".into();
    f.catalog
        .archive(&f.path(), &mut spotify, "KALX", &first, &tracks())
        .await
        .unwrap();
    assert_eq!(
        spotify.inspect("p1").await.unwrap().name,
        "KALX - FREEFORM - 2026-01-01"
    );
    let before = f.catalog.entries["KALX:1"].clone();
    // Unsaved and saved copies retain their membership across name changes.
    spotify.library.borrow_mut().remove("p1");
    let mut second = first.clone();
    second.id = 2;
    second.start_time = "2026-01-01T17:00:00-0800".into();
    f.catalog
        .archive(&f.path(), &mut spotify, "KALX", &second, &tracks())
        .await
        .unwrap();
    assert_eq!(
        spotify.inspect("p2").await.unwrap().name,
        "KALX - FREEFORM - 2026-01-01 5:00pm"
    );
    let pause = std::time::Duration::ZERO;
    assert_eq!(
        f.catalog
            .sync_archive_names(&f.path(), &spotify, 40, pause)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        spotify.inspect("p1").await.unwrap().name,
        "KALX - FREEFORM - 2026-01-01 12:00am"
    );
    assert_eq!(
        f.catalog
            .sync_archive_names(&f.path(), &spotify, 40, pause)
            .await
            .unwrap(),
        0
    );
    let mut after = f.catalog.entries["KALX:1"].clone();
    after.listing["name"] = before.listing["name"].clone();
    assert_eq!(
        serde_json::to_value(after).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    assert_eq!(
        spotify.track_uris("p1").await.unwrap(),
        spotify.track_uris("p2").await.unwrap()
    );
    assert!(!spotify.library.borrow().contains("p1"));
    assert!(spotify.library.borrow().contains("p2"));
    assert_eq!(
        (
            spotify.creates,
            spotify.fills.get(),
            spotify.renames.get(),
            spotify.removals.get()
        ),
        (2, 2, 1, 0)
    );
    assert_eq!(
        f.catalog.listings().unwrap()[0]["display_name"],
        "KALX - FREEFORM - 2026-01-01 12:00am"
    );
}

#[tokio::test]
async fn names_preserve_suffix_and_unicode_and_disambiguate_dst_and_truncation() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify::default();
    let title = "音楽".repeat(100);
    for (id, start, ending) in [
        (1, "2025-11-02T01:00:00-0700", "A"),
        (2, "2025-11-02T01:00:00-0800", "B"),
        (3, "2025-11-02T01:00:00-0800", "C"),
        (4, "2025-11-03T12:00:00-0800", "D"),
    ] {
        let mut broadcast = show(id);
        broadcast.title = format!("{title}{ending}");
        broadcast.start_time = start.into();
        f.catalog
            .archive(&f.path(), &mut spotify, "KALX", &broadcast, &tracks())
            .await
            .unwrap();
    }
    let names = f.catalog.archive_names().unwrap();
    assert!(names["KALX:1"].ends_with(" - 2025-11-02 1:00am -0700"));
    assert!(names["KALX:2"].ends_with(" - 2025-11-02 1:00am -0800 [KALX:2]"));
    assert!(names["KALX:3"].ends_with(" - 2025-11-02 1:00am -0800 [KALX:3]"));
    assert!(names["KALX:4"].ends_with(" - 2025-11-03"));
    assert!(names
        .values()
        .all(|n| n.chars().count() <= 100 && n.contains("音楽")));
    assert_eq!(names.values().collect::<HashSet<_>>().len(), 4);
}

#[tokio::test]
async fn name_migration_is_bounded_reconciles_lost_response_and_preserves_custom_names() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify::default();
    for id in [1, 2] {
        f.catalog
            .archive(&f.path(), &mut spotify, "KALX", &show(id), &tracks())
            .await
            .unwrap();
        let old = format!("KALX - 2026-01-01 12:00 - Old title {id}");
        f.catalog
            .entries
            .get_mut(&format!("KALX:{id}"))
            .unwrap()
            .listing["name"] = old.clone().into();
        spotify
            .playlists
            .borrow_mut()
            .get_mut(&format!("p{id}"))
            .unwrap()
            .0
            .name = old;
    }
    f.catalog.save(&f.path()).unwrap();
    spotify.uncertain_rename.set(true);
    let pause = std::time::Duration::ZERO;
    assert!(f
        .catalog
        .sync_archive_names(&f.path(), &spotify, 1, pause)
        .await
        .is_err());
    f.catalog = Catalog::load(&f.path()).unwrap();
    assert_eq!(
        f.catalog
            .sync_archive_names(&f.path(), &spotify, 1, pause)
            .await
            .unwrap(),
        1
    );
    assert_eq!(spotify.renames.get(), 1); // Lost response is recovered without another PUT.
    spotify.playlists.borrow_mut().get_mut("p2").unwrap().0.name = "My custom title".into();
    assert!(f
        .catalog
        .sync_archive_names(&f.path(), &spotify, 40, pause)
        .await
        .unwrap_err()
        .to_string()
        .contains("outside the catalog"));
    assert_eq!(spotify.renames.get(), 1);
    spotify
        .playlists
        .borrow_mut()
        .get_mut("p2")
        .unwrap()
        .0
        .owner_id = "someone-else".into();
    assert!(f
        .catalog
        .sync_archive_names(&f.path(), &spotify, 40, pause)
        .await
        .unwrap_err()
        .to_string()
        .contains("owner"));
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
    let imported_at = f.catalog.entries["KALX:1"].listing["imported_at"].clone();
    assert!(DateTime::parse_from_rfc3339(imported_at.as_str().unwrap()).is_ok());
    let mut renamed = show(1);
    renamed.title = "Changed episode title".into();
    assert!(!f
        .catalog
        .archive(&f.path(), &mut spotify, "KALX", &renamed, &[])
        .await
        .unwrap());
    assert_eq!(spotify.creates, 2);
    assert_eq!(
        f.catalog.entries["KALX:1"].listing["imported_at"],
        imported_at
    );
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
    assert_eq!(spotify.saves.get(), 0);
    assert_eq!(spotify.creates, 1);
    assert_eq!(f.catalog.listings().unwrap().len(), 1);
}

#[tokio::test]
async fn inaccessible_removal_restores_original_playlist_and_stops_the_batch() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify {
        unavailable_when_unsaved: true,
        ..Default::default()
    };
    for id in 1..=2 {
        f.catalog
            .archive(&f.path(), &mut spotify, "KALX", &show(id), &tracks())
            .await
            .unwrap();
    }
    let plan = f.catalog.prepare_release(&f.path(), "owner").unwrap();
    atomic_json(&f.plan(), &plan).unwrap();
    let error = f
        .catalog
        .apply_release(&f.path(), &f.plan(), &spotify)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Restored to the library"));
    assert_eq!(spotify.removals.get(), 1);
    assert_eq!(spotify.saves.get(), 1);
    assert!(spotify.is_saved("p1").await.unwrap());
    assert!(spotify.is_saved("p2").await.unwrap());
    assert_eq!(spotify.fills.get(), 2); // Recovery must never repopulate a broadcast.
    f.catalog = Catalog::load(&f.path()).unwrap();
    assert!(f
        .catalog
        .entries
        .values()
        .all(|e| e.state == State::ReleaseAttempted));
    assert!(f
        .catalog
        .prepare_release(&f.path(), "owner")
        .unwrap()
        .playlists
        .is_empty());
    assert_eq!(
        spotify.track_uris("p1").await.unwrap(),
        f.catalog.entries["KALX:1"].desired_uris
    );
}

#[tokio::test]
async fn failed_recovery_is_reported_without_repeating_writes() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify {
        unavailable_when_unsaved: true,
        fail_save: true,
        ..Default::default()
    };
    f.catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .unwrap();
    let plan = f.catalog.prepare_release(&f.path(), "owner").unwrap();
    atomic_json(&f.plan(), &plan).unwrap();
    let error = f
        .catalog
        .apply_release(&f.path(), &f.plan(), &spotify)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Recovery needs manual review"));
    assert!(error.to_string().contains("Resource not found"));
    assert_eq!(spotify.removals.get(), 1);
    assert_eq!(spotify.saves.get(), 1);
    assert_eq!(
        Catalog::load(&f.path()).unwrap().entries["KALX:1"].state,
        State::ReleaseAttempted
    );
}

#[tokio::test]
async fn changed_tracks_are_not_removed_from_the_library() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify::default();
    f.catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .unwrap();
    let plan = f.catalog.prepare_release(&f.path(), "owner").unwrap();
    atomic_json(&f.plan(), &plan).unwrap();
    spotify
        .playlists
        .borrow_mut()
        .get_mut("p1")
        .unwrap()
        .1
        .swap(0, 1);
    assert!(f
        .catalog
        .apply_release(&f.path(), &f.plan(), &spotify)
        .await
        .is_err());
    assert_eq!(spotify.removals.get(), 0);
    assert_eq!(spotify.saves.get(), 0);
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
    assert!(f.catalog.listings().unwrap().is_empty());
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
async fn rejected_creation_is_persisted_as_prepared_and_can_be_retried() {
    let mut f = Fixture::new();
    let mut spotify = FakeSpotify {
        reject_create: true,
        ..Default::default()
    };
    let error = f
        .catalog
        .archive(&f.path(), &mut spotify, "KALX", &show(1), &tracks())
        .await
        .unwrap_err();
    assert!(error.is::<CreationRejected>());
    f.catalog = Catalog::load(&f.path()).unwrap();
    assert_eq!(f.catalog.entries["KALX:1"].state, State::Prepared);
    assert!(f.catalog.entries["KALX:1"].playlist_id.is_none());
    assert!(spotify.playlists.borrow().is_empty());
    let resumed = f.catalog.resume_pending(&f.path(), &mut spotify).await;
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].0, "KALX:1");
    assert!(resumed.into_iter().next().unwrap().1.unwrap());
    assert_eq!(spotify.creates, 2);
    assert_eq!(spotify.playlists.borrow().len(), 1);
    assert_eq!(f.catalog.entries["KALX:1"].state, State::Ready);
    assert_eq!(spotify.track_uris("p2").await.unwrap().len(), 3);
    assert!(f
        .catalog
        .resume_pending(&f.path(), &mut spotify)
        .await
        .is_empty());
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
    assert_eq!(f.catalog.listings().unwrap().len(), 1);
    assert!(f.catalog.archive_names().unwrap().is_empty());
    assert_eq!(
        f.catalog
            .sync_archive_names(
                &f.path(),
                &FakeSpotify::default(),
                40,
                std::time::Duration::ZERO
            )
            .await
            .unwrap(),
        0
    );
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

#[test]
fn descriptions_are_one_line_bounded_and_keep_the_exact_marker() {
    let mut broadcast = show(1);
    broadcast.url = format!(
        "https://spinitron.com/KALX/pl/1/{}\nextra\ttext",
        "音".repeat(400)
    );
    let description = archive_description("Spinitron archive: KALX:1", &broadcast);
    assert!(description.starts_with("Spinitron archive: KALX:1 | Broadcast: "));
    assert!(!description.chars().any(char::is_control));
    assert_eq!(description.chars().count(), 300);
}

#[test]
fn recovery_markers_accept_both_formats_but_not_prefix_collisions() {
    for description in [
        "Spinitron archive: KALX:1 | Broadcast: 2026-01-01 | https://spinitron.com/KALX/pl/1",
        "Spinitron archive: KALX:1\nBroadcast: 2026-01-01\nhttps://spinitron.com/KALX/pl/1",
        "Spinitron archive: KALX:1",
    ] {
        let playlist = RemotePlaylist {
            id: "p".into(),
            owner_id: "owner".into(),
            name: "Broadcast".into(),
            description: description.into(),
        };
        assert!(playlist.matches_marker("Spinitron archive: KALX:1"));
        assert!(!playlist.matches_marker("Spinitron archive: KALX:10"));
        assert!(!playlist.matches_marker("Spinitron archive: KPOO:1"));
    }
    for description in [
        "Spinitron archive: KALX:10 | Broadcast: date",
        "Spinitron archive: KALX:1 extra",
        "Spinitron archive: KALX:1|invalid separator",
    ] {
        let playlist = RemotePlaylist {
            id: "p".into(),
            owner_id: "owner".into(),
            name: "Broadcast".into(),
            description: description.into(),
        };
        assert!(!playlist.matches_marker("Spinitron archive: KALX:1"));
    }
}
