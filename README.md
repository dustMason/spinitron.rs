# Spinitron Scraper

A Rust application that scrapes radio station playlists from Spinitron and creates Spotify playlists from them.

I made it because I love listening to KALX and wanted an easy way to pull music that i hear on the air into my Spotify library. I discovered that Spinitron powers their radio playlists feature, so I'm using that as the source of data to power this app.

The daily job checks the past seven days for completed broadcasts. Each broadcast gets its own Spotify playlist, preserving the original order and repeated tracks that Spotify can match. Once published, that playlist is never rewritten by the archive job. Broadcasts are identified by station and Spinitron episode ID, so title changes do not create replacements.

New playlists are named **"Station - YYYY-MM-DD HH:MM - Broadcast title"**. The date and time come from the station's broadcast timestamp. The job waits until an episode has finished, plus a one-hour buffer, and fetches fresh track data before archiving it.

The full catalog lives in `data/catalog.json` and on the website. New playlists are removed from the owner's Spotify library once, after the catalog has been committed. They remain accessible by their Spotify links. Save any playlist you want to keep in your library; subsequent archive runs leave it alone. Existing playlists are imported as legacy catalog entries and are not automatically removed or rewritten.

Claude Code wrote nearly all of this!

## Browsing the catalog

The website uses compact playlist rows with three artist names and a direct
Spotify link. Expand a row to see the twelve-song sample with album art. The recent view
has one section for each of the last seven calendar days, including days with no
imports. Each day initially shows ten rows; expand it to see the rest.

The full archive includes every catalog record, including empty playlists and
records without a known date, across static pages of 25 playlists. Search show
names, stations, artists and sampled songs, combine a search with a station
filter, and bookmark or share the resulting URL. Search uses the cached song
samples; it does not search every track on Spotify. Pagination and previews also
work without JavaScript; filtering requires it.

New broadcasts record their first successful import time as `imported_at`.
Import days use Pacific time (`America/Los_Angeles`, including daylight saving).
Legacy records retain their last recorded update date; the site labels them
"Updated" rather than assigning a new import date during regeneration.

Generate and preview the complete site without Spotify credentials:

```bash
cargo build --release --locked
python3 scripts/update_website.py
python3 -m http.server 8765 --bind 127.0.0.1 --directory docs
```

Open `http://127.0.0.1:8765/`. For a separate build target, pass
`--binary /path/to/spinitron-scraper` to `update_website.py`. The generator writes
`docs/index.html`, `docs/archive/*.html`, shared CSS/JavaScript and the search
data under `docs/assets/`, plus the original `docs/playlists.jsonl` export.

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
python scripts/get_spotify_token.py
```

**Step 2: Use the App**
```bash
# Set all credentials (including the refresh token from step 1)
export SPOTIFY_CLIENT_ID="your_client_id"
export SPOTIFY_CLIENT_SECRET="your_client_secret"
export SPOTIFY_REFRESH_TOKEN="your_refresh_token"

# Archive individual completed broadcasts
cargo run -- --archive --date 2026-09-07

# Verify Spotify credentials without scraping or changing playlists
cargo run -- --check-spotify-auth
```

### Listing Playlists

```bash
# List the complete catalog as JSONL (no Spotify auth or API calls)
cargo run -- --list-catalog

# Save JSONL output to a file
cargo run -- --list-catalog > playlists.jsonl

# Example JSONL output:
# {"station":"KALX","name":"KALX - Show Name","url":"https://open.spotify.com/playlist/abc123","track_count":25}
# {"station":"KPOO","name":"KPOO - Another Show","url":"https://open.spotify.com/playlist/def456","track_count":42}
```

### Testing

```bash
# Run tests
cargo test
```

## Configuration

Edit `config.toml` to specify which stations to scrape and which shows to ignore:

```toml
[stations.KALX]
# Example: ignore FREEFORM shows and test shows
# ignores = ["FREEFORM", "Test Show \\d+"]

[stations.KPOO]
# Ignore those generic KPOO San Francisco shows
ignores = ["KPOO San Francisco .*"]

[stations.KPFA]
# No ignores - scrape all shows
```

The `ignores` field accepts regex patterns to filter out unwanted shows. Shows matching any ignore pattern will be skipped during scraping.

## Spotify Setup

1. **Create a Spotify App:**
   - Go to https://developer.spotify.com/dashboard
   - Click "Create App"
   - Fill in app name and description
   - Set the redirect URI to: `http://127.0.0.1:8888/callback`
   - Save and note your **Client ID** and **Client Secret**

2. **Get a Refresh Token:**
   - Set your client credentials as environment variables
   - Run the included Python script: `python scripts/get_spotify_token.py`
   - The script will open your browser and guide you through authorization
   - Copy the refresh token it provides

3. **Set Environment Variables:**
   ```bash
   export SPOTIFY_CLIENT_ID="your_client_id_here"
   export SPOTIFY_CLIENT_SECRET="your_client_secret_here"
   export SPOTIFY_REFRESH_TOKEN="your_refresh_token_here"
   ```

### Renewing an expired refresh token

Spotify refresh tokens expire **6 months after authorization**. This applies to
existing apps starting July 20, 2026. Daily access-token refreshes do not extend
that lifetime. See [Spotify's announcement](https://developer.spotify.com/blog/2026-06-18-refresh-token-expiration).

If the daily run reports `invalid_grant`, the refresh token is expired, revoked,
or otherwise invalid. To reconnect:

1. Set `SPOTIFY_CLIENT_ID` and `SPOTIFY_CLIENT_SECRET` for the existing Spotify app.
2. Verify that its registered redirect URI is `http://127.0.0.1:8888/callback`.
3. Run `python3 scripts/get_spotify_token.py` and authorize the app again.
4. Replace `SPOTIFY_REFRESH_TOKEN` under the repository's **Settings → Secrets
   and variables → Actions**, and in any local environment that uses the app.
5. Manually run **Daily Playlist Update** from the **Actions** tab with
   **auth_check_only** enabled to verify the credentials without updating playlists.
   Once that succeeds, run it again with the option disabled to update playlists
   and the website.

Re-running the workflow with the old token cannot renew it. Reauthorization is
required again when the new token reaches its six-month lifetime.

### Retrying individual playlists

To retry a failed update locally, pass each existing Spotify playlist ID explicitly:

```bash
cargo run -- --spotify --date 2026-09-07 --playlist-id YOUR_PLAYLIST_ID
```

Repeat `--playlist-id` for multiple playlists. The selected IDs must exist in the
account's Spinitron playlists, with only one selected ID per show name. This mode
syncs only those IDs and exits with an error if any selected show is missing,
empty, or fails to sync. The legacy `--spotify` mode scrapes fresh track lists
and compares the matched Spotify track IDs with the complete existing playlist,
in order. Identical contents leave the tracks, description, and “Last updated”
date untouched; the log reports `Unchanged`. Added, removed, or reordered tracks
trigger an update. Track searches and Spotify playlist reads finish before any
writes; a failed Spotify lookup/read or zero matches preserves the existing playlist.

The daily `--archive` mode continues to preserve each completed broadcast and
its original import date permanently.

## Spotify Playlist Organization

Spotify's Web API cannot manage playlist folders. This project separates the full
public archive from the playlists you choose to save in your personal library.
See [Spotify's library-removal semantics](https://developer.spotify.com/documentation/web-api/concepts/playlists#following-and-unfollowing-a-playlist).

### Verify library behavior before rollout

```bash
python3 scripts/verify_archive.py --reauthorize
```

This command reuses `SPOTIFY_CLIENT_ID` and `SPOTIFY_CLIENT_SECRET` from your
environment (prompting only for missing values), then prints a Spotify sign-in
link. Open that link to authorize the app. The loopback callback captures the new
refresh token automatically, with no token copying. If you already have all three
credentials to paste, use `--prompt` instead.

The command builds the app and creates one temporary test playlist. It verifies that removing the playlist
from the library preserves its contents and ownership, and that it can be saved
again. The test playlist is then emptied, removed from the library, and removed
from the public profile. No existing playlist is modified. Credentials stay in
process memory; the verification receipt contains no secrets.

The same command writes a **read-only cleanup proposal** containing existing
legacy catalog playlists that are still owned and saved by the authenticated
account. Review which to keep before any existing-library cleanup. The proposal
does not itself remove anything.

### Remove the old scraped playlists from Your Library

After deploying the archive workflow and website, prepare a fresh plan:

```bash
python3 scripts/cleanup_spinitron_library.py --prompt
```

This one command prompts for the three Spotify values with hidden input. Use
`--reauthorize` instead to reuse the app credentials and sign in through Spotify.
The default mode only reads Spotify. It writes `plan.json` and a complete
`catalog-backup.json` into a new dated folder under `verification/`. The plan
includes all owned, saved **legacy catalog** playlists, including empty ones.
Uncataloged generated playlists are listed separately for review; draft and new
broadcast playlists are excluded. Remove entries from `plan.json` if you want to
keep some in Your Library, before starting the cleanup.

Apply the reviewed plan explicitly, using the folder printed by the first command:

```bash
python3 scripts/cleanup_spinitron_library.py --apply verification/library-cleanup-TIMESTAMP/plan.json --prompt
```

For a large sweep, add `--batch-size 40` to group library-removal requests.
Metadata is still checked separately for every playlist, with at most four
concurrent reads. Writes run one batch at a time. A failed or uncertain batch is
recorded for every affected ID and stops the run without retrying the removal.

The script requires the personal Spotify account `dustmason` and verifies GitHub
as `dustMason`, without changing the default GitHub CLI account. Before any
removal it checks that the archive workflow is on `main`, no daily run is active,
and every selected link exists in a successfully deployed Pages catalog. The old
workflow discovers playlists through Your Library and could recreate removed ones.

Cleanup uses Spotify's [Remove Items from Library](https://developer.spotify.com/documentation/web-api/reference/remove-library-items)
endpoint for playlist URIs only. It never edits tracks, names, visibility, or the
catalog. Each removal is checked afterward: the playlist must still be readable,
have the same owner, track count and snapshot, and no longer be saved. This matches
Spotify's [unfollowing semantics](https://developer.spotify.com/documentation/web-api/concepts/playlists#following-and-unfollowing-a-playlist).

Keep `receipt.json` beside the plan. Each attempt is recorded before sending the
request. Rerunning the **same plan** skips completed and uncertain attempts, so
playlists you saved again survive. An uncertain result stops that run for review;
it is never automatically retried. Changes since planning also stop cleanup.
The process holds a local lock to prevent overlapping cleanup commands. No
credentials are written to the plan, backup, or receipt.

### Catalog persistence and one-time library removal

The catalog is durable state, **not a disposable cache**. Keep it in version
control and restore it if a checkout loses it. Missing or malformed catalogs
cause the archive to fail rather than silently creating duplicate playlists.

The workflow performs these phases in order:

1. Archive new broadcasts and verify the exact track sequence. Failed drafts stay
   in the library and can be resumed; completed playlists are never repopulated.
2. Generate the full website from the catalog, including playlists outside the library.
3. Prepare a removal plan for newly completed broadcasts and mark those entries
   as attempted. Commit and push the catalog **before** applying that plan.
4. Consume the plan, remove those new playlists from the library, persist results,
   and publish the website. The new links are published after the initial removal.

For manual operation:

```bash
cargo run -- --archive
cargo run -- --prepare-library-release release-plan.json
# Commit and push data/catalog.json before the next command.
cargo run -- --apply-library-release release-plan.json
# Commit and push data/catalog.json again to record results.
```

Use a fresh plan filename for another manual run. A consumed or uncertain removal
is never retried automatically, because you might have saved the playlist in the
meantime. An interrupted run may therefore leave a playlist in the library for
manual review. Recovery artifacts preserve the catalog and attempted plan if a
workflow cannot push its state. Do not replay a consumed plan or rebuild the
catalog from the library: playlists outside the library would be lost.

An uncertain playlist-creation request is recovered only when exactly one owned
playlist has the broadcast's precise archive marker. If none or multiple are
found, reconcile the ID before continuing; the app does not repeat the creation
blindly. Run only one local archive process at a time; Actions runs are serialized.

Explicit creation rejections (such as HTTP 400) leave the entry `prepared` so a
later run can retry after the cause is fixed. Timeouts and server errors remain
`creating` and require reconciliation. Spotify's structured error message is
included in failures. Archive descriptions use one line because Spotify rejects
line breaks; recovery recognizes both the one-line and older multiline markers.
Each run resumes saved drafts before scraping new broadcasts, including drafts
older than the seven-day scraping window. A draft is attempted only once per run.

The older `--spotify` and `--playlist-id` modes remain available for explicit
repairs of legacy rolling playlists. The daily workflow uses `--archive`.

## Caching

The app uses two caches:
- **HTML Cache**: `cache/` - Stores scraped HTML to avoid re-downloading
- **Spotify Track Cache**: `spotify_cache/track_cache.json` - Caches track search results with 14-day expiration

## Automation

### GitHub Actions 

The repository includes a GitHub Actions workflow for automated daily playlist updates:

**Setup:**
1. Fork this repository to your GitHub account
2. Go to Settings → Secrets and variables → Actions
3. Add the following repository secrets:
   - `SPOTIFY_CLIENT_ID` - Your Spotify app client ID
   - `SPOTIFY_CLIENT_SECRET` - Your Spotify app client secret  
   - `SPOTIFY_REFRESH_TOKEN` - Your refresh token (from `scripts/get_spotify_token.py`)

**Workflow:**
The **Daily Playlist Update** workflow (`.github/workflows/daily-playlist-update.yml`)
runs daily at 6 AM UTC, commits the broadcast catalog, removes newly archived
playlists from the library once, and publishes the website. Partial failures are reported as
failures after recovery state has been persisted. Pull requests run offline Rust
and website tests without Spotify credentials.
