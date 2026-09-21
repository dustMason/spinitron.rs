//! Broadcast archives retain their Spotify library membership.
//!
//! The catalog permanently records each completed broadcast, including older
//! archives outside the library. Archiving never follows or unfollows playlists.
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
    // Retained to read catalogs written before automatic removal was retired.
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

#[derive(Debug, Clone)]
pub struct RemotePlaylist {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub description: String,
}

impl RemotePlaylist {
    pub fn matches_marker(&self, marker: &str) -> bool {
        // New descriptions are one line. Also recognize earlier multiline
        // descriptions when reconciling a playlist from an interrupted run.
        self.description.lines().any(|line| line == marker)
            || self
                .description
                .split_once(" | ")
                .is_some_and(|(prefix, _)| prefix == marker)
    }
}

fn archive_description(marker: &str, show: &Show) -> String {
    // Spotify rejects line breaks in descriptions with a generic HTTP 400.
    // The catalog retains the full source URL and broadcast timestamps.
    format!("{marker} | Broadcast: {} | {}", show.start_time, show.url)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(300)
        .collect()
}

/// Spotify explicitly rejected a creation request without creating a playlist.
/// Transport failures and server errors must never use this classification.
#[derive(Debug)]
pub struct CreationRejected(pub String);

impl std::fmt::Display for CreationRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CreationRejected {}

pub trait ArchiveSpotify {
    fn owner_id(&self) -> &str;
    async fn resolve(&mut self, tracks: &[Track]) -> Result<Vec<String>>;
    async fn find_archive(&mut self, marker: &str) -> Result<Option<RemotePlaylist>>;
    async fn create_archive(&mut self, name: &str, description: &str) -> Result<RemotePlaylist>;
    async fn replace_draft(&self, id: &str, uris: &[String]) -> Result<()>;
    async fn inspect(&self, id: &str) -> Result<RemotePlaylist>;
    async fn rename_archive(&self, id: &str, name: &str) -> Result<()>;
    async fn track_uris(&self, id: &str) -> Result<Vec<String>>;
    async fn preview(&self, id: &str) -> Result<Value>;
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

    pub fn listings(&self) -> Result<Vec<Value>> {
        let names = self.archive_names()?;
        let mut rows: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, e)| !matches!(e.state, State::Prepared | State::Creating | State::Filling))
            .map(|(key, e)| {
                let mut listing = e.listing.clone();
                if let Some(name) = names.get(key) {
                    // Share the exact naming policy with the website, even while
                    // a bounded migration is still catching up on Spotify.
                    listing["display_name"] = name.clone().into();
                }
                listing
            })
            .collect();
        rows.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        Ok(rows)
    }

    /// Names are computed over the whole catalog, including earlier broadcasts
    /// outside the scrape window. Reserve the suffix before truncating the title.
    pub fn archive_names(&self) -> Result<BTreeMap<String, String>> {
        let broadcasts = self
            .entries
            .iter()
            .filter_map(|(key, entry)| {
                entry
                    .broadcast
                    .as_ref()
                    .filter(|_| entry.state != State::Legacy)
                    .map(|show| (key, entry, show))
            })
            .collect::<Vec<_>>();
        let mut levels = BTreeMap::<&String, usize>::new();
        loop {
            let mut names = BTreeMap::new();
            let mut groups = BTreeMap::<String, Vec<&String>>::new();
            for (key, entry, show) in &broadcasts {
                let start = broadcast_time(&show.start_time)?;
                let level = *levels.get(key).unwrap_or(&0);
                let mut suffix = start.format(" - %Y-%m-%d").to_string();
                if level >= 1 {
                    suffix.push_str(&start.format(" %-I:%M%P").to_string());
                }
                if level >= 2 {
                    // The clock repeats during the autumn DST transition.
                    suffix.push_str(&start.format(" %z").to_string());
                }
                if level >= 3 {
                    suffix.push_str(&format!(" [{key}]"));
                }
                let room = 100usize
                    .checked_sub(suffix.chars().count())
                    .context("Archive name suffix exceeds Spotify's name limit")?;
                let title = show.title.replace("&amp;", "&").replace("&quot;", "\"");
                let prefix = format!(
                    "{} - {}",
                    entry.station,
                    title.split_whitespace().collect::<Vec<_>>().join(" ")
                );
                let prefix = prefix.chars().take(room).collect::<String>();
                let name = format!("{}{suffix}", prefix.trim_end());
                groups.entry(name.to_lowercase()).or_default().push(key);
                names.insert((*key).clone(), name);
            }
            let collisions = groups
                .values()
                .filter(|keys| keys.len() > 1)
                .collect::<Vec<_>>();
            if collisions.is_empty() {
                return Ok(names);
            }
            for keys in collisions {
                let next = keys
                    .iter()
                    .map(|key| *levels.get(key).unwrap_or(&0))
                    .max()
                    .unwrap()
                    + 1;
                if next > 3 {
                    bail!("Could not disambiguate archive names");
                }
                for key in keys {
                    levels.insert(key, next);
                }
            }
        }
    }

    /// Only name changes require Spotify requests. A lost response is reconciled
    /// with a read on the next run; completed tracks and library membership stay put.
    pub async fn sync_archive_names(
        &mut self,
        path: &Path,
        spotify: &impl ArchiveSpotify,
        limit: usize,
        interval: std::time::Duration,
    ) -> Result<usize> {
        let mut synced = 0;
        for (key, name) in self.archive_names()? {
            let entry = &self.entries[&key];
            if entry.listing["name"].as_str() == Some(&name) {
                continue;
            }
            if entry.owner_id.as_deref() != Some(spotify.owner_id()) {
                bail!("Archive name update belongs to a different Spotify account");
            }
            if let Some(id) = &entry.playlist_id {
                if synced == limit {
                    break;
                }
                let marker = format!("Spinitron archive: {key}");
                tokio::time::sleep(interval).await;
                let remote = spotify.inspect(id).await?;
                if remote.id != *id
                    || remote.owner_id != spotify.owner_id()
                    || !remote.matches_marker(&marker)
                {
                    bail!("Refusing to rename {key}: unexpected ID, owner, or archive marker");
                }
                if remote.name != name {
                    if entry.listing["name"].as_str() != Some(&remote.name) {
                        bail!("Name of {key} changed outside the catalog; review before renaming");
                    }
                    tokio::time::sleep(interval).await;
                    spotify.rename_archive(id, &name).await?;
                    tokio::time::sleep(interval).await;
                    let verified = spotify.inspect(id).await?;
                    if verified.id != *id
                        || verified.owner_id != remote.owner_id
                        || verified.description != remote.description
                        || verified.name != name
                    {
                        bail!("Spotify name update for {key} was not verified; retry will inspect first");
                    }
                }
                synced += 1;
                eprintln!("Archive name: {name}");
            } else if entry.state != State::Prepared {
                // Keep the old name while recovering an uncertain creation.
                continue;
            }
            self.entries.get_mut(&key).unwrap().listing["name"] = name.into();
            self.save(path)?;
        }
        Ok(synced)
    }

    /// Resume saved work even after its broadcast leaves the scraping window.
    pub async fn resume_pending(
        &mut self,
        path: &Path,
        spotify: &mut impl ArchiveSpotify,
    ) -> Vec<(String, Result<bool>)> {
        let pending: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, entry)| {
                matches!(
                    entry.state,
                    State::Prepared | State::Creating | State::Filling
                )
            })
            .filter_map(|(key, entry)| {
                entry
                    .broadcast
                    .clone()
                    .map(|show| (key.clone(), entry.station.clone(), show))
            })
            .collect();
        let mut results = Vec::new();
        for (key, station, show) in pending {
            // Existing entries already contain the scraped and matched sequence.
            let result = self.archive(path, spotify, &station, &show, &[]).await;
            results.push((key, result));
        }
        results
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
                eprintln!(
                    "Skipping {station} - {} ({}): no Spotify matches for {} listed tracks; no playlist created",
                    show.title, show.id, tracks.len()
                );
                // Leave it out of the catalog so corrected source metadata can
                // be reconsidered on a later run within the scraping window.
                return Ok(false);
            }
            self.entries.insert(
                key.clone(),
                Entry {
                    station: station.into(),
                    broadcast: Some(show.clone()),
                    state: State::Prepared,
                    owner_id: Some(spotify.owner_id().into()),
                    playlist_id: None,
                    desired_uris: uris,
                    listing: serde_json::json!({"station":station}),
                    release_attempt: None,
                },
            );
        }
        if self.entries[&key].state == State::Prepared {
            let name = self
                .archive_names()?
                .remove(&key)
                .context("Missing archive name")?;
            self.entries.get_mut(&key).unwrap().listing["name"] = name.into();
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
                    let result = spotify
                        .create_archive(name, &archive_description(&marker, show))
                        .await;
                    if result
                        .as_ref()
                        .is_err_and(|error| error.is::<CreationRejected>())
                    {
                        // An explicit rejection can be retried on a later run.
                        // Lost responses remain Creating to prevent duplicates.
                        self.entries.get_mut(&key).unwrap().state = State::Prepared;
                        self.save(path)?;
                    }
                    result?
                }
            };
            if remote.owner_id != spotify.owner_id() || !remote.matches_marker(&marker) {
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
        if remote.owner_id != spotify.owner_id() || !remote.matches_marker(&marker) {
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
        let imported_at = Utc::now();
        let listing = serde_json::json!({
            "station":station, "name":remote.name, "url":format!("https://open.spotify.com/playlist/{id}"),
            "track_count":entry.desired_uris.len(), "last_updated":imported_at.format("%Y-%m-%d %H:%M UTC").to_string(),
            "imported_at":imported_at.to_rfc3339(),
            "broadcast_start":show.start_time, "broadcast_end":show.end_time, "source_url":show.url,
            "preview":preview,
        });
        let entry = self.entries.get_mut(&key).unwrap();
        entry.listing = listing;
        entry.state = State::Ready;
        self.save(path)?;
        Ok(true)
    }
}
