//! Broadcast archives are independent of Spotify library membership.
//!
//! Publication and library removal are separate phases. The workflow checkpoints
//! the catalog before applying a one-use removal plan. An uncertain removal is
//! never retried automatically: the owner may have saved that playlist meanwhile.
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

use crate::models::{Show, Track};

pub fn broadcast_time(value: &str) -> Result<DateTime<chrono::FixedOffset>> {
    // Spinitron uses offsets such as -0700; RFC3339 requires -07:00.
    DateTime::parse_from_rfc3339(value)
        .or_else(|_| DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%z"))
        .context("Invalid Spinitron broadcast timestamp")
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Legacy,
    Prepared,
    Creating,
    Filling,
    Ready,
    ReleaseAttempted,
    Released,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub station: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broadcast: Option<Show>,
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub desired_uris: Vec<String>,
    /// Same public metadata consumed by the website; includes a cached preview.
    pub listing: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_attempt: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Catalog {
    version: u32,
    pub entries: BTreeMap<String, Entry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReleasePlan {
    pub attempt: String,
    pub owner_id: String,
    pub playlists: Vec<ReleaseItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReleaseItem {
    pub key: String,
    pub playlist_id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct RemotePlaylist {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub description: String,
}

pub trait ArchiveSpotify {
    fn owner_id(&self) -> &str;
    async fn resolve(&mut self, tracks: &[Track]) -> Result<Vec<String>>;
    async fn find_archive(&mut self, marker: &str) -> Result<Option<RemotePlaylist>>;
    async fn create_archive(&mut self, name: &str, description: &str) -> Result<RemotePlaylist>;
    async fn replace_draft(&self, id: &str, uris: &[String]) -> Result<()>;
    async fn inspect(&self, id: &str) -> Result<RemotePlaylist>;
    async fn track_uris(&self, id: &str) -> Result<Vec<String>>;
    async fn preview(&self, id: &str) -> Result<Value>;
    async fn remove_from_library(&self, id: &str) -> Result<()>;
}

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".catalog-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

impl Catalog {
    pub fn load(path: &Path) -> Result<Self> {
        let data = fs::read(path).with_context(|| {
            format!(
                "Read catalog {} (do not recreate a missing catalog automatically)",
                path.display()
            )
        })?;
        let catalog: Self = serde_json::from_slice(&data)
            .context("Invalid catalog; refusing to discard archive state")?;
        if catalog.version != 1 {
            bail!("Unsupported catalog version {}", catalog.version);
        }
        let mut ids = std::collections::HashSet::new();
        for entry in catalog.entries.values() {
            if let Some(id) = &entry.playlist_id {
                if !ids.insert(id) {
                    bail!("Catalog contains duplicate Spotify ID {id}");
                }
            } else if !matches!(entry.state, State::Prepared | State::Creating) {
                bail!("Catalog entry is missing its Spotify ID");
            }
        }
        Ok(catalog)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        atomic_json(path, self)
    }

    pub fn key(station: &str, show: &Show) -> String {
        format!("{station}:{}", show.id)
    }

    pub fn finished(&self, key: &str) -> bool {
        self.entries
            .get(key)
            .is_some_and(|e| !matches!(e.state, State::Prepared | State::Creating | State::Filling))
    }

    pub fn listings(&self) -> Vec<&Value> {
        let mut rows: Vec<_> = self
            .entries
            .values()
            .filter(|e| !matches!(e.state, State::Prepared | State::Creating | State::Filling))
            .map(|e| &e.listing)
            .collect();
        rows.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        rows
    }

    pub async fn archive(
        &mut self,
        path: &Path,
        spotify: &mut impl ArchiveSpotify,
        station: &str,
        show: &Show,
        tracks: &[Track],
    ) -> Result<bool> {
        let key = Self::key(station, show);
        if self.finished(&key) {
            return Ok(false);
        }
        let ended = broadcast_time(&show.end_time)?;
        if ended.with_timezone(&Utc) > Utc::now() - chrono::Duration::hours(1) {
            bail!(
                "Broadcast {} has not finished with a one-hour buffer",
                show.id
            );
        }
        let marker = format!("Spinitron archive: {key}");
        let recovering = self.entries.contains_key(&key);
        if !recovering {
            let uris = spotify.resolve(tracks).await?;
            if uris.is_empty() {
                bail!(
                    "No Spotify tracks matched broadcast {}; no playlist created",
                    show.id
                );
            }
            let date = broadcast_time(&show.start_time)?
                .format("%Y-%m-%d %H:%M")
                .to_string();
            let title = show.title.replace("&amp;", "&").replace("&quot;", "\"");
            // Put the date first so it survives title truncation. Keep Unicode.
            let name: String = format!("{station} - {date} - {title}")
                .chars()
                .take(100)
                .collect();
            self.entries.insert(
                key.clone(),
                Entry {
                    station: station.into(),
                    broadcast: Some(show.clone()),
                    state: State::Prepared,
                    owner_id: Some(spotify.owner_id().into()),
                    playlist_id: None,
                    desired_uris: uris,
                    listing: serde_json::json!({"station":station,"name":name}),
                    release_attempt: None,
                },
            );
            self.save(path)?;
        }
        if self.entries[&key].owner_id.as_deref() != Some(spotify.owner_id()) {
            bail!("Catalog belongs to a different Spotify account");
        }
        if self.entries[&key].playlist_id.is_none() {
            // Recovery can adopt an owned, precisely marked creation, but never
            // blindly repeats an uncertain POST and creates another playlist.
            let remote = match spotify.find_archive(&marker).await? {
                Some(found) => found,
                None if self.entries[&key].state == State::Creating => bail!(
                    "Creation of {key} is uncertain; reconcile the marked playlist before retrying"
                ),
                None => {
                    self.entries.get_mut(&key).unwrap().state = State::Creating;
                    self.save(path)?;
                    let name = self.entries[&key].listing["name"]
                        .as_str()
                        .context("Missing archive name")?;
                    spotify
                        .create_archive(
                            name,
                            &format!("{marker}\nBroadcast: {}\n{}", show.start_time, show.url),
                        )
                        .await?
                }
            };
            if remote.owner_id != spotify.owner_id()
                || !remote.description.lines().any(|line| line == marker)
            {
                bail!("Created/recovered playlist has unexpected owner or archive marker");
            }
            let entry = self.entries.get_mut(&key).unwrap();
            entry.playlist_id = Some(remote.id);
            entry.state = State::Filling;
            self.save(path)?;
        }
        let entry = &self.entries[&key];
        let id = entry.playlist_id.as_deref().unwrap();
        let remote = spotify.inspect(id).await?;
        if remote.owner_id != spotify.owner_id()
            || !remote.description.lines().any(|line| line == marker)
        {
            bail!("Refusing to populate a playlist with unexpected owner or archive marker");
        }
        // Only unfinished drafts can be replaced. Completed broadcasts are immutable.
        if spotify.track_uris(id).await? != entry.desired_uris {
            spotify.replace_draft(id, &entry.desired_uris).await?;
        }
        if spotify.track_uris(id).await? != entry.desired_uris {
            bail!("Spotify contents did not match broadcast {key}; leaving draft for recovery");
        }
        let preview = spotify.preview(id).await?;
        let listing = serde_json::json!({
            "station":station, "name":remote.name, "url":format!("https://open.spotify.com/playlist/{id}"),
            "track_count":entry.desired_uris.len(), "last_updated":Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
            "broadcast_start":show.start_time, "broadcast_end":show.end_time, "source_url":show.url,
            "preview":preview,
        });
        let entry = self.entries.get_mut(&key).unwrap();
        entry.listing = listing;
        entry.state = State::Ready;
        self.save(path)?;
        Ok(true)
    }

    /// This persisted intent must be committed remotely before applying the plan.
    /// Only new broadcasts are selected; imported legacy playlists are untouched.
    pub fn prepare_release(&mut self, path: &Path, owner: &str) -> Result<ReleasePlan> {
        let attempt = uuid::Uuid::new_v4().to_string();
        let playlists = self
            .entries
            .iter()
            .filter(|(_, e)| e.state == State::Ready)
            .map(|(key, e)| {
                if e.owner_id.as_deref() != Some(owner) {
                    bail!("Release owner does not match catalog");
                }
                Ok(ReleaseItem {
                    key: key.clone(),
                    playlist_id: e.playlist_id.clone().context("Missing playlist ID")?,
                    name: e.listing["name"].as_str().unwrap_or("").into(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        for item in &playlists {
            let e = self.entries.get_mut(&item.key).unwrap();
            e.state = State::ReleaseAttempted;
            e.release_attempt = Some(attempt.clone());
        }
        self.save(path)?;
        Ok(ReleasePlan {
            attempt,
            owner_id: owner.into(),
            playlists,
        })
    }

    pub async fn apply_release(
        &mut self,
        path: &Path,
        plan_path: &Path,
        spotify: &impl ArchiveSpotify,
    ) -> Result<()> {
        let plan: ReleasePlan = serde_json::from_slice(&fs::read(plan_path)?)?;
        if plan.owner_id != spotify.owner_id() {
            bail!("Release plan belongs to a different Spotify account");
        }
        for item in &plan.playlists {
            let entry = self
                .entries
                .get(&item.key)
                .context("Release plan refers to an unknown entry")?;
            if entry.state != State::ReleaseAttempted
                || entry.release_attempt.as_deref() != Some(&plan.attempt)
                || entry.playlist_id.as_deref() != Some(&item.playlist_id)
                || entry.owner_id.as_deref() != Some(spotify.owner_id())
            {
                bail!("Stale or inconsistent release plan");
            }
        }
        // Consume locally before any network mutation. A workflow rerun starts
        // with the committed attempted state and produces no new plan for these IDs.
        let consumed = plan_path.with_extension("consumed.json");
        if consumed.exists() {
            bail!("This release plan was already attempted");
        }
        fs::rename(plan_path, &consumed)?;
        let mut failures = Vec::new();
        for item in plan.playlists {
            let result = async {
                let remote = spotify.inspect(&item.playlist_id).await?;
                if remote.owner_id != spotify.owner_id() {
                    bail!("Playlist ownership changed");
                }
                spotify.remove_from_library(&item.playlist_id).await
            }
            .await;
            match result {
                Ok(()) => {
                    self.entries.get_mut(&item.key).unwrap().state = State::Released;
                    self.save(path)?;
                    eprintln!("Removed new broadcast from library: {}", item.name);
                }
                Err(error) => failures.push(format!("{}: {error}", item.playlist_id)),
            }
        }
        if !failures.is_empty() {
            bail!(
                "Some removals need review; they will not be retried automatically:\n{}",
                failures.join("\n")
            );
        }
        Ok(())
    }
}
