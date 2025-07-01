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
        if args.spotify {
            let mut spotify_client = SpotifyClient::new().await?;
            output_playlist_markdown(&mut spotify_client).await?;
        } else {
            output_cached_playlist_markdown()?;
        }
        return Ok(());
    }

    // Load configuration
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

    // Initialize Spotify client if needed
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
                        println!("    Processing: {}", show.title);

                        // Fetch and parse playlist
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

            // Output sample tracks to stdout
            println!("Sample tracks:");
            for track in all_tracks.iter().take(5) {
                println!("  {} - {} ({})", track.artist, track.song, track.album);
            }
            if all_tracks.len() > 5 {
                println!("  ... and {} more tracks", all_tracks.len() - 5);
            }

            // Create/update Spotify playlist if requested
            if let Some(ref mut spotify) = spotify_client {
                match spotify.create_or_update_show_playlist(&show_group).await {
                    Ok(playlist) => {
                        println!(
                            "✅ Successfully created/updated Spotify playlist: {}",
                            playlist.name
                        );
                        if let Some(url) = playlist.external_url {
                            println!("  🔗 Share: {}", url);
                        }
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

    Ok(())
}

async fn output_playlist_markdown(spotify_client: &mut SpotifyClient) -> Result<()> {
    println!("# KALX Spotify Playlists\n");
    println!("Generated from Spinitron radio playlists - weekly aggregations of show episodes.\n");

    // Refresh cache to get latest playlists
    spotify_client.refresh_playlist_cache().await?;

    // Get all playlists and sort by name
    let mut playlists: Vec<_> = spotify_client.get_cached_playlists().iter().collect();
    playlists.sort_by(|a, b| a.1.name.cmp(&b.1.name));

    for (_id, playlist) in playlists {
        let url = playlist.external_url.as_deref().unwrap_or("No URL");
        println!("- [{}]({}) | {}", playlist.name, url, playlist.track_count);
    }

    println!("\n---");
    println!(
        "*Last updated: {}*",
        chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")
    );
    println!(
        "*Total playlists: {}*",
        spotify_client.get_cached_playlists().len()
    );

    Ok(())
}

fn output_cached_playlist_markdown() -> Result<()> {
    use serde_json::Value;
    use std::fs;

    let cache_path = "spotify_cache/playlist_cache.json";

    if !std::path::Path::new(cache_path).exists() {
        println!("No playlist cache found. Run with --spotify first to create playlists.");
        return Ok(());
    }

    let cache_content = fs::read_to_string(cache_path)?;
    let cache: Value = serde_json::from_str(&cache_content)?;

    println!("# KALX Spotify Playlists\n");
    println!("Generated from Spinitron radio playlists - weekly aggregations of show episodes.\n");

    if let Some(playlists) = cache["playlists"].as_object() {
        let mut playlist_list: Vec<_> = playlists.values().collect();
        playlist_list.sort_by(|a, b| {
            let name_a = a["name"].as_str().unwrap_or("");
            let name_b = b["name"].as_str().unwrap_or("");
            name_a.cmp(name_b)
        });

        let playlist_count = playlist_list.len();

        for playlist in &playlist_list {
            let name = playlist["name"].as_str().unwrap_or("Unknown");
            let url = playlist["external_url"].as_str().unwrap_or("No URL");
            println!("- [{}]({})", name, url);
        }

        println!("\n---");
        println!(
            "*Last updated: {}*",
            chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")
        );
        println!("*Total playlists: {}*", playlist_count);
    } else {
        println!("No playlists found in cache.");
    }

    Ok(())
}
