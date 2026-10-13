---
name: Spinitron for iPhone — Silver Hi-Fi
description: A native iPhone radio-catalog browser using the shared satin silver and blue LCD skin.
colors:
  ink: "rgb(8% 18% 29%)"
  secondary: "rgb(25% 33% 41%)"
  edge: "rgb(37% 42% 47%)"
  paper: "rgb(88% 90% 92%)"
  blue: "rgb(4% 34% 56%)"
  lcd-top: "rgb(53% 76% 94%)"
  lcd-bottom: "rgb(77% 89% 98%)"
  error-surface: "rgb(100% 94% 80%)"
typography:
  title: { fontFamily: "SF" }
  artist: { fontFamily: "SF" }
  metadata: { fontFamily: "SF" }
  button: { fontFamily: "SF Mono", fontWeight: 600 }
  date: { fontFamily: "SF", fontWeight: 600 }
rounded:
  control: "3pt"
  lcd: "4pt"
spacing:
  summary: "5pt"
  status: "6pt"
  actions: "8pt"
  row: "12pt"
  lcd-horizontal: "14pt"
components:
  button:
    textColor: "{colors.ink}"
    typography: "{typography.button}"
    rounded: "{rounded.control}"
    padding: "0pt 13pt"
  button-compact:
    textColor: "{colors.ink}"
    typography: "{typography.button}"
    rounded: "{rounded.control}"
    padding: "0pt 8pt"
  station-display:
    textColor: "{colors.ink}"
    rounded: "{rounded.lcd}"
    padding: "8pt 14pt"
    height: "64pt"
  station-display-compact:
    textColor: "{colors.ink}"
    rounded: "{rounded.lcd}"
    padding: "8pt 14pt"
    height: "48pt"
  playlist-row:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    typography: "{typography.title}"
  refresh-error:
    backgroundColor: "{colors.error-surface}"
    textColor: "{colors.ink}"
---

# Design System: Spinitron for iPhone — Silver Hi-Fi

## Overview

**Creative North Star: "Silver Hi-Fi"**

The iPhone companion carries forward Jordan's selected Mac skin: satin brushed silver, a blue dot-matrix LCD, tactile beveled keys, and navy catalog text. Native iPhone navigation, search, lists, sheets, and reading sizes supply the interaction model. The user pinned the light silver appearance, including when the operating system uses Dark Mode.

**Key Characteristics:**

- Shared silver material, blue LCD, and shallow beveled controls.
- Native Dynamic Type for reading text; vector dot-matrix branding and station names.
- Full-width playlist targets and native station, search, and settings controls.

Implementation authority: `../shared/Sources/SpinitronUI/HiFiSkin.swift`, `../shared/Sources/SpinitronUI/PlaylistSummary.swift`, and `Spinitron/{CatalogView,SettingsView,SpinitronApp}.swift`. Measurements are native points; RGB percentages preserve the SwiftUI sRGB channels. The sidecar records native behavior and material details, without HTML replicas. Verification evidence and remaining device limits are in `design/catalog.md`.

## Colors

### Primary

The `lcd-top` → `lcd-bottom` gradient lights the station display and pressed key faces. `blue` supplies the app tint and outward playlist arrows.

### Neutral

`ink` carries titles and control labels; `secondary` carries artist samples, metadata, and freshness. `paper` backs the catalog and navigation bar. `edge` defines the metal keys and LCD rim. Brushed texture appears on the chassis, around the station display, and behind the normal-size footer.

The warm `error-surface` identifies a failed refresh above the retained catalog. Native settings, alerts, search, and station choices keep their system treatments within the app's explicit light color scheme.

## Typography

Playlist titles use SwiftUI `.headline`, artist samples `.subheadline`, and metadata `.caption`. Titles and samples wrap without a line limit. Date labels use `.caption.weight(.semibold)`; the footer uses `.caption`. Touch keys use `.system(.body, design: .monospaced).weight(.semibold)`. These are semantic text styles, not fixed point sizes; the sidecar maps the frontmatter roles to their SwiftUI definitions.

The wordmark and station display use the shared 5 × 7 vector glyphs. The wordmark pitch is 2 pt. Station names are uppercase and fit the available display using `0.85 * min(height / 7, width / glyphColumns)`; glyphs advance six cells and unsupported characters become a question mark. The enclosing station button exposes “Choose station,” the current station value, and a browsing/search hint to accessibility.

## Layout

The app uses `NavigationStack`, an inline native navigation bar, an always-visible search drawer, and a plain `List` grouped by Pacific broadcast date. Refresh and Settings sit together in the trailing toolbar with an 8 pt gap. Content follows native safe areas; the silver background extends behind them.

At normal reading sizes in portrait, the full-width 64 pt LCD sits above the list with 12 pt horizontal and 8 pt vertical outer padding. A wrapping freshness footer sits below the list. When the vertical size class is compact or Dynamic Type is an accessibility size, the LCD becomes 48 pt high and moves into the first scrolling list row; the footer is omitted. Freshness remains available in Settings. This uses native size classes and Dynamic Type, not pixel breakpoints.

Each playlist row places `PlaylistSummary` beside an outward arrow with a 12 pt gap. Its content has a 44 pt minimum height plus 5 pt vertical padding on each side, and grows with text. The shared summary has 5 pt vertical spacing. System list insets, separators, and scrolling remain native.

## Elevation & Depth

`SilverSurface` tiles `../shared/Sources/SpinitronUI/Resources/brushed-silver.png` at 0.85 opacity over `paper`, then adds a faint white-to-black vertical wash. Keys retain the fixed continuous rim, stepped metal gradient, and half-point inner bevel from the Mac implementation. Pressing moves the label down 1 pt and changes the face to the LCD gradient. Disabled keys use 0.48 opacity; the 0.09-second ease-out animation is disabled by Reduce Motion.

The LCD has a graphite gasket, inset upper shade and lower highlight, diagonal reflection, and subtle glyph shadow. Texture and glass decoration do not intercept touches. Native sheets, alerts, and search retain their platform depth; there is no additional card-shadow system.

## Shapes

Shared controls use small rounded rectangles; the LCD has a slightly wider radius. Playlist targets remain full-width rows. System search, sheets, station choices, and settings keep their native shapes. Touch controls do not inherit the Mac keys' fixed 30 pt height.

## Components

- **Touch keys:** `HiFiButtonStyle(touch: true)` gives Refresh, Settings, and the initial-load Refresh button at least 44 × 44 pt. The toolbar uses compact horizontal padding. Refresh shows progress and disables while refreshing. Shared hover styling may support a pointer, but no action depends on hover.
- **Station LCD:** the entire display is a button with a down chevron. It opens a native confirmation dialog containing All stations, available stations, and Cancel. All stations is displayed as `ALL`; the accessibility value remains “All stations.”
- **Search:** native `.searchable` filters the downloaded catalog for the chosen station. Autocorrection and automatic capitalization are disabled. Scrolling dismisses the keyboard interactively.
- **Catalog:** rows show title, sampled artists, and track count, with station metadata in All stations and an “Older collection” label when applicable. The entire row attempts to open its Spotify playlist. Empty results use `ContentUnavailableView`; pull-to-refresh updates the catalog.
- **Refresh error:** a warm banner keeps the catalog visible. Retry and Dismiss each own a 44 × 44 pt minimum target inside their button labels, including the padded area. Dismiss removes the banner.
- **Settings:** a native sheet contains a `Form`, default-station picker, older-collection toggle, cache count/freshness, refresh action, and Done. Controls use native Dynamic Type and accessibility behavior.
- **Spotify fallback:** a failed app-open attempt presents the native “Spotify isn’t available” alert, with Open in browser and Cancel. It is a navigation handoff, not an in-app player.

## Do's and Don'ts

### Do:

- **Do** reuse the shared HiFi material, palette, vector glyphs, and touch button style.
- **Do** preserve native controls, 44 pt touch targets, semantic text styles, and growing playlist rows.
- **Do** keep the selected light Silver Hi-Fi appearance under either system appearance.

### Don't:

- **Don't** replace the selected silver and blue skin with another visual world.
- **Don't** apply the Mac popover's fixed width, tiny type, or 30 pt key height to iPhone.
- **Don't** treat a screenshot or a Simulator handoff alert as physical-device or Spotify playback validation.
