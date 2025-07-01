# Spinitron Scraper

A Rust application that scrapes radio station playlists from Spinitron and creates Spotify playlists from them.

I made it because I love listening to KALX and wanted an easy way to pull music that i hear on the air into my Spotify library. I discovered that Spinitron powers their radio playlists feature, so I'm using that as the source of data to power this app.

When it runs, it first scrapes the list of all shows for the given station. Then it scrapes each show's playlist to get all spins over the past week, and updates a Spotify playlist to include all of those tracks (or at least the ones that it can find on Spotify). This repo has a github action that runs daily at 6 AM UTC to update the playlists for the stations configured in `config.toml`.

Playlists are created with the naming format: **"Station Name - Show Name"**, like "KALX - Another Flippin' Sunday".

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
```

### Listing Playlists

```bash
# List all playlists as JSONL (requires Spotify auth)
cargo run -- --list-playlists
```

### Testing

```bash
# Run tests
cargo test
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

## Caching

The app uses two caches:
- **HTML Cache**: `cache/` - Stores scraped HTML to avoid re-downloading
- **Spotify Track Cache**: `spotify_cache/track_cache.json` - Caches track search results with 14-day expiration

## Automation

### GitHub Actions 

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