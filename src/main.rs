use anyhow::{bail, Result};
use chrono::{Local, NaiveDate};
use clap::Parser;
use std::path::PathBuf;

mod catalog;
mod config;
mod models;
mod scraper;
mod spotify;

use catalog::Catalog;
use config::AppConfig;
use models::{ShowEpisode, ShowGroup};
use spotify::{PlaylistUpdate, SpotifyClient};
use std::collections::HashMap;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(group(clap::ArgGroup::new("mode").args(["spotify", "list_playlists", "check_spotify_auth", "archive", "list_catalog", "sync_archive_names", "plan_archive_names"])))]
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

    /// Update only these existing Spotify playlist IDs (repeat for multiple IDs)
    #[arg(
        long = "playlist-id",
        requires = "spotify",
        conflicts_with = "list_playlists"
    )]
    playlist_ids: Vec<String>,

    /// Output markdown list of all cached playlists
    #[arg(long)]
    list_playlists: bool,

    /// Verify Spotify authentication without scraping or modifying playlists
    #[arg(long, conflicts_with_all = ["spotify", "list_playlists"])]
    check_spotify_auth: bool,

    /// Archive each completed broadcast once; preserve its tracks permanently
    #[arg(long)]
    archive: bool,

    /// Durable catalog, independent of the Spotify library
    #[arg(long, default_value = "data/catalog.json")]
    catalog: PathBuf,

    /// Export the complete catalog as JSONL without Spotify authentication
    #[arg(long)]
    list_catalog: bool,

    /// Rename broadcast archives whose catalog names differ; preserve tracks and membership
    #[arg(long)]
    sync_archive_names: bool,

    /// Show proposed broadcast name changes without authentication or writes
    #[arg(long)]
    plan_archive_names: bool,

    /// Maximum existing playlists to rename per pass (two seconds between requests)
    #[arg(long, default_value_t = 40, requires = "sync_archive_names")]
    name_limit: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.plan_archive_names {
        let catalog = Catalog::load(&args.catalog)?;
        for (key, name) in catalog.archive_names()? {
            let entry = &catalog.entries[&key];
            if entry.listing["name"].as_str() != Some(&name) {
                println!(
                    "{}",
                    serde_json::json!({"key":key,"playlist_id":entry.playlist_id,"before":entry.listing["name"],"after":name})
                );
            }
        }
        return Ok(());
    }
    if args.sync_archive_names {
        let mut catalog = Catalog::load(&args.catalog)?;
        let spotify = SpotifyClient::new().await?;
        let count = catalog
            .sync_archive_names(
                &args.catalog,
                &spotify,
                args.name_limit,
                std::time::Duration::from_secs(2),
            )
            .await?;
        eprintln!("Synchronized {count} archive names");
        return Ok(());
    }

    if args.list_catalog {
        let catalog = Catalog::load(&args.catalog)?;
        for listing in catalog.listings()? {
            println!("{}", serde_json::to_string(&listing)?);
        }
        return Ok(());
    }
    if args.check_spotify_auth {
        SpotifyClient::new().await?;
        println!("Spotify authentication succeeded.");
        return Ok(());
    }

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

    if args.archive {
        return archive_broadcasts(&config, &args.catalog, start_date, end_date).await;
    }

    println!(
        "Scraping playlists from {} to {} (7 days)",
        start_date, end_date
    );

    let mut spotify_client = if args.spotify {
        let client = SpotifyClient::new().await?;
        Some(client)
    } else {
        None
    };

    // Collect all episodes across the 7-day period
    let mut all_episodes: HashMap<String, Vec<ShowEpisode>> = HashMap::new();
    let spinitron = scraper::SpinitronClient::new();

    // Process each station
    for (station_name, station_config) in &config.stations {
        println!("\n=== Processing station: {} ===", station_name);

        // Collect shows for each day in the 7-day period
        let mut current_date = start_date;
        while current_date <= end_date {
            println!("  Fetching shows for {}", current_date);

            match scraper::fetch_shows_for_date(station_name, current_date).await {
                Ok(shows) => {
                    let shows_to_process = station_config.filter_shows(shows);

                    // Process each show
                    for show in shows_to_process {
                        let scraped = if args.spotify {
                            spinitron.fetch_playlist_fresh(&show.url).await
                        } else {
                            scraper::fetch_playlist(&show.url).await
                        };
                        match scraped {
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

    // Always refresh playlist cache from Spotify to avoid duplicates
    let mut pending_playlists = None;
    if let Some(ref mut spotify) = spotify_client {
        spotify.refresh_playlist_cache().await?;
        if !args.playlist_ids.is_empty() {
            pending_playlists = Some(spotify.restrict_to_playlists(&args.playlist_ids)?);
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

            let playlist_name = show_group.playlist_name();
            if pending_playlists
                .as_ref()
                .is_some_and(|names| !names.contains(&playlist_name))
            {
                continue;
            }

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
                    Ok(Some(result)) => {
                        let (status, playlist) = match result {
                            PlaylistUpdate::Created(playlist) => ("Created", playlist),
                            PlaylistUpdate::Updated(playlist) => ("Updated", playlist),
                            PlaylistUpdate::Unchanged(playlist) => ("Unchanged", playlist),
                        };
                        if let Some(names) = pending_playlists.as_mut() {
                            names.remove(&playlist_name);
                        }
                        println!("✅ {} Spotify playlist: {}\n", status, playlist.name);
                        if let Some(url) = playlist.external_url {
                            println!("  🔗 Share: {}", url);
                        }
                    }
                    Ok(None) => {
                        println!(
                            "⚠️  Skipped playlist for '{}' - no tracks found",
                            show_group.playlist_name()
                        );
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

    if let Some(names) = pending_playlists {
        if !names.is_empty() {
            let mut names: Vec<_> = names.into_iter().collect();
            names.sort();
            bail!(
                "Selected playlists did not sync successfully: {}",
                names.join(", ")
            );
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
            println!(
                "  API calls: {} ({:.1}%)",
                api_calls,
                100.0 - cache_hit_rate
            );
        }

        spotify.purge_expired_cache_entries()?;
    }

    Ok(())
}

async fn archive_broadcasts(
    config: &AppConfig,
    catalog_path: &std::path::Path,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<()> {
    let mut catalog = Catalog::load(catalog_path)?;
    let mut spotify = SpotifyClient::new().await?;
    let scraper = scraper::SpinitronClient::new();
    let mut failures = Vec::new();
    let mut created = 0;
    let mut attempted = std::collections::HashSet::new();
    for (key, result) in catalog.resume_pending(catalog_path, &mut spotify).await {
        attempted.insert(key.clone());
        match result {
            Ok(true) => {
                created += 1;
                eprintln!("Resumed archive {key}");
            }
            Ok(false) => (),
            Err(error) => failures.push(format!("{key}: {error}")),
        }
    }
    for (station, settings) in &config.stations {
        let mut date = start;
        while date <= end {
            match scraper::fetch_shows_for_date(station, date).await {
                Ok(shows) => {
                    for show in settings.filter_shows(shows) {
                        let key = Catalog::key(station, &show);
                        if catalog.finished(&key) || !attempted.insert(key) {
                            continue;
                        }
                        if catalog::broadcast_time(&show.end_time).is_ok_and(|time| {
                            time.with_timezone(&chrono::Utc)
                                > chrono::Utc::now() - chrono::Duration::hours(1)
                        }) {
                            eprintln!("Waiting for broadcast {} to finish", show.id);
                            continue;
                        }
                        let result = async {
                            let tracks = scraper.fetch_playlist_fresh(&show.url).await?;
                            if tracks.is_empty() {
                                eprintln!("No music listed for {} ({})", show.title, show.id);
                                return Ok(false);
                            }
                            catalog
                                .archive(catalog_path, &mut spotify, station, &show, &tracks)
                                .await
                        }
                        .await;
                        match result {
                            Ok(true) => {
                                created += 1;
                                eprintln!("Archived {station} - {} ({})", show.title, show.id);
                            }
                            Ok(false) => (),
                            Err(error) => failures.push(format!("{station}:{}: {error}", show.id)),
                        }
                    }
                }
                Err(error) => failures.push(format!("{station} {date}: {error}")),
            }
            date += chrono::Duration::days(1);
        }
    }
    // A newly discovered same-day broadcast can also change an older name.
    // The bounded migration resumes from the catalog on the next daily run.
    if let Err(error) = catalog
        .sync_archive_names(
            catalog_path,
            &spotify,
            40,
            std::time::Duration::from_secs(2),
        )
        .await
    {
        failures.push(format!("Archive names: {error}"));
    }
    eprintln!(
        "Archived {created} broadcasts; {} failures. Existing completed tracks were preserved.",
        failures.len()
    );
    if !failures.is_empty() {
        bail!("Broadcast archive incomplete:\n{}", failures.join("\n"));
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

        // Build a small preview of up to 12 tracks (artists, name, and full-size album image)
        let preview_items = spotify_client
            .get_playlist_preview(&playlist.id, 12)
            .await?;
        let preview = preview_items
            .into_iter()
            .map(|(name, artists, image_url)| {
                serde_json::json!({
                    "artists": artists,
                    "name": name,
                    "image_url": image_url,
                })
            })
            .collect::<Vec<_>>();

        // Extract a simple last-updated timestamp from the playlist description
        let last_updated = playlist
            .description
            .as_deref()
            .and_then(|d| d.split("Last updated: ").nth(1))
            .unwrap_or("");
        let playlist_json = serde_json::json!({
            "station": station,
            "name": playlist.name,
            "url": playlist.external_url.as_deref().unwrap_or(""),
            "track_count": playlist.track_count,
            "last_updated": last_updated,
            "preview": preview,
        });
        println!("{}", playlist_json);
    }
    Ok(())
}
