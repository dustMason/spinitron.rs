use anyhow::{anyhow, Result};
use base64::{engine::general_purpose, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::models::{ShowGroup, Track};

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
    access_token: String,
    user_id: String,
    track_cache: TrackSearchCache,
    playlist_cache: PlaylistCache,
    cache_dir: String,
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
        let cache_dir = "spotify_cache".to_string();

        // Ensure cache directory exists
        if !Path::new(&cache_dir).exists() {
            fs::create_dir_all(&cache_dir)?;
        }

        // Get access token
        let access_token =
            Self::get_access_token(&client, &client_id, &client_secret, &refresh_token).await?;

        // Get user ID and verify permissions
        let user_id = Self::get_user_id(&client, &access_token).await?;
        println!("✅ Spotify client initialized for user: {}", user_id);

        // Test if we can read user's playlists (to verify token permissions)
        let test_response = client
            .get("https://api.spotify.com/v1/me/playlists?limit=1")
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await?;

        if test_response.status().is_success() {
            println!("✅ Token has playlist permissions");
        } else {
            println!(
                "⚠️  Token may not have sufficient permissions: {}",
                test_response.status()
            );
        }

        // Load track cache only - we'll always refresh playlist cache from Spotify
        let track_cache = Self::load_track_cache(&cache_dir);
        let playlist_cache = PlaylistCache {
            playlists: std::collections::HashMap::new(),
        };

        Ok(Self {
            client,
            access_token,
            user_id,
            track_cache,
            playlist_cache,
            cache_dir,
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

        let json: Value = response.json().await?;

        json["access_token"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("Failed to get access token from Spotify"))
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
        let cache_path = format!("{}/track_cache.json", cache_dir);
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
        // Clean up expired entries before saving
        let current_time = Self::current_timestamp();
        self.track_cache.entries.retain(|_, entry| entry.expires_at > current_time);
        
        let cache_path = format!("{}/track_cache.json", self.cache_dir);
        let content = serde_json::to_string_pretty(&self.track_cache)?;
        fs::write(cache_path, content)?;
        Ok(())
    }

    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    pub async fn search_track(&mut self, track: &Track) -> Result<Option<SpotifyTrack>> {
        let search_key = format!("{} - {}", track.artist, track.song);

        // Check cache first
        if let Some(cached_entry) = self.track_cache.entries.get(&search_key) {
            return Ok(cached_entry.track.clone());
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

        let json: Value = response.json().await?;

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
        self.save_track_cache()?;

        Ok(spotify_track)
    }

    pub async fn create_or_update_show_playlist(
        &mut self,
        show_group: &ShowGroup,
    ) -> Result<Option<SpotifyPlaylist>> {
        let playlist_name = show_group.playlist_name();
        let description = show_group.description();
        let latest_id = show_group.latest_spinitron_id();
        let all_tracks = show_group.all_tracks();

        println!("Processing show playlist: '{}'", playlist_name);
        println!("  Episodes: {}", show_group.episodes.len());
        println!("  Total tracks: {}", all_tracks.len());
        println!("  Latest Spinitron ID: {}", latest_id);

        // Skip creating playlist if no tracks
        if all_tracks.is_empty() {
            println!("  ⚠️  Skipping playlist creation - no tracks found");
            return Ok(None);
        }

        // Always refresh playlist cache from Spotify to avoid duplicates
        println!("  Refreshing playlist cache from Spotify...");
        self.refresh_playlist_cache().await?;

        // Check if playlist already exists by name (more reliable than ID lookup)
        let existing_playlist = self
            .playlist_cache
            .playlists
            .values()
            .find(|p| p.name == playlist_name)
            .cloned();

        let playlist = if let Some(existing) = existing_playlist {
            println!("Found existing playlist: {}", existing.name);
            println!("  Updating playlist with latest 7-day collection");

            // Replace all tracks with the latest 7-day collection
            let new_tracks = show_group.all_tracks();
            println!(
                "  Replacing playlist with {} tracks from last 7 days",
                new_tracks.len()
            );

            // First, clear the existing playlist
            self.clear_playlist_tracks(&existing.id).await?;

            // Then add all the new tracks
            self.add_tracks_to_playlist(&existing.id, &new_tracks)
                .await?;

            // Update the playlist description with new latest ID
            let updated_description = show_group.description();
            self.update_playlist_description(&existing.id, &updated_description)
                .await?;

            let updated_existing = existing.clone();

            // Update in-memory cache
            self.playlist_cache
                .playlists
                .insert(latest_id.to_string(), updated_existing.clone());

            Some(updated_existing)
        } else {
            println!("Creating new playlist");

            // Debug: Check playlist name and description lengths and content
            println!(
                "  Playlist name: '{}' (length: {})",
                playlist_name,
                playlist_name.len()
            );
            println!("  Description length: {}", description.len());
            println!("  User ID: '{}'", self.user_id);

            // Validate playlist name (Spotify requirements)
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

            let url = format!(
                "https://api.spotify.com/v1/users/{}/playlists",
                self.user_id
            );
            println!("  Request URL: {}", url);

            // Create new playlist
            let playlist_data = serde_json::json!({
                "name": playlist_name,
                "description": description,
                "public": true
            });

            println!(
                "  Playlist data: {}",
                serde_json::to_string_pretty(&playlist_data)?
            );

            // Test token validity and permissions
            let test_response = self
                .client
                .get("https://api.spotify.com/v1/me")
                .header("Authorization", format!("Bearer {}", self.access_token))
                .send()
                .await?;

            if !test_response.status().is_success() {
                return Err(anyhow!(
                    "Token appears invalid. Status: {}",
                    test_response.status()
                ));
            }

            // Test playlist creation permissions by getting existing playlists
            let playlist_test_response = self
                .client
                .get("https://api.spotify.com/v1/me/playlists?limit=1")
                .header("Authorization", format!("Bearer {}", self.access_token))
                .send()
                .await?;

            if !playlist_test_response.status().is_success() {
                return Err(anyhow!(
                    "Token lacks playlist permissions. Status: {}",
                    playlist_test_response.status()
                ));
            }

            // Convert to JSON string manually to ensure proper encoding
            let json_payload = serde_json::to_string(&playlist_data)?;
            println!("  JSON payload: {}", json_payload);

            let response = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.access_token))
                .header("Content-Type", "application/json")
                .body(json_payload)
                .send()
                .await?;

            let status = response.status();
            let mut response_text = response.text().await?;

            println!("  Response status: {}", status);
            println!("  Response body: {}", response_text);

            if !status.is_success() {
                // If it's an auth error, try refreshing the token
                if status == 401 {
                    println!("  Token may have expired, attempting to refresh...");
                    self.access_token = Self::get_access_token(
                        &self.client,
                        &std::env::var("SPOTIFY_CLIENT_ID").unwrap_or_default(),
                        &std::env::var("SPOTIFY_CLIENT_SECRET").unwrap_or_default(),
                        &std::env::var("SPOTIFY_REFRESH_TOKEN").unwrap_or_default(),
                    )
                    .await?;

                    // Retry the request with new token
                    let json_payload = serde_json::to_string(&playlist_data)?;
                    let retry_response = self
                        .client
                        .post(&url)
                        .header("Authorization", format!("Bearer {}", self.access_token))
                        .header("Content-Type", "application/json")
                        .body(json_payload)
                        .send()
                        .await?;

                    let retry_status = retry_response.status();
                    let retry_response_text = retry_response.text().await?;

                    if !retry_status.is_success() {
                        return Err(anyhow!("Failed to create playlist after token refresh. Status: {}, Response: {}", retry_status, retry_response_text));
                    }

                    response_text = retry_response_text;
                } else {
                    return Err(anyhow!(
                        "Failed to create playlist. Status: {}, Response: {}",
                        status,
                        response_text
                    ));
                }
            }

            let playlist_json: Value = serde_json::from_str(&response_text)?;

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

            // Add tracks to the new playlist
            let all_tracks = show_group.all_tracks();
            self.add_tracks_to_playlist(&playlist.id, &all_tracks)
                .await?;

            let mut updated_playlist = playlist.clone();
            updated_playlist.track_count = all_tracks.len() as u32;

            // Cache the playlist in memory
            self.playlist_cache
                .playlists
                .insert(latest_id.to_string(), updated_playlist.clone());

            Some(updated_playlist)
        };

        match &playlist {
            Some(p) => {
                println!(
                    "✅ Processed playlist: {} ({} total tracks)",
                    p.name,
                    all_tracks.len()
                );
            }
            None => {
                println!("⚠️  No playlist created - no tracks available");
            }
        }
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
            .put(&format!(
                "https://api.spotify.com/v1/playlists/{}",
                playlist_id
            ))
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

    async fn add_tracks_to_playlist(&mut self, playlist_id: &str, tracks: &[Track]) -> Result<()> {
        let mut track_uris = Vec::new();
        let mut found_tracks = 0;
        let mut not_found_tracks = 0;

        println!("Searching for {} tracks on Spotify...", tracks.len());

        // For very large playlists, limit to first 5000 tracks to avoid timeouts
        let tracks_to_process = if tracks.len() > 5000 {
            println!("  Large playlist detected, limiting to first 5000 tracks");
            &tracks[..5000]
        } else {
            tracks
        };

        for (i, track) in tracks_to_process.iter().enumerate() {
            if i % 50 == 0 {
                println!(
                    "  Progress: {}/{} tracks processed",
                    i,
                    tracks_to_process.len()
                );
            }

            match self.search_track(track).await {
                Ok(Some(spotify_track)) => {
                    track_uris.push(spotify_track.uri);
                    found_tracks += 1;
                }
                Ok(None) => {
                    not_found_tracks += 1;
                }
                Err(_) => {
                    not_found_tracks += 1;
                }
            }

            // Add delay every 10 tracks to avoid rate limiting
            if i % 10 == 0 && i > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }

        println!(
            "Track search complete: {} found, {} not found",
            found_tracks, not_found_tracks
        );

        if !track_uris.is_empty() {
            println!("Adding {} tracks to playlist...", track_uris.len());

            for (i, chunk) in track_uris.chunks(100).enumerate() {
                let add_tracks_data = serde_json::json!({
                    "uris": chunk
                });

                let response = self
                    .client
                    .post(&format!(
                        "https://api.spotify.com/v1/playlists/{}/tracks",
                        playlist_id
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

                println!("  Added batch {} ({} tracks)", i + 1, chunk.len());
            }
        }

        Ok(())
    }

    async fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Vec<String>> {
        let mut all_track_uris = Vec::new();
        let mut url = Some(format!(
            "https://api.spotify.com/v1/playlists/{}/tracks?limit=100",
            playlist_id
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

            if let Some(items) = json["items"].as_array() {
                for item in items {
                    if let Some(track) = item["track"].as_object() {
                        if let Some(uri) = track["uri"].as_str() {
                            all_track_uris.push(uri.to_string());
                        }
                    }
                }
            }

            url = json["next"].as_str().map(|s| s.to_string());
        }

        Ok(all_track_uris)
    }

    async fn clear_playlist_tracks(&self, playlist_id: &str) -> Result<()> {
        println!("    Clearing existing tracks from playlist...");

        // Get all current track URIs
        let track_uris = self.get_playlist_tracks(playlist_id).await?;

        if track_uris.is_empty() {
            println!("    Playlist is already empty");
            return Ok(());
        }

        println!("    Removing {} existing tracks", track_uris.len());

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
                    "https://api.spotify.com/v1/playlists/{}/tracks",
                    playlist_id
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

            println!("    Removed batch {} ({} tracks)", i + 1, chunk.len());
        }

        Ok(())
    }

    pub async fn refresh_playlist_cache(&mut self) -> Result<()> {
        println!("Refreshing playlist cache from Spotify...");

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

            let json: Value = response.json().await?;

            if let Some(items) = json["items"].as_array() {
                for item in items {
                    let playlist_name = item["name"].as_str().unwrap_or("Unknown");
                    if let Some(description) = item["description"].as_str() {
                        // Look for either old format "Spinítron ID:" or new format "Latest ID:"
                        let has_generated =
                            description.contains("Generated from Spinitron playlists");
                        let has_old_format = description.contains("Spinítron ID:");
                        let has_new_format = description.contains("Latest ID:");
                        let is_kalx = playlist_name.starts_with("KALX -");

                        // Be more flexible - match if it's a KALX playlist OR has our description
                        let is_spinitron_playlist =
                            has_generated || (is_kalx && (has_old_format || has_new_format));

                        if is_spinitron_playlist {
                            // Try to extract ID from either format, fallback to playlist name hash
                            let spinitron_id =
                                if let Some(id_str) = description.split("Latest ID: ").nth(1) {
                                    id_str.split_whitespace().next().unwrap_or("0").to_string()
                                } else if let Some(spinitron_line) = description
                                    .lines()
                                    .find(|line| line.contains("Spinítron ID:"))
                                {
                                    if let Some(id_str) = spinitron_line.split(':').nth(1) {
                                        let cleaned = id_str.trim().replace(['[', ']'], "");
                                        cleaned.split(',').next().unwrap_or("0").trim().to_string()
                                    } else {
                                        "0".to_string()
                                    }
                                } else {
                                    // Fallback: use a hash of the playlist name for unique identification
                                    use std::collections::hash_map::DefaultHasher;
                                    use std::hash::{Hash, Hasher};
                                    let mut hasher = DefaultHasher::new();
                                    playlist_name.hash(&mut hasher);
                                    hasher.finish().to_string()
                                };

                            // Get track count from the response (already included!)
                            let track_count = item["tracks"]["total"].as_u64().unwrap_or(0) as u32;

                            let playlist = SpotifyPlaylist {
                                id: item["id"].as_str().unwrap_or("").to_string(),
                                name: item["name"].as_str().unwrap_or("").to_string(),
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

        println!(
            "Refreshed cache with {} playlists",
            self.playlist_cache.playlists.len()
        );

        Ok(())
    }

    pub fn get_cached_playlists(&self) -> &std::collections::HashMap<String, SpotifyPlaylist> {
        &self.playlist_cache.playlists
    }

}
