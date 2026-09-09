use anyhow::{anyhow, Result};
use base64::{engine::general_purpose, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::catalog::{ArchiveSpotify, CreationRejected, RemotePlaylist};
use crate::models::{ShowGroup, Track};

const CACHE_DIR: &str = "spotify_cache";
const TRACK_CACHE_FILE: &str = "track_cache.json";

#[cfg(test)]
#[path = "spotify_sync_tests.rs"]
mod sync_tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotifyTrack {
    pub id: String,
    pub name: String,
    pub artists: Vec<SpotifyArtist>,
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotifyArtist {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotifyPlaylist {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub uri: String,
    pub external_url: Option<String>,
    pub track_count: u32,
}

#[derive(Debug)]
pub enum PlaylistUpdate {
    Created(SpotifyPlaylist),
    Updated(SpotifyPlaylist),
    Unchanged(SpotifyPlaylist),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotifyFolder {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedTrackEntry {
    track: Option<SpotifyTrack>,
    expires_at: u64, // Unix timestamp
}

#[derive(Debug, Serialize, Deserialize)]
struct TrackSearchCache {
    entries: HashMap<String, CachedTrackEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlaylistCache {
    playlists: HashMap<String, SpotifyPlaylist>, // Key is spinitron_id
}

pub struct SpotifyClient {
    client: Client,
    api_base_url: String,
    access_token: String,
    user_id: String,
    track_cache: TrackSearchCache,
    playlist_cache: PlaylistCache,
    cache_dir: String,
    total_cache_hits: u32,
    total_api_calls: u32,
    archive_inventory: Option<Vec<RemotePlaylist>>,
}

impl SpotifyClient {
    pub async fn new() -> Result<Self> {
        let client_id = std::env::var("SPOTIFY_CLIENT_ID")
            .map_err(|_| anyhow!("SPOTIFY_CLIENT_ID environment variable not set"))?;
        let client_secret = std::env::var("SPOTIFY_CLIENT_SECRET")
            .map_err(|_| anyhow!("SPOTIFY_CLIENT_SECRET environment variable not set"))?;
        let refresh_token = std::env::var("SPOTIFY_REFRESH_TOKEN")
            .map_err(|_| anyhow!("SPOTIFY_REFRESH_TOKEN environment variable not set. Run get_spotify_token.py to get one."))?;

        let client = Client::new();
        let cache_dir = CACHE_DIR.to_string();

        // Ensure cache directory exists
        if !Path::new(&cache_dir).exists() {
            fs::create_dir_all(&cache_dir)?;
        }

        // Get access token
        let access_token =
            Self::get_access_token(&client, &client_id, &client_secret, &refresh_token).await?;

        // Get user ID and verify permissions
        let user_id = Self::get_user_id(&client, &access_token).await?;

        let track_cache = Self::load_track_cache(&cache_dir);
        let playlist_cache = PlaylistCache {
            playlists: std::collections::HashMap::new(),
        };

        Ok(Self {
            client,
            api_base_url: "https://api.spotify.com/v1".into(),
            access_token,
            user_id,
            track_cache,
            playlist_cache,
            cache_dir,
            total_cache_hits: 0,
            total_api_calls: 0,
            archive_inventory: None,
        })
    }

    async fn get_access_token(
        client: &Client,
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
    ) -> Result<String> {
        let auth_header =
            general_purpose::STANDARD.encode(format!("{}:{}", client_id, client_secret));

        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ];

        let response = client
            .post("https://accounts.spotify.com/api/token")
            .header("Authorization", format!("Basic {}", auth_header))
            .form(&params)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;
        Self::parse_access_token_response(status, &body)
    }

    fn parse_access_token_response(status: reqwest::StatusCode, body: &str) -> Result<String> {
        // Never include the raw response: successful responses contain credentials.
        let json: Value = serde_json::from_str(body).map_err(|_| {
            anyhow!(
                "Spotify token refresh failed (HTTP {}): invalid JSON response",
                status
            )
        })?;

        if !status.is_success() {
            let error = json["error"].as_str().unwrap_or("unknown_error");
            let guidance = match error {
                "invalid_grant" => concat!(
                    "The refresh token is expired, revoked, or invalid. Reauthorize with ",
                    "python3 scripts/get_spotify_token.py, then replace SPOTIFY_REFRESH_TOKEN ",
                    "in your environment and GitHub Actions repository secrets. ",
                    "Spotify refresh tokens expire after 6 months; retrying this token will not renew it."
                ),
                "invalid_client" => concat!(
                    "Check SPOTIFY_CLIENT_ID and SPOTIFY_CLIENT_SECRET; they must belong ",
                    "to the same Spotify app used to obtain the refresh token."
                ),
                _ => "Spotify rejected the token refresh request.",
            };
            return Err(anyhow!(
                "Spotify token refresh failed (HTTP {}, {}): {}",
                status,
                error,
                guidance
            ));
        }

        json["access_token"]
            .as_str()
            .filter(|token| !token.is_empty())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                anyhow!(
                    "Spotify token response (HTTP {}) is missing a non-empty access_token",
                    status
                )
            })
    }

    async fn get_user_id(client: &Client, access_token: &str) -> Result<String> {
        let response = client
            .get("https://api.spotify.com/v1/me")
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await?;

        let json: Value = response.json().await?;

        json["id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("Failed to get user ID from Spotify"))
    }

    fn load_track_cache(cache_dir: &str) -> TrackSearchCache {
        let cache_path = format!("{}/{}", cache_dir, TRACK_CACHE_FILE);
        if let Ok(content) = fs::read_to_string(&cache_path) {
            if let Ok(cache) = serde_json::from_str(&content) {
                return cache;
            }
        }
        TrackSearchCache {
            entries: HashMap::new(),
        }
    }

    fn save_track_cache(&mut self) -> Result<()> {
        let cache_path = format!("{}/{}", self.cache_dir, TRACK_CACHE_FILE);
        let content = serde_json::to_string(&self.track_cache)?;
        fs::write(cache_path, content)?;
        Ok(())
    }

    pub fn purge_expired_cache_entries(&mut self) -> Result<()> {
        let current_time = Self::current_timestamp();
        let initial_count = self.track_cache.entries.len();
        self.track_cache
            .entries
            .retain(|_, entry| entry.expires_at > current_time);
        let expired_count = initial_count - self.track_cache.entries.len();

        if expired_count > 0 {
            println!("🗑️  Removed {} expired cache entries", expired_count);
            self.save_track_cache()?;
        }

        Ok(())
    }

    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    async fn search_track_with_cache_info(
        &mut self,
        track: &Track,
    ) -> Result<(Option<SpotifyTrack>, bool)> {
        let search_key = track.cache_key();

        // Check cache first
        if let Some(cached_entry) = self
            .track_cache
            .entries
            .get(&search_key)
            .filter(|entry| entry.expires_at > Self::current_timestamp())
        {
            self.total_cache_hits += 1;
            return Ok((cached_entry.track.clone(), false)); // false = no API call made
        }

        // Search Spotify
        let query = format!("track:{} artist:{}", track.song, track.artist);
        let encoded_query = urlencoding::encode(&query);

        let url = format!(
            "https://api.spotify.com/v1/search?q={}&type=track&limit=1",
            encoded_query
        );

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!(
                "Spotify search API error ({}): {}",
                status,
                error_text
            ));
        }

        let response_text = response.text().await?;
        let json: Value = serde_json::from_str(&response_text).map_err(|e| {
            anyhow!(
                "Failed to parse search JSON response: {}. Response body: {}",
                e,
                response_text
            )
        })?;

        let spotify_track = if let Some(tracks) = json["tracks"]["items"].as_array() {
            if let Some(track_data) = tracks.first() {
                Some(SpotifyTrack {
                    id: track_data["id"].as_str().unwrap_or("").to_string(),
                    name: track_data["name"].as_str().unwrap_or("").to_string(),
                    artists: track_data["artists"]
                        .as_array()
                        .unwrap_or(&vec![])
                        .iter()
                        .map(|artist| SpotifyArtist {
                            name: artist["name"].as_str().unwrap_or("").to_string(),
                        })
                        .collect(),
                    uri: track_data["uri"].as_str().unwrap_or("").to_string(),
                })
            } else {
                None
            }
        } else {
            None
        };

        // Cache the result with 14-day expiration
        let current_time = Self::current_timestamp();
        let expires_at = current_time + (14 * 24 * 60 * 60); // 14 days in seconds
        let cache_entry = CachedTrackEntry {
            track: spotify_track.clone(),
            expires_at,
        };
        self.track_cache.entries.insert(search_key, cache_entry);

        self.total_api_calls += 1;
        Ok((spotify_track, true)) // true = API call was made
    }

    pub async fn create_or_update_show_playlist(
        &mut self,
        show_group: &ShowGroup,
    ) -> Result<Option<PlaylistUpdate>> {
        let playlist_name = show_group.playlist_name();
        let description = show_group.description();
        let latest_id = show_group.latest_spinitron_id();
        let all_tracks = show_group.all_tracks();

        // Skip creating playlist if no tracks
        if all_tracks.is_empty() {
            println!("  ⚠️  Skipping playlist creation - no tracks found");
            return Ok(None);
        }

        // Check if playlist already exists by name
        let existing_playlist = self
            .playlist_cache
            .playlists
            .values()
            .find(|p| p.name == playlist_name)
            .cloned();

        let playlist = if let Some(existing) = existing_playlist {
            // Finish fallible track searches before removing existing music.
            let track_uris = self.resolve_track_uris(&all_tracks).await?;
            if track_uris.is_empty() {
                return Err(anyhow!(
                    "No Spotify tracks matched; preserving existing playlist"
                ));
            }
            let current_uris = self.get_playlist_tracks(&existing.id).await?;
            // Compare the complete ordered sequence, including repeated tracks.
            // A new scrape/date alone must not change Spotify's activity or our
            // website's Last updated date, which comes from the description.
            if current_uris == track_uris {
                return Ok(Some(PlaylistUpdate::Unchanged(existing)));
            }
            self.clear_playlist_tracks(&existing.id, &current_uris)
                .await?;

            self.add_track_uris_to_playlist(&existing.id, &track_uris)
                .await?;

            // Update the playlist description with new latest ID
            self.update_playlist_description(&existing.id, &description)
                .await?;

            let mut updated_existing = existing;
            updated_existing.description = Some(description);
            updated_existing.track_count = track_uris.len() as u32;

            // Update in-memory cache
            self.playlist_cache
                .playlists
                .insert(latest_id.to_string(), updated_existing.clone());

            Some(PlaylistUpdate::Updated(updated_existing))
        } else {
            if playlist_name.is_empty() {
                return Err(anyhow!("Playlist name cannot be empty"));
            }
            if playlist_name.len() > 100 {
                return Err(anyhow!(
                    "Playlist name too long: {} characters (max 100)",
                    playlist_name.len()
                ));
            }
            if description.len() > 300 {
                return Err(anyhow!(
                    "Description too long: {} characters (max 300)",
                    description.len()
                ));
            }

            // Do not create an empty playlist if track lookup fails.
            let track_uris = self.resolve_track_uris(&all_tracks).await?;
            if track_uris.is_empty() {
                return Err(anyhow!("No Spotify tracks matched; no playlist created"));
            }
            // Create new playlist
            let playlist_data = serde_json::json!({
                "name": playlist_name,
                "description": description,
                "public": true
            });
            let playlist_json = self
                .archive_request(reqwest::Method::POST, "me/playlists", Some(playlist_data))
                .await?;

            let playlist = SpotifyPlaylist {
                id: playlist_json["id"].as_str().unwrap_or("").to_string(),
                name: playlist_json["name"].as_str().unwrap_or("").to_string(),
                description: playlist_json["description"].as_str().map(|s| s.to_string()),
                uri: playlist_json["uri"].as_str().unwrap_or("").to_string(),
                external_url: playlist_json["external_urls"]["spotify"]
                    .as_str()
                    .map(|s| s.to_string()),
                track_count: 0, // Will be updated after tracks are added
            };

            self.add_track_uris_to_playlist(&playlist.id, &track_uris)
                .await?;
            let mut updated_playlist = playlist.clone();
            updated_playlist.track_count = track_uris.len() as u32;

            // Cache the playlist in memory
            self.playlist_cache
                .playlists
                .insert(latest_id.to_string(), updated_playlist.clone());

            Some(PlaylistUpdate::Created(updated_playlist))
        };

        Ok(playlist)
    }

    async fn update_playlist_description(
        &self,
        playlist_id: &str,
        description: &str,
    ) -> Result<()> {
        let update_data = serde_json::json!({
            "description": description
        });

        let response = self
            .client
            .put(&format!("{}/playlists/{}", self.api_base_url, playlist_id))
            .header("Authorization", format!("Bearer {}", self.access_token))
            .header("Content-Type", "application/json")
            .json(&update_data)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!(
                "Failed to update playlist description: {}",
                error_text
            ));
        }

        Ok(())
    }

    async fn resolve_track_uris(&mut self, tracks: &[Track]) -> Result<Vec<String>> {
        let mut track_uris = Vec::new();
        let mut found_tracks = 0;
        let mut not_found_tracks = 0;
        let mut api_calls_made = 0;
        let mut cache_hits = 0;

        // For very large playlists, limit to first 5000 tracks to avoid timeouts
        let tracks_to_process = if tracks.len() > 5000 {
            &tracks[..5000]
        } else {
            tracks
        };

        for track in tracks_to_process.iter() {
            let (result, made_api_call) = self.search_track_with_cache_info(track).await?;

            match result {
                Some(spotify_track) => {
                    track_uris.push(spotify_track.uri);
                    found_tracks += 1;
                }
                None => {
                    not_found_tracks += 1;
                }
            }

            // Track cache hits vs API calls
            if made_api_call {
                api_calls_made += 1;
                if api_calls_made % 10 == 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            } else {
                cache_hits += 1;
            }
        }

        if api_calls_made > 0 {
            self.save_track_cache()?;
        }

        println!(
            "Track search complete: {} found, {} not found ({} cache hits, {} API calls)",
            found_tracks, not_found_tracks, cache_hits, api_calls_made
        );

        Ok(track_uris)
    }

    async fn add_track_uris_to_playlist(
        &self,
        playlist_id: &str,
        track_uris: &[String],
    ) -> Result<()> {
        if !track_uris.is_empty() {
            for (i, chunk) in track_uris.chunks(100).enumerate() {
                let add_tracks_data = serde_json::json!({
                    "uris": chunk
                });

                let response = self
                    .client
                    .post(&format!(
                        "{}/playlists/{}/tracks",
                        self.api_base_url, playlist_id
                    ))
                    .header("Authorization", format!("Bearer {}", self.access_token))
                    .header("Content-Type", "application/json")
                    .json(&add_tracks_data)
                    .send()
                    .await?;

                if !response.status().is_success() {
                    let error_text = response.text().await?;
                    return Err(anyhow!(
                        "Failed to add tracks batch {}: {}",
                        i + 1,
                        error_text
                    ));
                }
            }
        }

        Ok(())
    }

    async fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Vec<String>> {
        let mut all_track_uris = Vec::new();
        let mut url = Some(format!(
            "{}/playlists/{}/tracks?limit=100",
            self.api_base_url, playlist_id
        ));

        while let Some(current_url) = url {
            let response = self
                .client
                .get(&current_url)
                .header("Authorization", format!("Bearer {}", self.access_token))
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                return Err(anyhow!("Failed to get playlist tracks: {}", error_text));
            }

            let json: serde_json::Value = response.json().await?;

            let items = json["items"].as_array().ok_or_else(|| {
                anyhow!("Cannot compare playlist contents: Spotify response is missing items")
            })?;
            for item in items {
                let uri = item["track"]["uri"]
                    .as_str()
                    .filter(|uri| !uri.is_empty())
                    .ok_or_else(|| {
                        anyhow!("Cannot compare playlist contents: track URI unavailable")
                    })?;
                all_track_uris.push(uri.to_string());
            }

            url = match json.get("next") {
                Some(Value::Null) => None,
                Some(Value::String(next)) if !next.is_empty() => Some(next.clone()),
                _ => {
                    return Err(anyhow!(
                        "Cannot compare playlist contents: invalid pagination"
                    ))
                }
            };
        }

        Ok(all_track_uris)
    }

    /// Fetch up to `limit` tracks for a preview (track name, artists, and largest album image URL).
    pub async fn get_playlist_preview(
        &self,
        playlist_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, Vec<String>, String)>> {
        let url = format!(
            "https://api.spotify.com/v1/playlists/{}/tracks?limit={}&fields=items(track(name,artists(name),album(images)))",
            playlist_id, limit
        );
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await?;
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Spotify preview API error: {}", error_text));
        }
        let json: Value = response.json().await?;
        let mut previews = Vec::new();
        if let Some(items) = json["items"].as_array() {
            for item in items {
                let track = &item["track"];
                let name = track["name"].as_str().unwrap_or("").to_string();
                let artists = track["artists"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                    .collect();
                let image_url = track["album"]["images"]
                    .as_array()
                    .and_then(|imgs| imgs.first())
                    .and_then(|img| img["url"].as_str())
                    .unwrap_or("")
                    .to_string();
                previews.push((name, artists, image_url));
            }
        }
        Ok(previews)
    }

    async fn clear_playlist_tracks(&self, playlist_id: &str, track_uris: &[String]) -> Result<()> {
        if track_uris.is_empty() {
            return Ok(());
        }

        // Remove tracks in batches of 100 (Spotify limit)
        for (i, chunk) in track_uris.chunks(100).enumerate() {
            let tracks_to_remove: Vec<serde_json::Value> = chunk
                .iter()
                .map(|uri| serde_json::json!({"uri": uri}))
                .collect();

            let remove_tracks_data = serde_json::json!({
                "tracks": tracks_to_remove
            });

            let response = self
                .client
                .delete(&format!(
                    "{}/playlists/{}/tracks",
                    self.api_base_url, playlist_id
                ))
                .header("Authorization", format!("Bearer {}", self.access_token))
                .header("Content-Type", "application/json")
                .json(&remove_tracks_data)
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                return Err(anyhow!(
                    "Failed to remove tracks batch {}: {}",
                    i + 1,
                    error_text
                ));
            }
        }

        Ok(())
    }

    pub async fn refresh_playlist_cache(&mut self) -> Result<()> {
        let mut offset = 0;
        let limit = 50;
        let mut all_playlists = Vec::new();

        loop {
            let url = format!(
                "https://api.spotify.com/v1/me/playlists?limit={}&offset={}",
                limit, offset
            );

            let response = self
                .client
                .get(&url)
                .header("Authorization", format!("Bearer {}", self.access_token))
                .send()
                .await?;

            let status = response.status();
            if !status.is_success() {
                let error_text = response.text().await?;
                return Err(anyhow!("Spotify API error ({}): {}", status, error_text));
            }

            let response_text = response.text().await?;
            let json: Value = serde_json::from_str(&response_text).map_err(|e| {
                anyhow!(
                    "Failed to parse JSON response: {}. Response body: {}",
                    e,
                    response_text
                )
            })?;

            if let Some(items) = json["items"].as_array() {
                for item in items {
                    // Skip playlists without proper names to avoid hash collisions
                    let Some(playlist_name) = item["name"].as_str() else {
                        continue;
                    };

                    if let Some(description) = item["description"].as_str() {
                        let has_generated =
                            description.contains("Generated from Spinitron playlists");
                        let has_latest_id = description.contains("Latest ID:");

                        if has_generated || has_latest_id {
                            // Extract ID from "Latest ID: " format
                            let spinitron_id =
                                if let Some(id_str) = description.split("Latest ID: ").nth(1) {
                                    id_str.split_whitespace().next().unwrap_or("0").to_string()
                                } else {
                                    // Fallback: use a hash of the playlist name for unique identification
                                    use std::collections::hash_map::DefaultHasher;
                                    use std::hash::{Hash, Hasher};
                                    let mut hasher = DefaultHasher::new();
                                    playlist_name.hash(&mut hasher);
                                    hasher.finish().to_string()
                                };

                            let track_count = item["tracks"]["total"].as_u64().unwrap_or(0) as u32;

                            let playlist = SpotifyPlaylist {
                                id: item["id"].as_str().unwrap_or("").to_string(),
                                name: playlist_name.to_string(),
                                description: Some(description.to_string()),
                                uri: item["uri"].as_str().unwrap_or("").to_string(),
                                external_url: item["external_urls"]["spotify"]
                                    .as_str()
                                    .map(|s| s.to_string()),
                                track_count,
                            };
                            all_playlists.push((spinitron_id, playlist));
                        }
                    }
                }

                if items.len() < limit {
                    break;
                }
                offset += limit;
            } else {
                break;
            }
        }

        // Update cache
        self.playlist_cache.playlists.clear();
        for (spinitron_id, playlist) in all_playlists {
            self.playlist_cache.playlists.insert(spinitron_id, playlist);
        }

        Ok(())
    }

    pub fn get_cached_playlists(&self) -> &std::collections::HashMap<String, SpotifyPlaylist> {
        &self.playlist_cache.playlists
    }

    pub fn restrict_to_playlists(&mut self, playlist_ids: &[String]) -> Result<HashSet<String>> {
        let requested: HashSet<_> = playlist_ids.iter().collect();
        if requested.is_empty() {
            return Err(anyhow!("At least one existing playlist ID is required"));
        }
        let selected: Vec<_> = self
            .playlist_cache
            .playlists
            .values()
            .filter(|playlist| requested.contains(&playlist.id))
            .collect();
        let found: HashSet<_> = selected.iter().map(|playlist| &playlist.id).collect();
        if found != requested {
            return Err(anyhow!(
                "Some selected playlist IDs were not found in this account's Spinitron playlists"
            ));
        }
        let names: HashSet<_> = selected
            .iter()
            .map(|playlist| playlist.name.clone())
            .collect();
        if names.len() != selected.len() {
            return Err(anyhow!("Select only one playlist ID per show name"));
        }
        self.playlist_cache
            .playlists
            .retain(|_, playlist| requested.contains(&playlist.id));
        Ok(names)
    }

    pub fn get_cache_stats(&self) -> (u32, u32) {
        (self.total_cache_hits, self.total_api_calls)
    }
}

impl SpotifyClient {
    pub async fn legacy_library_cleanup_plan(
        &self,
        catalog: &crate::catalog::Catalog,
    ) -> Result<Value> {
        let legacy: HashSet<_> = catalog
            .entries
            .values()
            .filter(|e| e.state == crate::catalog::State::Legacy)
            .filter_map(|e| e.playlist_id.as_deref())
            .collect();
        let mut playlists = Vec::new();
        let mut offset = 0;
        loop {
            let value = self
                .archive_request(
                    reqwest::Method::GET,
                    &format!("me/playlists?limit=50&offset={offset}"),
                    None,
                )
                .await?;
            let items = value["items"]
                .as_array()
                .ok_or_else(|| anyhow!("Spotify library response is missing items"))?;
            for item in items {
                if item["owner"]["id"].as_str() == Some(&self.user_id)
                    && item["id"].as_str().is_some_and(|id| legacy.contains(id))
                {
                    let p = Self::archive_metadata(item)?;
                    playlists.push(serde_json::json!({"playlist_id":p.id,"name":p.name,
                        "url":format!("https://open.spotify.com/playlist/{}",p.id),
                        "track_count":item["tracks"]["total"],"action":"remove_from_library_only"}));
                }
            }
            if items.len() < 50 {
                break;
            }
            offset += 50;
        }
        playlists.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        Ok(
            serde_json::json!({"created_at":chrono::Utc::now().to_rfc3339(),"owner_id":self.user_id,
            "read_only_plan":true,"count":playlists.len(),"playlists":playlists,
            "note":"Existing catalog playlists currently owned and saved by this account. Review which to retain before any library cleanup. No playlist contents will be changed."}),
        )
    }

    /// Exercise only a newly created test playlist; the source playlist is read-only.
    pub async fn verify_archive_flow(
        &mut self,
        source_id: &str,
        receipt_path: &Path,
    ) -> Result<()> {
        let source = self
            .archive_request(
                reqwest::Method::GET,
                &format!("playlists/{source_id}/tracks?limit=1&fields=items(track(uri))"),
                None,
            )
            .await?;
        let uri = source["items"][0]["track"]["uri"]
            .as_str()
            .filter(|uri| uri.starts_with("spotify:track:"))
            .ok_or_else(|| anyhow!("Verification source has no available track"))?
            .to_string();
        // Check membership-read access before creating anything.
        self.library_contains(source_id).await?;
        let marker = format!("Spinitron archive: verification:{}", uuid::Uuid::new_v4());
        let playlist = self
            .create_archive("Spinitron archive verification (temporary)", &marker)
            .await?;
        if playlist.owner_id != self.user_id {
            return Err(anyhow!("Verification playlist has unexpected owner"));
        }
        let id = &playlist.id;
        eprintln!("Verifying temporary playlist {id}");
        let result = async {
            crate::catalog::atomic_json(receipt_path, &serde_json::json!({
                "spotify_user_id":self.user_id,"test_playlist_id":id,"verification_status":"started"
            }))?;
            self.replace_draft(id, &[uri.clone()]).await?;
            if !self.library_contains(id).await? { return Err(anyhow!("Created playlist was not saved to the library")); }
            self.remove_from_library(id).await?;
            if self.library_contains(id).await? { return Err(anyhow!("Playlist is still in the library after removal")); }
            if self.inspect(id).await?.owner_id != self.user_id || self.track_uris(id).await? != [uri.clone()] {
                return Err(anyhow!("Playlist did not remain owned and readable after library removal"));
            }
            self.archive_request(reqwest::Method::PUT, &format!("playlists/{id}/followers"), Some(serde_json::json!({"public":true}))).await?;
            if !self.library_contains(id).await? { return Err(anyhow!("Could not save the archived playlist back to the library")); }
            Ok(serde_json::json!({"verified_at":chrono::Utc::now().to_rfc3339(),"spotify_user_id":self.user_id,
                "test_playlist_id":id,"removal_preserves_playlist":true,"readable_after_removal":true,"can_save_again":true}))
        }.await;
        // Only this freshly created test object is cleaned up. Try every cleanup
        // operation even if an earlier check failed, and report incomplete cleanup.
        let mut cleanup_errors = Vec::new();
        if let Err(error) = self
            .archive_request(
                reqwest::Method::PUT,
                &format!("playlists/{id}"),
                Some(serde_json::json!({"public":false})),
            )
            .await
        {
            cleanup_errors.push(error.to_string());
        }
        if let Err(error) = self
            .archive_request(
                reqwest::Method::PUT,
                &format!("playlists/{id}/tracks"),
                Some(serde_json::json!({"uris":[]})),
            )
            .await
        {
            cleanup_errors.push(error.to_string());
        }
        if let Err(error) = self.remove_from_library(id).await {
            cleanup_errors.push(error.to_string());
        }
        // Read back the final state even after an uncertain write. Preserve the
        // test result separately so a cleanup error cannot erase useful evidence.
        let cleanup = async {
            let value = self
                .archive_request(reqwest::Method::GET, &format!("playlists/{id}"), None)
                .await?;
            let saved = self.library_contains(id).await?;
            Ok::<_, anyhow::Error>(serde_json::json!({
                "saved":saved,"public":value["public"],"track_count":value["tracks"]["total"],
                "verified":!saved && value["public"] == false && value["tracks"]["total"] == 0
                    && value["owner"]["id"] == self.user_id,
            }))
        }
        .await;
        let cleanup = match cleanup {
            Ok(value) => value,
            Err(error) => {
                cleanup_errors.push(error.to_string());
                serde_json::json!({"verified":false})
            }
        };
        let mut receipt = match &result {
            Ok(receipt) => receipt.clone(),
            Err(error) => serde_json::json!({
                "verified_at":chrono::Utc::now().to_rfc3339(),"spotify_user_id":self.user_id,
                "test_playlist_id":id,"verification_error":error.to_string(),
            }),
        };
        receipt["verification_status"] = if result.is_ok() { "passed" } else { "failed" }.into();
        receipt["cleanup"] = cleanup.clone();
        receipt["cleanup_errors"] = serde_json::json!(cleanup_errors);
        crate::catalog::atomic_json(receipt_path, &receipt)?;
        if cleanup["verified"] != true {
            return Err(anyhow!(
                "Verification cleanup needs attention for {id}; see {}. Verification: {:?}",
                receipt_path.display(),
                result.as_ref().map(|_| "passed").map_err(|e| e.to_string())
            ));
        }
        result.map(|_| ())
    }

    async fn library_contains(&self, id: &str) -> Result<bool> {
        let encoded = urlencoding::encode(&self.user_id);
        self.archive_request(
            reqwest::Method::GET,
            &format!("playlists/{id}/followers/contains?ids={encoded}"),
            None,
        )
        .await?
        .get(0)
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("Invalid Spotify library-membership response"))
    }

    async fn archive_request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        for attempt in 0..3 {
            // Only reads can be repeated. A lost write response is ambiguous.
            let can_retry = method == reqwest::Method::GET && attempt < 2;
            let mut request = self
                .client
                .request(method.clone(), format!("{}/{path}", self.api_base_url))
                .bearer_auth(&self.access_token)
                .timeout(std::time::Duration::from_secs(30));
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) if can_retry && (error.is_timeout() || error.is_connect()) => {
                    tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let status = response.status();
            if !status.is_success() {
                let delay = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1 << attempt);
                if can_retry
                    && (status.is_server_error()
                        || status == reqwest::StatusCode::TOO_MANY_REQUESTS)
                    && delay <= 30
                {
                    eprintln!("Retrying Spotify GET {path} in {delay}s (HTTP {status})");
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    continue;
                }
                let text = response.text().await.unwrap_or_default();
                let detail = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|json| json["error"]["message"].as_str().map(str::to_owned))
                    .map(|message| {
                        message
                            .replace(&self.access_token, "[REDACTED]")
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(300)
                            .collect::<String>()
                    })
                    .filter(|message| !message.is_empty())
                    .map(|message| format!(": {message}"))
                    .unwrap_or_default();
                let message = format!("Spotify {method} {path} failed (HTTP {status}){detail}");
                if method == reqwest::Method::POST
                    && (path == "me/playlists"
                        || path == format!("users/{}/playlists", self.user_id))
                    && matches!(status.as_u16(), 400 | 401 | 403 | 404 | 405 | 422 | 429)
                {
                    return Err(CreationRejected(message).into());
                }
                return Err(anyhow!(message));
            }
            let text = match response.text().await {
                Ok(text) => text,
                Err(error) if can_retry && (error.is_timeout() || error.is_body()) => {
                    tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            return if text.is_empty() {
                Ok(Value::Null)
            } else {
                Ok(serde_json::from_str(&text)?)
            };
        }
        unreachable!("last attempt returns without retrying")
    }

    fn archive_metadata(value: &Value) -> Result<RemotePlaylist> {
        let required = |field: &Value, name: &str| {
            field
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| anyhow!("Spotify playlist response is missing {name}"))
        };
        Ok(RemotePlaylist {
            id: required(&value["id"], "id")?,
            owner_id: required(&value["owner"]["id"], "owner.id")?,
            name: required(&value["name"], "name")?,
            description: value["description"].as_str().unwrap_or("").into(),
        })
    }
}

impl ArchiveSpotify for SpotifyClient {
    fn owner_id(&self) -> &str {
        &self.user_id
    }

    async fn resolve(&mut self, tracks: &[Track]) -> Result<Vec<String>> {
        if tracks.len() > 5000 {
            return Err(anyhow!(
                "Broadcast exceeds 5000 tracks; refusing to truncate the archive"
            ));
        }
        self.resolve_track_uris(tracks).await
    }

    async fn find_archive(&mut self, marker: &str) -> Result<Option<RemotePlaylist>> {
        if self.archive_inventory.is_none() {
            let mut inventory = Vec::new();
            let mut offset = 0;
            loop {
                let response = self
                    .archive_request(
                        reqwest::Method::GET,
                        &format!("me/playlists?limit=50&offset={offset}"),
                        None,
                    )
                    .await?;
                let items = response["items"]
                    .as_array()
                    .ok_or_else(|| anyhow!("Spotify library response is missing items"))?;
                for item in items {
                    if item["owner"]["id"].as_str() == Some(&self.user_id)
                        && item["description"]
                            .as_str()
                            .is_some_and(|s| s.starts_with("Spinitron archive: "))
                    {
                        inventory.push(Self::archive_metadata(item)?);
                    }
                }
                if items.len() < 50 {
                    break;
                }
                offset += 50;
            }
            self.archive_inventory = Some(inventory);
        }
        let matches: Vec<_> = self
            .archive_inventory
            .as_ref()
            .unwrap()
            .iter()
            .filter(|p| p.matches_marker(marker))
            .collect();
        if matches.len() > 1 {
            return Err(anyhow!("Multiple playlists have archive marker {marker}; choose the correct ID before continuing"));
        }
        Ok(matches.first().map(|p| (*p).clone()))
    }

    async fn create_archive(&mut self, name: &str, description: &str) -> Result<RemotePlaylist> {
        let value = self
            .archive_request(
                reqwest::Method::POST,
                "me/playlists",
                Some(serde_json::json!({
                    "name":name, "description":description, "public":true,
                })),
            )
            .await?;
        let playlist = Self::archive_metadata(&value)?;
        if let Some(inventory) = &mut self.archive_inventory {
            inventory.push(playlist.clone());
        }
        Ok(playlist)
    }

    async fn replace_draft(&self, id: &str, uris: &[String]) -> Result<()> {
        if uris.is_empty() {
            return Err(anyhow!("Refusing to publish an empty broadcast"));
        }
        self.archive_request(
            reqwest::Method::PUT,
            &format!("playlists/{id}/tracks"),
            Some(serde_json::json!({"uris":&uris[..uris.len().min(100)]})),
        )
        .await?;
        for chunk in uris.get(100..).unwrap_or_default().chunks(100) {
            self.archive_request(
                reqwest::Method::POST,
                &format!("playlists/{id}/tracks"),
                Some(serde_json::json!({"uris":chunk})),
            )
            .await?;
        }
        Ok(())
    }

    async fn inspect(&self, id: &str) -> Result<RemotePlaylist> {
        Self::archive_metadata(
            &self
                .archive_request(reqwest::Method::GET, &format!("playlists/{id}"), None)
                .await?,
        )
    }

    async fn track_uris(&self, id: &str) -> Result<Vec<String>> {
        self.get_playlist_tracks(id).await
    }

    async fn preview(&self, id: &str) -> Result<Value> {
        let preview = self.get_playlist_preview(id, 12).await?.into_iter()
            .map(|(name,artists,image_url)| serde_json::json!({"name":name,"artists":artists,"image_url":image_url})).collect::<Vec<_>>();
        Ok(serde_json::json!(preview))
    }

    async fn remove_from_library(&self, id: &str) -> Result<()> {
        // Removing library membership leaves the public playlist available by ID.
        // Never retry this write automatically: the user may have re-saved it.
        self.archive_request(
            reqwest::Method::DELETE,
            &format!("playlists/{id}/followers"),
            None,
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{PlaylistCache, SpotifyClient, SpotifyPlaylist, TrackSearchCache};
    use crate::models::{Show, ShowEpisode, ShowGroup, Track};
    use reqwest::StatusCode;

    pub(super) fn offline_client() -> SpotifyClient {
        SpotifyClient {
            client: reqwest::Client::builder()
                .proxy(reqwest::Proxy::all("http://127.0.0.1:0").unwrap())
                .timeout(std::time::Duration::from_secs(1))
                .build()
                .unwrap(),
            api_base_url: "https://api.spotify.com/v1".into(),
            access_token: "test-token".into(),
            user_id: "test-user".into(),
            track_cache: TrackSearchCache {
                entries: Default::default(),
            },
            playlist_cache: PlaylistCache {
                playlists: Default::default(),
            },
            cache_dir: String::new(),
            total_cache_hits: 0,
            total_api_calls: 0,
            archive_inventory: None,
        }
    }

    fn playlist(id: &str, name: &str) -> SpotifyPlaylist {
        SpotifyPlaylist {
            id: id.into(),
            name: name.into(),
            description: None,
            uri: String::new(),
            external_url: None,
            track_count: 42,
        }
    }

    #[test]
    fn selection_requires_known_unambiguous_ids_and_excludes_other_playlists() {
        let mut spotify = offline_client();
        for (id, name) in [
            ("a", "KALX - FREEFORM"),
            ("b", "KALX - FREEFORM"),
            ("c", "KPOO - More Overnight"),
        ] {
            spotify
                .playlist_cache
                .playlists
                .insert(id.into(), playlist(id, name));
        }
        for invalid in [vec![], vec!["missing".into()], vec!["a".into(), "b".into()]] {
            assert!(spotify.restrict_to_playlists(&invalid).is_err());
            assert_eq!(spotify.playlist_cache.playlists.len(), 3);
        }
        let names = spotify
            .restrict_to_playlists(&["a".into(), "c".into()])
            .unwrap();
        assert_eq!(names.len(), 2);
        assert!(names.contains("KALX - FREEFORM"));
        assert!(names.contains("KPOO - More Overnight"));
        assert_eq!(spotify.playlist_cache.playlists.len(), 2);
        assert!(!spotify.playlist_cache.playlists.contains_key("b"));
    }

    #[tokio::test]
    async fn failed_track_lookup_precedes_any_playlist_request() {
        let show = ShowGroup {
            station: "KALX".into(),
            show_name: "Test Show".into(),
            episodes: vec![ShowEpisode {
                show: Show {
                    id: 1,
                    title: "Test Show".into(),
                    url: String::new(),
                    start_time: String::new(),
                    end_time: String::new(),
                },
                tracks: vec![Track {
                    artist: "Test Artist".into(),
                    song: "Test Song".into(),
                    album: String::new(),
                    label: None,
                    time: None,
                }],
            }],
        };

        for existing in [true, false] {
            // Port zero cannot host a proxy: every request fails locally, and
            // the error URL identifies which operation was attempted first.
            // No real Spotify credentials or external API calls are used.
            let mut spotify = offline_client();
            if existing {
                spotify.playlist_cache.playlists.insert(
                    "1".into(),
                    playlist("existing-playlist", &show.playlist_name()),
                );
            }

            let error = spotify
                .create_or_update_show_playlist(&show)
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("https://api.spotify.com/v1/search?"),
                "playlist request happened before track lookup (existing={existing}): {error}"
            );
        }
    }

    #[test]
    fn accepts_successful_token_response() {
        let token = SpotifyClient::parse_access_token_response(
            StatusCode::OK,
            r#"{"access_token":"test-access-token","expires_in":3600}"#,
        )
        .unwrap();
        assert_eq!(token, "test-access-token");
    }

    #[test]
    fn expired_refresh_token_explains_how_to_reauthorize() {
        let error = SpotifyClient::parse_access_token_response(
            StatusCode::BAD_REQUEST,
            r#"{"error":"invalid_grant","error_description":"Refresh token expired"}"#,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("400"));
        assert!(error.contains("invalid_grant"));
        assert!(error.contains("scripts/get_spotify_token.py"));
        assert!(error.contains("SPOTIFY_REFRESH_TOKEN"));
        assert!(error.contains("GitHub Actions repository secrets"));
    }

    #[test]
    fn invalid_client_points_to_app_credentials() {
        let error = SpotifyClient::parse_access_token_response(
            StatusCode::UNAUTHORIZED,
            r#"{"error":"invalid_client"}"#,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("401"));
        assert!(error.contains("SPOTIFY_CLIENT_ID"));
        assert!(error.contains("SPOTIFY_CLIENT_SECRET"));
    }

    #[test]
    fn non_json_error_preserves_http_status_without_body() {
        let error = SpotifyClient::parse_access_token_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "<html>private upstream details</html>",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("503"));
        assert!(!error.contains("private upstream details"));
    }

    #[test]
    fn rejects_missing_or_empty_access_token_without_exposing_response() {
        for body in [
            r#"{"refresh_token":"secret-refresh-token"}"#,
            r#"{"access_token":"","refresh_token":"secret-refresh-token"}"#,
        ] {
            let error = SpotifyClient::parse_access_token_response(StatusCode::OK, body)
                .unwrap_err()
                .to_string();
            assert!(error.contains("missing a non-empty access_token"));
            assert!(!error.contains("secret-refresh-token"));
        }
    }

    #[test]
    fn does_not_accept_token_in_failed_http_response() {
        let error = SpotifyClient::parse_access_token_response(
            StatusCode::BAD_REQUEST,
            r#"{"error":"invalid_request","access_token":"secret-access-token","error_description":"private details"}"#,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("invalid_request"));
        assert!(!error.contains("secret-access-token"));
        assert!(!error.contains("private details"));
    }
}
