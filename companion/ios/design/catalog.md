# iPhone catalog

Mode: Operate.

THESIS: The Silver Hi-Fi radio browser fits an iPhone without inheriting the
Mac popover's fixed width, tiny type, hover requirements, or Quit control.

OWN-WORLD: Existing brushed silver, graphite LCD gasket, blue dot-matrix station
name, navy type, shallow tactile keys. Existing asset and vector components are
shared with macOS, not replaced by a generated screenshot or a new skin.

FIRST VIEWPORT: A native navigation/search bar, compact full-width station LCD,
and a scrolling dated list with three artist names, track count and outward
arrow. The whole row opens Spotify. Refresh and Settings remain immediately
available; the footer identifies cached content and freshness.

INTERACTION: Native station action sheet and settings form; pull-to-refresh;
normal keyboard search and scrolling. All controls have at least 44pt targets,
body text uses Dynamic Type, long titles expand, and errors keep cached content.
Station choices and settings have ordinary accessibility labels. Spotify absence
offers a browser fallback; no fake playback or hidden selection stage.

APPEARANCE: Fixed light Silver Hi-Fi is user-pinned, matching macOS, including
when the phone uses Dark Mode. Respect Reduce Motion in shared button responses.

ADAPTATION: At compact vertical size or an accessibility Dynamic Type size, the
station LCD changes from 64pt to 48pt and moves into the scrolling list. The
footer is omitted; freshness remains available in Settings. Reading text and
rows grow with Dynamic Type. Retry and Dismiss each own a 44×44pt minimum target
inside the button label, including its padded area.

VERIFIED — 2026-10-13: Final native review disposition: **Ship**, with no material
findings remaining. Twelve shared tests passed on macOS and twelve on iOS; three
native UI tests passed. Generic Simulator Debug and unsigned device-SDK
Release builds passed, alongside the Mac Release build and signature check.
The normal live launch loaded 2,269 catalog records.

Native Simulator captures in `../.impeccable/review/` cover portrait, landscape,
normal text, accessibility XXXL and scrolling, settings at both reading sizes,
KPOO selection, empty search, cached offline refresh, and the missing-Spotify
alert. `iphone-live-dark-system.png` shows live catalog data with the system in
Dark Mode and the app preserving its user-pinned light appearance.

The final focused offline test measured Retry and Dismiss accessibility frames
at least 44×44pt and tapped their lower edges. Dismiss removed the error banner
while cached rows remained. Retry was exercised while already offline; the test
does not independently observe a network request. The updated capture is
`../.impeccable/review/touch-fix/6423A3C1-6137-4A4D-A6EE-E5AEB9198169.png`.

LIMITS: No connected physical iPhone was available. Personal signing, installation,
hardware usability, and real Spotify handoff remain untested. The Simulator
verified the missing-Spotify alert and browser option, not browser navigation or
playback. Tests used the iOS 27 SDK/runtime; iOS 17 runtime behavior is untested.
The desktop was locked, so no fresh Mac GUI capture was made; the installed Mac
app was untouched. Existing CI excludes `companion/` and was not changed.
