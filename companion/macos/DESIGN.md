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
  station-fallback: { fontFamily: "SF Mono", fontSize: "23pt", fontWeight: 500 }
  title: { fontFamily: "SF", fontSize: "13pt", fontWeight: 600 }
  body: { fontFamily: "SF", fontSize: "12pt", fontWeight: 400 }
  artist: { fontFamily: "SF", fontSize: "11pt", fontWeight: 400 }
  button: { fontFamily: "SF Mono", fontSize: "12pt", fontWeight: 600 }
  date: { fontFamily: "SF Mono", fontSize: "10pt", fontWeight: 600, letterSpacing: "0.6pt" }
  subtitle: { fontFamily: "SF Mono", fontSize: "9pt", fontWeight: 600, letterSpacing: "1.1pt" }
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
  button-selected:
    textColor: "{colors.ink}"
    typography: "{typography.button}"
    rounded: "{rounded.control}"
  search:
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0pt 10pt"
    height: "33pt"
  station-display:
    textColor: "{colors.ink}"
    rounded: "{rounded.lcd}"
    padding: "0pt 14pt"
    height: "64pt"
  station-preset:
    height: "64pt"
  station-menu:
    width: "24pt"
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
- Blue LCD and selected controls; navy text on pale playlist rows.
- Native SF typography, custom dot-matrix lettering, and SF Symbols.

The implementation authority is `Sources/SpinitronMenu/HiFiSkin.swift` and `CatalogView.swift`. All dimensions here are native macOS points. Color percentages preserve the exact SwiftUI sRGB channels; they do not imply CSS rendering. The button-rim correction received a **ship** review: the loose white lower lip is gone on selected and ordinary controls, with no visual, size, or interaction regressions found.

## Colors

### Primary

LCD blue runs from `lcd-top` to `lcd-bottom`. `blue` marks search focus, button hover outlines, and outbound arrows; `highlight` is the playlist hover fill.

### Neutral

`ink` is deep navy for primary text. `secondary` softens artist samples and status. `paper` backs the inset catalog; `edge` defines the chassis and controls. Silver texture belongs to the frame, settings surface, and date dividers.

## Typography

SwiftUI's system font supplies SF; `.monospaced` supplies SF Mono. Keep the frontmatter hierarchy, native metrics, and monospaced counts. Titles wrap to two lines; artist samples stay on one line.

The brand and short station names use custom 5 × 7 vector glyphs, not a font asset. Glyph pitch is 1.65 pt for the brand and 3.4 pt for the LCD; each character advances six cells. Station names longer than six characters or outside the implemented glyph set use the SF Mono fallback with a minimum scale of 0.65. Dot-matrix text retains an ordinary-text accessibility label.

## Layout

The AppKit popover is fixed at 520 × 640 pt, with a 4 pt outer edge, 10 pt content inset, and 8 pt major stack spacing. The inset catalog expands into the available space. Its lazy vertical list scrolls under pinned date headers. Rows have a 58 pt minimum height and expand for wrapped titles. The separate settings content is 420 × 290 pt, with 22 pt padding and 18 pt section spacing. These are desktop layouts; there are no web breakpoints.

## Elevation & Depth

The chassis and catalog use one-point edge strokes and inset light/dark bevels. Buttons have a fixed, continuous `edge` rim around a metal face with a narrow mid-face gradient step. Their half-point inner bevel fades to clear at the bottom; the button surface has no drop shadow or separate outer stroke. The LCD sits behind a graphite gasket, with top shadow, bottom highlight, and subtle diagonal glass reflection. `SilverSurface` tiles the bundled `Resources/brushed-silver.png` at 0.85 opacity over `paper`, with a subtle vertical white-to-black overlay. The brand retains its one-point white etched shadow. Exact material, bevel, and motion details are recorded in the sidecar.

## Shapes

Small rounded rectangles distinguish the controls, LCD, and chassis using the frontmatter radii. Playlist rows remain full-width rectangular targets. Dividers pair a one-point dark rule with a one-point white rule.

## Components

- **Hi-Fi buttons:** minimum width 30 pt and default fixed height 30 pt, with 13 pt horizontal padding or 8 pt for compact controls. Station presets override the height to match the LCD. The socket stays fixed; pressing shifts the label down 1 pt. Face insets are 1 pt horizontally and at the bottom; the top inset increases from 1 to 2 pt when selected or pressed. Selected or pressed faces use the opaque LCD gradient. Selection is indicated by the blue face without an underline. Enabled hover adds a 1 pt blue outline and, when not pressed, a faint white face overlay. Disabled opacity remains 0.48; the 0.09-second ease-out press animation respects Reduce Motion.
- **Station LCD and presets:** the LCD, presets, and station menu share `HiFi.stationControlHeight` (64 pt), aligning their top and bottom edges. LCD content is vertically centered with 14 pt horizontal padding; the dropdown is 24 pt wide. The display has a 1 pt outer rim, 2 pt graphite gasket inset 1 pt, and a 0.5 pt inner edge inset 3 pt. A 9 pt top shadow and 5 pt bottom highlight shade the subtle glass reflection. Dot-matrix lettering includes inactive cells at 0.08 ink opacity; its tracked subtitle and SF Mono fallback remain. Two station presets sit beside a native menu using the same button surface; selection retains its accessibility trait.
- **Search:** plain native text field inside a translucent white recess. A 2 pt blue outline marks focus. Its custom placeholder uses `secondary`, ignores pointer events, and is hidden from accessibility. A clear button appears for nonempty queries. ⌘F focuses search.
- **Catalog rows:** navy title, three sampled artists when available, track count, and blue outbound arrow. The entire row opens the playlist in Spotify. Hover adds the blue fill. All-stations and older-collection rows add identifying metadata.
- **Date dividers and footer:** etched silver headers carry uppercase Pacific dates and counts. The footer carries cache status and Quit. Refresh uses a native progress indicator, disables during refresh, and supports ⌘R.
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
