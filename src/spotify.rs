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

        // Test if we can read user's playlists (to verify token permissions)
        let test_response = client
            .get("https://api.spotify.com/v1/me/playlists?limit=1")
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await?;

        if !test_response.status().is_success() {
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

    async fn search_track_with_cache_info(&mut self, track: &Track) -> Result<(Option<SpotifyTrack>, bool)> {
        let search_key = format!("{} - {}", track.artist, track.song);

        // Check cache first
        if let Some(cached_entry) = self.track_cache.entries.get(&search_key) {
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
        let json: Value = serde_json::from_str(&response_text)
            .map_err(|e| anyhow!("Failed to parse search JSON response: {}. Response body: {}", e, response_text))?;

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

        Ok((spotify_track, true)) // true = API call was made
    }


    pub async fn create_or_update_show_playlist(
        &mut self,
        show_group: &ShowGroup,
    ) -> Result<Option<SpotifyPlaylist>> {
        let playlist_name = show_group.playlist_name();
        let description = show_group.description();
        let latest_id = show_group.latest_spinitron_id();
        let all_tracks = show_group.all_tracks();


        // Skip creating playlist if no tracks
        if all_tracks.is_empty() {
            println!("  ⚠️  Skipping playlist creation - no tracks found");
            return Ok(None);
        }

        // Always refresh playlist cache from Spotify to avoid duplicates
        self.refresh_playlist_cache().await?;

        // Check if playlist already exists by name (more reliable than ID lookup)
        let existing_playlist = self
            .playlist_cache
            .playlists
            .values()
            .find(|p| p.name == playlist_name)
            .cloned();

        let playlist = if let Some(existing) = existing_playlist {

            // First, clear the existing playlist
            self.clear_playlist_tracks(&existing.id).await?;

            // Then add all the new tracks
            let new_tracks = show_group.all_tracks();
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

            // Create new playlist
            let playlist_data = serde_json::json!({
                "name": playlist_name,
                "description": description,
                "public": true
            });


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

            if !status.is_success() {
                // If it's an auth error, try refreshing the token
                if status == 401 {
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
        let mut api_calls_made = 0;
        let mut cache_hits = 0;

        println!("Searching for {} tracks on Spotify...", tracks.len());

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

        println!(
            "Track search complete: {} found, {} not found ({} cache hits, {} API calls)",
            found_tracks, not_found_tracks, cache_hits, api_calls_made
        );

        if !track_uris.is_empty() {

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
            }
            println!("Added {} tracks to playlist", track_uris.len());
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
        // Get all current track URIs
        let track_uris = self.get_playlist_tracks(playlist_id).await?;

        if track_uris.is_empty() {
            return Ok(());
        }

        println!("Removed {} existing tracks", track_uris.len());

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
                return Err(anyhow!(
                    "Spotify API error ({}): {}",
                    status,
                    error_text
                ));
            }

            let response_text = response.text().await?;
            let json: Value = serde_json::from_str(&response_text)
                .map_err(|e| anyhow!("Failed to parse JSON response: {}. Response body: {}", e, response_text))?;

            if let Some(items) = json["items"].as_array() {
                for item in items {
                    // Skip playlists without proper names to avoid hash collisions
                    let Some(playlist_name) = item["name"].as_str() else {
                        continue;
                    };
                    
                    if let Some(description) = item["description"].as_str() {
                        let has_generated = description.contains("Generated from Spinitron playlists");
                        let has_latest_id = description.contains("Latest ID:");

                        if has_generated || has_latest_id {
                            // Extract ID from "Latest ID: " format
                            let spinitron_id = if let Some(id_str) = description.split("Latest ID: ").nth(1) {
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

}
