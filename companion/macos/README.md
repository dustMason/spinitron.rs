# Spinitron for macOS

A native menu bar browser for the published Spinitron catalog. Requires macOS
14 or later and the Spotify desktop app. No Spotify API credentials are needed.

- Launching or reopening the app shows its panel. Click the radio icon in the
  menu bar to browse broadcasts, newest first.
- Click a row to open that exact playlist in Spotify.
- Search show names, artists and the sampled songs. **⌘F** focuses search.
- Use the station picker to switch stations. **Settings → Default station**
  controls the initial selection (KALX by default).
- Settings can also include the older playlist collections.
- **Refresh / ⌘R** downloads the latest catalog. Launching with a saved catalog
  makes no network request. The first launch downloads it automatically.
- Failed downloads or malformed data leave the last good catalog intact.
- **Quit / ⌘Q** exits the menu bar app.

## Build and launch

With Xcode or its command-line tools installed:

```sh
cd companion/macos
./scripts/build-app.sh
open build/Spinitron.app
```

The script builds for the current Mac's architecture, assembles the `.app`
bundle and signs it locally. Copy `build/Spinitron.app` into `~/Applications`
for a permanent installation. Distribution to other Macs would need a Developer
ID signature and notarization, or a local rebuild on those Macs.

## Data and settings

The app reads only the public
[catalog export](https://fiftyfootfoghorn.com/spinitron.rs/playlists.jsonl).
It doesn't scrape stations or change Spotify playlists or library membership.
Dates are grouped in Pacific time. Selecting a row sends the validated
`spotify:playlist:<id>` URL directly to `com.spotify.client` through AppKit.

Cached data: `~/Library/Caches/net.fiftyfootfoghorn.spinitron/catalog.json`.
Preferences use the `net.fiftyfootfoghorn.spinitron` UserDefaults domain.

## Tests

```sh
./scripts/test.sh
```

Tests cover broadcast dates, station/search filtering, legacy records, valid
Spotify destinations, offline reload and preserving the cache on refresh
failure. To also validate a downloaded production catalog, set
`SPINITRON_CATALOG_FIXTURE=/absolute/path/to/playlists.jsonl` when running tests.

## CI

The app source stays in this repository under `companion/macos/`. App-only
changes skip the scraper's push and pull-request checks. Both the checks and
the scheduled playlist job exclude `companion/` from their working checkouts;
they do not build or test Swift. Run the app's build and test scripts locally.
GitHub Pages publishes only `docs/`, so the app is not part of the website.
