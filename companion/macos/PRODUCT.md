# Spinitron for macOS

## Product

A native macOS menu-bar browser for Jordan's published radio-broadcast catalog.
Choose a station, scan broadcasts grouped by Pacific date, search shows/artists/
songs, and click a broadcast to open its exact playlist in Spotify. KALX is the
default. The app caches the catalog and refreshes explicitly. It does not play
audio, scrape broadcasts, or modify Spotify playlists.

## Platform and constraints

SwiftUI inside an AppKit popover, macOS 14+, 520 × 640 points. Preserve the bundle
identity, cache, preferences, startup installation, keyboard shortcuts, and
Spotify URL validation. Keep companion changes isolated from scraper CI.

## Approved direction

Jordan chose **Silver Hi-Fi** on October 6, 2026 from three visual concepts:
brushed silver, blue LCD station display, tactile buttons, dark navy text,
compact rows with three artists, and distinct date dividers. The comparison
image uses illustrative catalog data; implementation must show real records.
