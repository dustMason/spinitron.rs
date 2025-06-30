# Spinitron Scraper

A Rust application that scrapes radio station playlists from Spinitron and creates Spotify playlists from them.

## Features

- Scrapes show listings from Spinitron calendar API
- Parses playlist HTML to extract track information (artist, song, album, label)
- Configurable via TOML to specify stations and shows
- Caches scraped HTML content to avoid excessive requests
- Creates **public** Spotify playlists
- Outputs shareable URLs for each created playlist
- Caches Spotify track searches and playlist metadata for deduplication
- Includes Spinitron ID in playlist descriptions to prevent duplicates
- Outputs playlist data to stdout

## Usage

### Basic Usage

```bash
# Build the application
cargo build --release

# Run with default config (scrapes yesterday's playlists)
cargo run

# Specify a different date
cargo run -- --date 2025-06-29

# Use a different config file
cargo run -- --config my-config.toml
```

### Spotify Integration

**Step 1: Get a Refresh Token**
```bash
# First, set your Spotify app credentials
export SPOTIFY_CLIENT_ID="your_client_id"
export SPOTIFY_CLIENT_SECRET="your_client_secret"

# Run the Python script to get a refresh token
python3 get_spotify_token.py
```

**Step 2: Use the App**
```bash
# Set all credentials (including the refresh token from step 1)
export SPOTIFY_CLIENT_ID="your_client_id"
export SPOTIFY_CLIENT_SECRET="your_client_secret"
export SPOTIFY_REFRESH_TOKEN="your_refresh_token"

# Scrape and create Spotify playlists
cargo run -- --spotify --date 2025-06-29

# Refresh playlist cache before creating new playlists
cargo run -- --spotify --refresh-spotify-cache --date 2025-06-29
```

### Listing Playlists

```bash
# List all cached playlists as markdown (fast)
cargo run -- --list-playlists

# List playlists by refreshing from Spotify (slower, requires auth)
cargo run -- --list-playlists --spotify

# Save to a markdown file
cargo run -- --list-playlists > kalx-playlists.md
```

### Testing

```bash
# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture
```

## Configuration

Edit `config.toml` to specify which stations and shows to scrape:

```toml
[stations.KALX]
# Empty shows list means scrape all shows for the day
shows = []

[stations.KPFA]
# Or specify particular shows
shows = ["Morning Mix", "The Afternoon Show"]
```

## Output Format

The app outputs playlist data to stdout. When creating Spotify playlists, it also shows shareable URLs:

```
=== Processing station: KALX ===

--- Show: Round Midnight ---
Loscil - Bell Flame (Lake Fire)
Emeralds - Up in the Air (What Happened)
...
✓ Created Spotify playlist: Round Midnight - 2025-06-29
  🔗 Share: https://open.spotify.com/playlist/abc123xyz
```

## Spotify Setup

1. **Create a Spotify App:**
   - Go to https://developer.spotify.com/dashboard
   - Click "Create App"
   - Fill in app name and description
   - Set the redirect URI to: `http://localhost:8888/callback`
   - Save and note your **Client ID** and **Client Secret**

2. **Get a Refresh Token:**
   - Set your client credentials as environment variables
   - Run the included Python script: `python3 get_spotify_token.py`
   - The script will open your browser and guide you through authorization
   - Copy the refresh token it provides

3. **Set Environment Variables:**
   ```bash
   export SPOTIFY_CLIENT_ID="your_client_id_here"
   export SPOTIFY_CLIENT_SECRET="your_client_secret_here"
   export SPOTIFY_REFRESH_TOKEN="your_refresh_token_here"
   ```

## Spotify Playlist Organization

Playlists are created with the naming format: **"Station Name - Show Name"**

Each playlist includes the latest Spinitron ID for that show in its description to prevent duplicates.

## Caching

The app uses several caching mechanisms:
- **HTML Cache**: `cache/` - Stores scraped HTML to avoid re-downloading
- **Spotify Track Cache**: `spotify_cache/track_cache.json` - Caches track search results
- **Spotify Playlist Cache**: `spotify_cache/playlist_cache.json` - Caches existing playlists by Spinitron ID