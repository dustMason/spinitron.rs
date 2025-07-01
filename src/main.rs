use anyhow::Result;
use chrono::{Local, NaiveDate};
use clap::Parser;
use std::path::PathBuf;

mod config;
mod models;
mod scraper;
mod spotify;

use config::AppConfig;
use models::{ShowEpisode, ShowGroup};
use spotify::SpotifyClient;
use std::collections::HashMap;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to config file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Date to scrape (YYYY-MM-DD format). Defaults to yesterday
    #[arg(short, long)]
    date: Option<String>,

    /// Create Spotify playlists from scraped data
    #[arg(short = 's', long)]
    spotify: bool,

    /// Output markdown list of all cached playlists
    #[arg(long)]
    list_playlists: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Handle list playlists command first
    if args.list_playlists {
        let mut spotify_client = SpotifyClient::new().await?;
        output_playlist_jsonl(&mut spotify_client).await?;
        return Ok(());
    }

    let config = AppConfig::load(&args.config)?;

    // Determine the end date (default to yesterday)
    let end_date = if let Some(date_str) = args.date {
        NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?
    } else {
        let yesterday = Local::now().naive_local().date() - chrono::Duration::days(1);
        yesterday
    };

    // Calculate start date (7 days before end date)
    let start_date = end_date - chrono::Duration::days(6);

    println!(
        "Scraping playlists from {} to {} (7 days)",
        start_date, end_date
    );

    let mut spotify_client = if args.spotify {
        println!("Initializing Spotify client...");
        let client = SpotifyClient::new().await?;
        Some(client)
    } else {
        None
    };

    // Collect all episodes across the 7-day period
    let mut all_episodes: HashMap<String, Vec<ShowEpisode>> = HashMap::new();

    // Process each station
    for (station_name, station_config) in &config.stations {
        println!("\n=== Processing station: {} ===", station_name);

        // Collect shows for each day in the 7-day period
        let mut current_date = start_date;
        while current_date <= end_date {
            println!("  Fetching shows for {}", current_date);

            match scraper::fetch_shows_for_date(station_name, current_date).await {
                Ok(shows) => {
                    // Filter shows if specific shows are configured
                    let shows_to_process = if station_config.shows.is_empty() {
                        shows
                    } else {
                        shows
                            .into_iter()
                            .filter(|show| station_config.shows.contains(&show.title))
                            .collect()
                    };

                    // Process each show
                    for show in shows_to_process {
                        match scraper::fetch_playlist(&show.url).await {
                            Ok(tracks) => {
                                let episode = ShowEpisode {
                                    show: show.clone(),
                                    tracks,
                                };

                                // Group by station + show name
                                let group_key = format!("{}-{}", station_name, show.title);
                                all_episodes
                                    .entry(group_key)
                                    .or_insert_with(Vec::new)
                                    .push(episode);
                            }
                            Err(e) => {
                                eprintln!(
                                    "    ❌ Failed to fetch playlist for {}: {}",
                                    show.title, e
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("  ❌ Failed to fetch shows for {}: {}", current_date, e);
                }
            }

            current_date = current_date + chrono::Duration::days(1);
        }
    }

    // Create ShowGroups and process playlists
    println!("\n=== Creating Spotify playlists ===");
    for (group_key, episodes) in all_episodes {
        if let Some(_first_episode) = episodes.first() {
            // Extract station from the group key
            let parts: Vec<&str> = group_key.split('-').collect();
            let station = parts[0].to_string();
            let show_name = parts[1..].join("-"); // Rejoin in case show name has dashes

            let show_group = ShowGroup {
                station,
                show_name,
                episodes,
            };

            let all_tracks = show_group.all_tracks();
            println!(
                "\n📺 Show Group: {} ({} episodes, {} total tracks)",
                show_group.playlist_name(),
                show_group.episodes.len(),
                all_tracks.len()
            );

            // Create/update Spotify playlist if requested
            if let Some(ref mut spotify) = spotify_client {
                match spotify.create_or_update_show_playlist(&show_group).await {
                    Ok(Some(playlist)) => {
                        println!(
                            "✅ Successfully created/updated Spotify playlist: {}\n",
                            playlist.name
                        );
                        if let Some(url) = playlist.external_url {
                            println!("  🔗 Share: {}", url);
                        }
                    }
                    Ok(None) => {
                        println!("⚠️  Skipped playlist for '{}' - no tracks found", show_group.playlist_name());
                    }
                    Err(e) => {
                        eprintln!(
                            "❌ Failed to create/update Spotify playlist for '{}': {}",
                            show_group.playlist_name(),
                            e
                        );
                    }
                }
            }
        }
    }

    if let Some(ref mut spotify) = spotify_client {
        let (cache_hits, api_calls) = spotify.get_cache_stats();
        let total_requests = cache_hits + api_calls;
        if total_requests > 0 {
            let cache_hit_rate = (cache_hits as f64 / total_requests as f64) * 100.0;
            println!("\n📊 Cache Statistics:");
            println!("  Total track searches: {}", total_requests);
            println!("  Cache hits: {} ({:.1}%)", cache_hits, cache_hit_rate);
            println!("  API calls: {} ({:.1}%)", api_calls, 100.0 - cache_hit_rate);
        }
        
        spotify.purge_expired_cache_entries()?;
    }

    Ok(())
}

async fn output_playlist_jsonl(spotify_client: &mut SpotifyClient) -> Result<()> {
    spotify_client.refresh_playlist_cache().await?;
    let mut playlists: Vec<_> = spotify_client.get_cached_playlists().iter().collect();
    playlists.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    
    for (_id, playlist) in playlists {
        // Extract station from playlist name (format: "STATION - Show Name")
        let station = if let Some(dash_pos) = playlist.name.find(" - ") {
            &playlist.name[..dash_pos]
        } else {
            "Unknown"
        };
        
        let playlist_json = serde_json::json!({
            "station": station,
            "name": playlist.name,
            "url": playlist.external_url.as_deref().unwrap_or(""),
            "track_count": playlist.track_count
        });
        println!("{}", playlist_json);
    }
    Ok(())
}
