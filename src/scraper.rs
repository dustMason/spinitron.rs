use anyhow::Result;
use chrono::NaiveDate;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;
use std::fs;
use std::path::Path;

use crate::models::{Show, Track};

pub struct SpinitronClient {
    client: Client,
    cache_dir: String,
}

impl SpinitronClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            cache_dir: "cache".to_string(),
        }
    }

    async fn ensure_cache_dir(&self) -> Result<()> {
        if !Path::new(&self.cache_dir).exists() {
            fs::create_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }

    fn get_cache_path(&self, url: &str) -> String {
        let sanitized = url.replace(['/', ':', '?', '&', '='], "_");
        format!("{}/{}.html", self.cache_dir, sanitized)
    }

    async fn fetch_with_cache(&self, url: &str, name: &str) -> Result<String> {
        self.ensure_cache_dir().await?;

        let cache_path = self.get_cache_path(url);

        // Try to read from cache first
        if let Ok(cached_content) = fs::read_to_string(&cache_path) {
            println!("✅ Using cached content for: {}", name);
            return Ok(cached_content);
        }

        // Fetch from network
        println!("⬇️ Fetching from network: {} ({})", name, url);
        let response = self.client.get(url).send().await?;
        let content = response.text().await?;

        // Save to cache
        if let Err(e) = fs::write(&cache_path, &content) {
            eprintln!("Warning: Could not cache response: {}", e);
        }

        Ok(content)
    }
}

pub async fn fetch_shows_for_date(station: &str, date: NaiveDate) -> Result<Vec<Show>> {
    let client = SpinitronClient::new();

    // Format the date for the API call
    let start_date = format!("{}T00:00:00", date.format("%Y-%m-%d"));
    let end_date = format!("{}T23:59:59", date.format("%Y-%m-%d"));

    // Build the calendar feed URL
    let url = format!(
        "https://spinitron.com/{}/calendar-feed?timeslot=15&start={}&end={}&_={}",
        station,
        urlencoding::encode(&start_date),
        urlencoding::encode(&end_date),
        chrono::Utc::now().timestamp_millis()
    );

    let content = client
        .fetch_with_cache(&url, &format!("{} shows for {}", station, date))
        .await?;
    let json: Value = serde_json::from_str(&content)?;

    let mut shows = Vec::new();

    if let Some(array) = json.as_array() {
        for item in array {
            if let (Some(id), Some(title), Some(url_path), Some(start), Some(end)) = (
                item["id"].as_u64(),
                item["title"].as_str(),
                item["url"].as_str(),
                item["start"].as_str(),
                item["end"].as_str(),
            ) {
                // Build full URL for the playlist
                let full_url = format!("https://spinitron.com{}", url_path);

                shows.push(Show {
                    id,
                    title: title.to_string(),
                    url: full_url,
                    start_time: start.to_string(),
                    end_time: end.to_string(),
                });
            }
        }
    }

    Ok(shows)
}

pub async fn fetch_playlist(url: &str) -> Result<Vec<Track>> {
    let client = SpinitronClient::new();
    // Extract show name from URL for better logging
    let show_name = url.split('/').last().unwrap_or("playlist");
    let html_content = client
        .fetch_with_cache(url, &format!("playlist for {}", show_name))
        .await?;

    parse_playlist_html(&html_content)
}

pub fn parse_playlist_html(html: &str) -> Result<Vec<Track>> {
    let document = Html::parse_document(html);

    // Select all spin items (table rows with class spin-item)
    let spin_selector = Selector::parse("tr.spin-item").unwrap();
    let artist_selector = Selector::parse("span.artist").unwrap();
    let song_selector = Selector::parse("span.song").unwrap();
    let release_selector = Selector::parse("span.release").unwrap();
    let label_selector = Selector::parse("span.label").unwrap();
    let time_selector = Selector::parse("td.spin-time a").unwrap();

    let mut tracks = Vec::new();

    for spin_element in document.select(&spin_selector) {
        let artist = spin_element
            .select(&artist_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let song = spin_element
            .select(&song_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let album = spin_element
            .select(&release_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let label = spin_element
            .select(&label_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        let time = spin_element
            .select(&time_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        // Only add tracks that have at least artist and song
        if !artist.is_empty() && !song.is_empty() {
            tracks.push(Track {
                artist,
                song,
                album,
                label,
                time,
            });
        }
    }

    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_playlist_html_with_sample() {
        let html_content = std::fs::read_to_string("playlist-sample.html")
            .expect("Failed to read playlist-sample.html");

        let tracks = parse_playlist_html(&html_content).expect("Failed to parse HTML");

        // Verify we parsed some tracks
        assert!(!tracks.is_empty(), "Should have parsed some tracks");

        // Check the first few tracks match what we expect from the sample
        let first_track = &tracks[0];
        assert_eq!(first_track.artist, "Loscil");
        assert_eq!(first_track.song, "Bell Flame");
        assert_eq!(first_track.album, "Lake Fire");
        assert_eq!(first_track.label, Some("Kranky".to_string()));
        assert_eq!(first_track.time, Some("12:03 AM".to_string()));

        let second_track = &tracks[1];
        assert_eq!(second_track.artist, "Emeralds");
        assert_eq!(second_track.song, "Up in the Air");
        assert_eq!(second_track.album, "What Happened");
        assert_eq!(second_track.label, Some("No Fun".to_string()));

        // Verify we have a reasonable number of tracks (the sample has many tracks)
        assert!(tracks.len() > 10, "Should have parsed more than 10 tracks");

        println!(
            "Successfully parsed {} tracks from sample file",
            tracks.len()
        );

        // Print first few tracks for verification
        for (i, track) in tracks.iter().take(5).enumerate() {
            println!(
                "Track {}: {} - {} ({})",
                i + 1,
                track.artist,
                track.song,
                track.album
            );
        }
    }

    #[test]
    fn test_parse_empty_html() {
        let empty_html = "<html><body></body></html>";
        let tracks = parse_playlist_html(empty_html).expect("Failed to parse empty HTML");
        assert!(tracks.is_empty(), "Empty HTML should result in no tracks");
    }

    #[test]
    fn test_parse_malformed_html() {
        let malformed_html = "<html><body><div>not a playlist</div></body></html>";
        let tracks = parse_playlist_html(malformed_html).expect("Failed to parse malformed HTML");
        assert!(
            tracks.is_empty(),
            "Malformed HTML should result in no tracks"
        );
    }
}
