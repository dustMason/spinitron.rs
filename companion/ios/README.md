# Spinitron for iPhone

A native iOS 17+ companion with the same Silver Hi-Fi skin as the Mac app.
Choose a station, browse broadcasts grouped by Pacific date, search shows and
the catalog's sampled artists/songs, and tap a row to open its playlist in Spotify.
The first release targets iPhone, in portrait and landscape.

## Set up on another Mac

The repository contains the app, shared Swift package, assets, and Xcode project.
No files or signing credentials need to be copied from the original development
laptop.

Prerequisites:

- Full Xcode with iOS platform support, not only the Command Line Tools.
  This implementation was built and tested with Xcode 27.0; other Xcode versions
  have not been verified.
- Your personal Apple account added in Xcode's account settings for device signing.
- An iPhone running iOS 17 or later, with Spotify installed for direct app links.
- Internet access for the first catalog download. An iOS Simulator runtime is
  only required if you want to run the Simulator build or tests.

Clone `https://github.com/dustMason/spinitron.rs.git` and check out the revision
containing this app. Until the app is merged into `main`, use the published
`feat/ios-companion` branch. The branch must be pushed before another laptop can
fetch it.

Keep `companion/ios` and `companion/shared` together: the Xcode project references
the shared package at `../shared`. Rust, the scraper's dependencies, and Spotify
API credentials are not required to build or use the iPhone app.

## Install on your own iPhone from that Mac

1. Open `companion/ios/Spinitron.xcodeproj` in Xcode. The **Spinitron** scheme is
   shared; its Swift packages are local and require no external dependencies.
2. Select the **Spinitron** app target → **Signing & Capabilities**, enable
   automatic signing, and choose your **personal** Apple development team.
   If Xcode needs a unique bundle identifier for your team, change it there.
3. Connect your iPhone, select it as the run destination, and press **Run (⌘R)**.
   Complete any device-trust or Developer Mode steps Xcode requests on the phone.

Alternatively, create the ignored `Signing.local.xcconfig` beside
`Signing.xcconfig` with your `DEVELOPMENT_TEAM = ...`. Personal signing settings
and Xcode user state are not committed. No Spotify client secret, login cookie,
refresh token, or GitHub credential belongs in the app.

The app does not need TestFlight. Apple signing/provisioning is a local Xcode
installation step, separate from building and testing the source.

After installation, select a station and tap a broadcast to confirm that it opens
the expected playlist in Spotify. Physical-device installation and this handoff
are checks to perform on the installing Mac and phone; they were not verified by
the Simulator tests.

## Use

- The blue LCD opens the station chooser. **Settings → Default station** controls
  the station selected when the app starts; it defaults to KALX.
- Tap any broadcast row to open that exact playlist. This navigates to Spotify;
  it does not start playback automatically. If Spotify cannot open, an alert
  offers the same playlist in the browser or Cancel.
- Search covers the selected station's entire downloaded catalog, not just the
  visible date. Song search is limited to the preview tracks exported by the site.
- **Refresh** or pull down on the list for the latest catalog. A cached launch
  does not download again. Refresh failures retain the last good catalog.
- Settings can include the older playlist collections and show cache freshness.
- The cache works offline; fetching new data and Spotify playback still need
  their respective network connections. Dates use Pacific time on both platforms.

The silver/blue appearance intentionally remains light in Dark Mode, matching
the chosen Mac skin. Reading text follows Dynamic Type; native sheets, list
scrolling and touch targets replace desktop-only interactions.

## Build and test locally

With Xcode and an iOS simulator runtime installed:

```sh
./companion/ios/scripts/build.sh
./companion/ios/scripts/test.sh
```

`test.sh` chooses an available iPhone simulator, preferring a booted one. Set
`IOS_SIMULATOR_ID` to an explicit simulator UDID when needed. The Xcode test
target compiles and runs the same catalog tests used by the shared Swift package.
Three native UI tests also cover browsing/search/stations, offline refresh,
settings, the missing-Spotify alert, landscape, and large accessibility text.
The scripts build without a signing identity for Simulator. Device signing is
only needed for installation on a physical iPhone.

For the shared package alone:

```sh
./companion/macos/scripts/test.sh
```

Build output is ignored under `build/` and `build-device/`. `scripts/make-app-icon.swift`
reproduces the native dot-matrix app icon from the shared skin's palette/glyphs.

## CI boundary

The daily importer, Rust/Python/JavaScript checks and GitHub Pages do not build
the native apps. Their path filters and sparse checkout already exclude
`companion/`; no CI or website deployment changes are required for this app.
