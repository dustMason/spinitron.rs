# Spinitron native apps

The iPhone app and Mac menu-bar app share one local Swift package. Both read the
published catalog, keep an offline copy, default to KALX, and open playlists in
Spotify. They do not scrape stations, authenticate to Spotify, or edit playlists.

```text
companion/
  shared/   SpinitronCore + SpinitronUI, shared tests and Silver Hi-Fi assets
  macos/    AppKit menu-bar shell, desktop layout, local app packaging
  ios/      iPhone SwiftUI shell, Xcode project, signing and simulator scripts
```

Shared code owns catalog decoding/validation, downloading, atomic caching,
refresh state, filtering, Pacific-date grouping, playlist URLs and artist
samples. Shared SwiftUI code owns the Silver Hi-Fi colors, texture, LCD,
dot-matrix glyphs, button treatment and playlist text. Platform shells own
navigation, settings presentation and launching Spotify.

- [iPhone build and installation](ios/README.md)
- [Mac build and installation](macos/README.md)

All code remains under `companion/`. Existing scraper workflows already exclude
this entire directory and do not run for companion-only changes. No additional
scheduled workflow or signing secret is required; native builds/tests run locally.
