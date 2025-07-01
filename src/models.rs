use serde::{Deserialize, Serialize};
use chrono;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Show {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub start_time: String,
    pub end_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub artist: String,
    pub song: String,
    pub album: String,
    pub label: Option<String>,
    pub time: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ShowGroup {
    pub station: String,
    pub show_name: String,
    pub episodes: Vec<ShowEpisode>,
}

#[derive(Debug, Clone)]
pub struct ShowEpisode {
    pub show: Show,
    pub tracks: Vec<Track>,
}

impl ShowGroup {
    pub fn playlist_name(&self) -> String {
        // Sanitize show name to remove problematic characters
        let sanitized_show_name = self.show_name
            .replace("(((∞)))", "Infinity")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .chars()
            .filter(|c| c.is_ascii() || c.is_whitespace())
            .collect::<String>()
            .trim()
            .to_string();
        
        format!("{} - {}", self.station, sanitized_show_name)
    }
    
    pub fn all_tracks(&self) -> Vec<Track> {
        use std::collections::HashSet;
        
        let mut all_tracks = Vec::new();
        let mut seen_tracks = HashSet::new();
        
        for episode in &self.episodes {
            for track in &episode.tracks {
                // Create a unique key for deduplication (artist + song)
                let track_key = format!("{} - {}", track.artist.trim().to_lowercase(), track.song.trim().to_lowercase());
                
                if !seen_tracks.contains(&track_key) {
                    seen_tracks.insert(track_key);
                    all_tracks.push(track.clone());
                }
            }
        }
        
        all_tracks
    }
    
    pub fn spinitron_ids(&self) -> Vec<u64> {
        self.episodes.iter().map(|ep| ep.show.id).collect()
    }
    
    pub fn description(&self) -> String {
        // Get the highest Spinitron ID (most recent episode)
        let mut ids = self.spinitron_ids();
        ids.sort();
        ids.dedup(); // Remove duplicates
        let latest_id = ids.last().unwrap_or(&0);
        
        let sanitized_show_name = self.show_name
            .replace("(((∞)))", "Infinity")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .chars()
            .filter(|c| c.is_ascii() || c.is_whitespace())
            .collect::<String>()
            .trim()
            .to_string();
        
        format!(
            "Generated from Spinitron playlists. Station: {} Show: {} Episodes: {} Latest ID: {} Last updated: {}",
            self.station,
            sanitized_show_name,
            self.episodes.len(),
            latest_id,
            chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")
        )
    }
    
    pub fn latest_spinitron_id(&self) -> u64 {
        let mut ids = self.spinitron_ids();
        ids.sort();
        ids.dedup();
        *ids.last().unwrap_or(&0)
    }
}