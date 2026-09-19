# Spinitron Scraper

A Rust application that scrapes radio station playlists from Spinitron and creates Spotify playlists from them.

I made it because I love listening to KALX and wanted an easy way to pull music that i hear on the air into my Spotify library. I discovered that Spinitron powers their radio playlists feature, so I'm using that as the source of data to power this app.

The daily job checks the past seven days for completed broadcasts. Each broadcast gets its own Spotify playlist, preserving the original order and repeated tracks that Spotify can match. Once published, its tracks are never rewritten by the archive job. Broadcasts are identified by station and Spinitron episode ID, so title changes do not create replacements.

New playlists are named **"Station - Broadcast title - YYYY-MM-DD"**. Duplicate show names on the same date add a time such as **5:00pm**. The date and time come from the station's broadcast timestamp. The job waits until an episode has finished, plus a one-hour buffer, and fetches fresh track data before archiving it.

Spotify track searches retry temporary server and connection errors up to three
attempts, with backoff and a 30-second timeout per attempt. Short `Retry-After`
delays are honored; longer cooldowns stop the lookup. Failed searches remain
errors and are never cached as missing songs. Playlist creation and other writes
are never retried automatically after an uncertain response.

The full catalog lives in `data/catalog.json` and on the website. New playlists stay saved in the owner's Spotify library. Archive runs preserve completed broadcasts and never automatically remove library membership. Existing playlists remain in the catalog even if they were removed from the library previously.

Started with Claude Code, then built out with Codex.

## Browsing the catalog

The website uses compact playlist rows with three artist names. **Open in Spotify**
uses a `spotify:playlist:…` link to open the installed app; the playlist title
opens the web player. These links work in recent imports, archive pages and
filtered search results. Expand a row to see the twelve-song sample with album art. The recent view
has one section for each of the last seven calendar days, including days with no
imports. Each day initially shows ten rows; expand it to see the rest.

Titles include the broadcast date, for example **FREEFORM - 2026-09-01**.
When a station has multiple entries with the same title on that date, a time
such as **5:00pm** distinguishes them; the UTC offset appears only if the clock
time repeats. Legacy collections use a clearly labeled
**updated** date because their original broadcast dates are unknown. Records
with identical titles and timestamps also show a stable identifier. These
catalog labels are calculated across the full archive, so filtering and paging
do not change them. Broadcast labels share the Spotify naming policy; legacy
labels use update dates only on the website.

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

## macOS menu bar app

The [native companion app](companion/macos/README.md) browses the published
catalog from the menu bar and opens playlists directly in Spotify. It caches
the list locally, includes an explicit Refresh command, and defaults to KALX.
Use Settings to change the default station or include older collections.

## Filing playlists in Spotify

The daily GitHub job can move saved Spinitron playlists into the existing **KALX**
folder immediately after importing. It runs on GitHub, so the laptop and Spotify
app can stay closed. Both KALX and KPOO broadcasts go into that folder.

This uses Spotify's undocumented web-player rootlist API. It selects only
completed catalog IDs that are saved at the library root and owned by `dustmason`.
Playlists inside any folder stay there. Older unsaved catalog records are never
restored. The script changes only folder membership; tracks, titles and library
membership stay unchanged. The personal account and existing folder ID are pinned
in `scripts/file_spotify_playlists.py`.

Configure the repository Actions secret **SPOTIFY_SP_DC** with the `sp_dc` cookie
from a signed-in personal Spotify web-player session (Chrome DevTools → Application
→ Cookies → `https://open.spotify.com`). Treat it as a login credential: never
commit it, put it in a command argument, or include it in logs. The script exchanges
it directly with Spotify for a fresh web token each run; it does not use the
scraper's public API client credentials. The cookie can expire or be revoked, and
Spotify can change this private protocol. A 401/403 requires checking the cookie
and the pinned public token-protocol parameters, not repeatedly retrying.

With `SPOTIFY_SP_DC` set securely in the environment:

```bash
# Preview without changing anything.
python3 scripts/file_spotify_playlists.py

# Move up to 50 verified playlists, one at a time, with readback after each move.
python3 scripts/file_spotify_playlists.py --apply
```

Each run writes a receipt under `verification/` with planned, completed and any
uncertain playlist IDs. The workflow preserves it as a recovery artifact. Writes
use the library revision to detect concurrent edits and are never automatically
retried. Rate limits stop the run and save a cooldown for the next run. A fresh
run reads actual folder membership, so an interrupted move is not blindly replayed.
Filing failures are reported separately and do not prevent the catalog or website
from being saved. Without the secret, the workflow reports that filing is not
configured and continues importing normally.

The local UI automation should be retired only after the deployed GitHub filing
step has succeeded with this personal session.

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

Spotify's Web API [does not expose playlist folders](https://developer.spotify.com/documentation/web-api/concepts/playlists#folders).
New broadcasts stay in Your Library. Use Spotify's desktop or web interface to
move generated playlists into a folder such as **KALX**; moving a playlist keeps
its ID, songs, and catalog link intact.

A separate local Codex automation can organize saved catalog playlists through
Spotify's interface after the daily import. It matches playlists against the
published catalog and skips ones already filed. Folder organization does not
run in GitHub Actions and needs the local Spotify session to be available.

Automatic library removal was retired after new archives became unavailable
following removal on September 17, 2026. The scraper no longer provides release
plans or an archive-removal verification command. The Python legacy-cleanup
scripts and old receipts are historical recovery tools, not part of daily runs.
Do not rerun an old cleanup plan to organize playlists.

### Catalog persistence

Broadcast playlists use `STATION - Show - YYYY-MM-DD`, using the broadcast's
local date. Repeated show names on the same station and date add a time, for
example `KALX - FREEFORM - 2026-09-09 5:00pm`. Times appear only when needed;
repeated daylight-saving hours also include the UTC offset. A broadcast ID is
the final fallback for otherwise identical names. Long show titles are shortened
before the date suffix, preserving the suffix and Unicode.

The website uses the same names. Each daily run also reconciles up to 40 existing
broadcast names, including older broadcasts when a later import introduces a
collision. It spaces rename reads and writes by two seconds and checkpoints
each verified name. Unchanged names make no requests. A lost rename response is
checked before another write, and unexpected manual name changes stop the pass.
Tracks, import dates, and saved status are preserved. Legacy rolling playlists
retain their names because their recorded update dates are not broadcast dates.

```bash
# Read-only name proposal; no Spotify authentication needed.
cargo run -- --plan-archive-names
# Apply one bounded pass using the configured Spotify account.
cargo run -- --sync-archive-names
```

Run a manual name pass separately from the archive workflow and library cleanup;
honor any active Spotify cooldown before starting it.

The catalog is durable state, **not a disposable cache**. Keep it in version
control and restore it if a checkout loses it. Missing or malformed catalogs
cause the archive to fail rather than silently creating duplicate playlists.

The workflow performs these phases in order:

1. Archive new broadcasts and verify the exact track sequence. Failed drafts stay
   in the library and can be resumed; completed playlists are never repopulated.
2. Generate the full website from the catalog, including older playlists outside
   the library.
3. Commit the catalog and generated website together, keeping new playlists saved.

For manual operation:

```bash
cargo run -- --archive
python3 scripts/update_website.py
# Commit and push data/catalog.json and the generated docs changes.
```

Recovery artifacts preserve the catalog if a workflow cannot push its state.
Historical `released` and `release_attempted` entries remain readable and are
never automatically re-created, saved, or removed. Do not rebuild the catalog
from Your Library: older playlists outside the library would be lost.

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
runs daily at 6 AM UTC, keeps new broadcasts saved in Spotify, commits the
broadcast catalog, and publishes the website. Partial failures are reported as
failures after recovery state has been persisted. Pull requests run offline Rust
and website tests without Spotify credentials.
