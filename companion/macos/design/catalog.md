# Catalog popover

Mode: Operate. The task is finding a broadcast and opening it in Spotify.

## Direction contract

THESIS: A small silver hi-fi browser whose playlist list is its main surface.

OWN-WORLD: Brushed silver frame, shallow machined bevels, pale blue LCD with
dark dot-matrix station lettering, navy text, blue row hover, quiet etched labels.

STORY: Select the station, scan dated broadcasts and three sampled artists,
then open the chosen playlist. Cached content stays available during refresh.

FIRST VIEWPORT: Within the existing 520 × 640-point popover, a narrow title bar
sits above the station display and preset buttons. Search and refresh sit above
the inset scrolling list. Date headers pin. A compact cache-status footer closes
the frame. Rows are full-width buttons and support two-line long titles.

FORM: User-pinned option B, Silver Hi-Fi, approved with “lets do silver hifi!”
Reference: middle panel of the October 6 Spinitron / Skin Studies comparison.
Native text and controls carry the real catalog; the mockup's sample records,
duplicate Refresh button, and accidental placeholder row are not product data.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## Verification

- Release build and app signature verified; installed bundle opens independently.
- Nine native core tests pass, including the exported catalog fixture and title formatting.
- Native search, clear, empty state, KALX/KPOO and All stations, Settings, and
  successful refresh checked with real catalog data.
- Finish review: `ship` for the corrected material texture and search-prompt
  contrast (approximately 6.6:1). Both findings resolved in the final captures.
- `DESIGN.md` and `.impeccable/design.json` record the shipped components.
  The single bundled raster contains its generation prompt as provenance.
