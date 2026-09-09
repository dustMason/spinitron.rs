use anyhow::{bail, Result};
use chrono::{Local, NaiveDate};
use clap::Parser;
use std::path::PathBuf;

mod catalog;
mod config;
mod models;
mod scraper;
mod spotify;

use catalog::{ArchiveSpotify, Catalog};
use config::AppConfig;
use models::{ShowEpisode, ShowGroup};
use spotify::SpotifyClient;
use std::collections::HashMap;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(group(clap::ArgGroup::new("mode").args(["spotify", "list_playlists", "check_spotify_auth", "archive", "list_catalog", "prepare_library_release", "apply_library_release", "verify_archive_flow", "plan_library_cleanup"])))]
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

    /// Prepare one-time library removals for new broadcasts; commit the catalog before applying
    #[arg(long, value_name = "PLAN.json")]
    prepare_library_release: Option<PathBuf>,

    /// Apply and consume a prepared library-removal plan; never reuses a consumed plan
    #[arg(long, value_name = "PLAN.json")]
    apply_library_release: Option<PathBuf>,

    /// Verify the library/archive behavior on one temporary playlist and write a receipt
    #[arg(long, value_name = "RECEIPT.json")]
    verify_archive_flow: Option<PathBuf>,

    /// Write a read-only cleanup proposal for existing catalog playlists in your library
    #[arg(long, value_name = "PLAN.json")]
    plan_library_cleanup: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(plan_path) = &args.plan_library_cleanup {
        let catalog = Catalog::load(&args.catalog)?;
        let spotify = SpotifyClient::new().await?;
        let plan = spotify.legacy_library_cleanup_plan(&catalog).await?;
        catalog::atomic_json(plan_path, &plan)?;
        println!("Wrote a read-only proposal for {} existing playlists; no library membership was changed.", plan["count"]);
        return Ok(());
    }

    if let Some(receipt_path) = &args.verify_archive_flow {
        let catalog = Catalog::load(&args.catalog)?;
        let source = catalog
            .entries
            .values()
            .find(|e| e.listing["track_count"].as_u64().unwrap_or(0) > 0)
            .and_then(|e| e.playlist_id.as_deref())
            .ok_or_else(|| {
                anyhow::anyhow!("Catalog needs a nonempty source playlist for verification")
            })?;
        let mut spotify = SpotifyClient::new().await?;
        spotify.verify_archive_flow(source, receipt_path).await?;
        println!("Verified archive access after removal and saving it again. The test playlist was emptied, removed from the library, and removed from the public profile.");
        return Ok(());
    }

    if args.list_catalog {
        let catalog = Catalog::load(&args.catalog)?;
        for listing in catalog.listings() {
            println!("{}", serde_json::to_string(listing)?);
        }
        return Ok(());
    }
    if let Some(plan_path) = &args.prepare_library_release {
        if plan_path.exists() || plan_path.with_extension("consumed.json").exists() {
            bail!("Choose a new plan path; existing plans must not be overwritten");
        }
        let spotify = SpotifyClient::new().await?;
        let mut catalog = Catalog::load(&args.catalog)?;
        let plan = catalog.prepare_release(&args.catalog, spotify.owner_id())?;
        catalog::atomic_json(plan_path, &plan)?;
        eprintln!("Prepared {} new broadcasts. Commit and push the catalog before applying this one-use plan.", plan.playlists.len());
        return Ok(());
    }
    if let Some(plan_path) = &args.apply_library_release {
        let spotify = SpotifyClient::new().await?;
        let mut catalog = Catalog::load(&args.catalog)?;
        catalog
            .apply_release(&args.catalog, plan_path, &spotify)
            .await?;
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
                    Ok(Some(playlist)) => {
                        if let Some(names) = pending_playlists.as_mut() {
                            names.remove(&playlist_name);
                        }
                        println!(
                            "✅ Successfully created/updated Spotify playlist: {}\n",
                            playlist.name
                        );
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
            bail!("Selected playlists were not updated: {}", names.join(", "));
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
    for (station, settings) in &config.stations {
        let mut date = start;
        while date <= end {
            match scraper::fetch_shows_for_date(station, date).await {
                Ok(shows) => {
                    for show in settings.filter_shows(shows) {
                        if catalog.finished(&Catalog::key(station, &show)) {
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
    eprintln!(
        "Archived {created} broadcasts; {} failures. Existing completed playlists were preserved.",
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
