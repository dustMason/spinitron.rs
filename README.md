# Spinitron Scraper

A Rust application that scrapes radio station playlists from Spinitron and creates Spotify playlists from them.

I made it because I love listening to KALX and wanted an easy way to pull music that i hear on the air into my Spotify library. I discovered that Spinitron powers their radio playlists feature, so I'm using that as the source of data to power this app.

Claude Code wrote nearly all of this!

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

The app uses two caches:
- **HTML Cache**: `cache/` - Stores scraped HTML to avoid re-downloading
- **Spotify Track Cache**: `spotify_cache/track_cache.json` - Caches track search results

## Automation

### GitHub Actions (Recommended)

The repository includes GitHub Actions workflows for automated daily playlist updates:

**Setup:**
1. Fork this repository to your GitHub account
2. Go to Settings → Secrets and variables → Actions
3. Add the following repository secrets:
   - `SPOTIFY_CLIENT_ID` - Your Spotify app client ID
   - `SPOTIFY_CLIENT_SECRET` - Your Spotify app client secret  
   - `SPOTIFY_REFRESH_TOKEN` - Your refresh token (from `get_spotify_token.py`)

**Available Workflows:**
- **Daily Update** (`.github/workflows/daily-playlist-update.yml`)
  - Runs once daily at 6 AM UTC
  - Updates playlists and commits markdown list

### Other Automation Options

**Cron Job (Linux/macOS):**
```bash
# Add to crontab (crontab -e)
0 6 * * * cd /path/to/kalx && ./target/release/spinitron-scraper --spotify
```

**Windows Task Scheduler:**
- Create a daily task to run the executable
- Set environment variables in the task properties

**Docker + Cron:**
- Build a Docker image with the app
- Run in a container with cron scheduler