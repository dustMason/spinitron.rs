---
name: Spinitron — Silver Hi-Fi
description: A compact native macOS radio-catalog browser in satin silver and blue LCD.
colors:
  ink: "rgb(8% 18% 29%)"
  secondary: "rgb(25% 33% 41%)"
  edge: "rgb(37% 42% 47%)"
  paper: "rgb(88% 90% 92%)"
  blue: "rgb(4% 34% 56%)"
  highlight: "rgb(68% 84% 97%)"
  lcd-top: "rgb(53% 76% 94%)"
  lcd-bottom: "rgb(77% 89% 98%)"
typography:
  title: { fontFamily: "SF", fontSize: "13pt", fontWeight: 600 }
  body: { fontFamily: "SF", fontSize: "12pt", fontWeight: 400 }
  artist: { fontFamily: "SF", fontSize: "11pt", fontWeight: 400 }
  button: { fontFamily: "SF Mono", fontSize: "12pt", fontWeight: 600 }
  date: { fontFamily: "SF Mono", fontSize: "10pt", fontWeight: 600, letterSpacing: "0.6pt" }
  metadata: { fontFamily: "SF", fontSize: "10pt", fontWeight: 400 }
rounded:
  control: "3pt"
  lcd: "4pt"
  frame: "9pt"
spacing:
  tight: "4pt"
  label: "6pt"
  section: "8pt"
  panel: "10pt"
  row-horizontal: "12pt"
  settings: "22pt"
components:
  button:
    textColor: "{colors.ink}"
    typography: "{typography.button}"
    rounded: "{rounded.control}"
    padding: "0pt 13pt"
    height: "30pt"
  button-pressed:
    textColor: "{colors.ink}"
    typography: "{typography.button}"
    rounded: "{rounded.control}"
  title-action:
    width: "30pt"
    height: "30pt"
  search:
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0pt 10pt"
    height: "33pt"
  station-display:
    textColor: "{colors.ink}"
    rounded: "{rounded.lcd}"
    padding: "8pt 14pt"
    height: "64pt"
  station-menu:
    height: "64pt"
  playlist-row:
    textColor: "{colors.ink}"
    typography: "{typography.title}"
    padding: "10pt 12pt"
  playlist-row-hover:
    backgroundColor: "{colors.highlight}"
  catalog:
    backgroundColor: "{colors.paper}"
    rounded: "{rounded.control}"
---

# Design System: Spinitron — Silver Hi-Fi

## Overview

**Creative North Star: "Silver Hi-Fi"**

Jordan selected this direction from the three-way Winamp concept board on October 6, 2026. Satin brushed silver, a blue dot-matrix LCD, tactile beveled controls, and compact navy text give the native catalog browser its identity. Preserve the chosen material and density when extending it.

**Key Characteristics:**

- Satin silver chassis and shallow machined bevels.
- Blue LCD and pressed controls; navy text on pale playlist rows.
- Native SF typography, custom dot-matrix lettering, and SF Symbols.

The implementation authority is `../shared/Sources/SpinitronUI/HiFiSkin.swift`, `../shared/Sources/SpinitronUI/PlaylistSummary.swift`, and `Sources/SpinitronMenu/CatalogView.swift`. All dimensions here are native macOS points. Color percentages preserve the exact SwiftUI sRGB channels; they do not imply CSS rendering. The aligned LCD/search/catalog layout received a **ship** review, including the matching Refresh/Settings controls and native LCD menu. Native menu opening, keyboard station selection, and top-bar Refresh were verified.

## Colors

### Primary

LCD blue runs from `lcd-top` to `lcd-bottom`. `blue` marks search focus, button hover outlines, and outbound arrows; `highlight` is the playlist hover fill.

### Neutral

`ink` is deep navy for primary text. `secondary` softens artist samples and status. `paper` backs the inset catalog; `edge` defines the chassis and controls. Silver texture belongs to the frame, settings surface, and date dividers.

## Typography

SwiftUI's system font supplies SF; `.monospaced` supplies SF Mono. Keep the frontmatter hierarchy, native metrics, and monospaced counts. Titles wrap to two lines; artist samples stay on one line.

The brand and all station names use custom 5 × 7 vector glyphs, not a font asset. Station display names are uppercase; the glyph set covers A–Z, digits, and common punctuation. Glyph pitch is 1.65 pt for the brand. The station title uses 85% of the LCD’s 48 pt lettering area with pitch `0.85 * min(availableHeight / 7, availableWidth / glyphColumns)`; each character advances six cells. Longer names scale uniformly to fit the width. There is no subtitle. Unsupported characters display a question-mark glyph rather than disappearing. The native station menu exposes the current station name as its accessibility value; its decorative LCD background is hidden from accessibility.

## Layout

The AppKit popover is fixed at 520 × 640 pt, with a 4 pt outer edge, 10 pt content inset, and 8 pt major stack spacing. The LCD, search field, and catalog fill the same width with aligned left and right edges. Refresh sits immediately left of Settings in the title bar; both are 30 × 30 pt with an 8 pt gap. The inset catalog expands into the remaining space. Its lazy vertical list scrolls under pinned date headers. Rows have a 58 pt minimum height and expand for wrapped titles. The separate settings content is 420 × 290 pt, with 22 pt padding and 18 pt section spacing. These are desktop layouts; there are no web breakpoints.

## Elevation & Depth

The chassis and catalog use one-point edge strokes and inset light/dark bevels. Buttons have a fixed, continuous `edge` rim around a metal face with a narrow mid-face gradient step. Their half-point inner bevel fades to clear at the bottom; the button surface has no drop shadow or separate outer stroke. The LCD sits behind a graphite gasket, with top shadow, bottom highlight, and subtle diagonal glass reflection. `SilverSurface` tiles the bundled `../shared/Sources/SpinitronUI/Resources/brushed-silver.png` at 0.85 opacity over `paper`, with a subtle vertical white-to-black overlay. The brand retains its one-point white etched shadow. Exact material, bevel, and motion details are recorded in the sidecar.

## Shapes

Small rounded rectangles distinguish the controls, LCD, and chassis using the frontmatter radii. Playlist rows remain full-width rectangular targets. Dividers pair a one-point dark rule with a one-point white rule.

## Components

- **Hi-Fi buttons:** minimum width 30 pt and default fixed height 30 pt, with 13 pt horizontal padding or 8 pt for compact controls. The socket stays fixed; pressing shifts the label down 1 pt. Face insets are 1 pt horizontally and at the bottom; the top inset increases from 1 to 2 pt when pressed. Pressed faces use the opaque LCD gradient. Enabled hover adds a 1 pt blue outline and, when not pressed, a faint white face overlay. Disabled opacity remains 0.48; the 0.09-second ease-out press animation respects Reduce Motion.
- **Station LCD and menu:** the entire full-width LCD opens the native station menu. A clear Menu label fills its 64 pt height, with the noninteractive `StationDisplay` as an accessibility-hidden background. The menu is labeled “Choose station” and exposes the current station or “All stations” as its accessibility value. It contains “All stations” and every available station, with no separate dropdown control. The station title is scaled to 85% of its fitted size and vertically centered with 14 pt horizontal and 8 pt vertical padding. The display retains its 1 pt outer rim, 2 pt graphite gasket inset 1 pt, and 0.5 pt inner edge inset 3 pt; its 9 pt top shadow, 5 pt bottom highlight, and subtle glass reflection are unchanged. Dot-matrix lettering includes inactive cells at 0.08 ink opacity; all station names use the same scaled lettering without a subtitle.
- **Search:** full-width, 33 pt-high plain native text field inside a translucent white recess. A 2 pt blue outline marks focus. Its custom placeholder uses `secondary`, ignores pointer events, and is hidden from accessibility. A clear button appears for nonempty queries. ⌘F focuses search.
- **Title actions:** Refresh and Settings are the two compact top controls; there is no Close button. Refresh supports ⌘R, uses a native progress indicator, and disables during refresh.
- **Catalog rows:** navy title, three sampled artists when available, track count, and blue outbound arrow. The entire row opens the playlist in Spotify. Hover adds the blue fill. All-stations and older-collection rows add identifying metadata.
- **Date dividers and footer:** etched silver headers carry uppercase Pacific dates and counts. The footer carries cache status and Quit.
- **Settings and content states:** retain the native picker and checkbox. Loading, empty, and error content stays inside the established frame; errors use the implemented warm tinted banner. Both windows explicitly use the light appearance.

## Do's and Don'ts

### Do:

- **Do** reuse the HiFi palette, material, bezel, and button components.
- **Do** preserve native keyboard, menu, focus, and accessibility behavior.
- **Do** render real catalog records and let long titles increase row height.

### Don't:

- **Don't** replace the satin silver, blue LCD, or compact navy typography with a different visual world.
- **Don't** rasterize catalog text or replace native controls with web controls.
- **Don't** carry illustrative concept-board records or placeholder rows into the live catalog.
