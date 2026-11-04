# How BitDinghy uses the brand assets

The brand pack lives in [`assets/`](../assets/README.md). That README describes the pack itself
(it is written for the website and installers too). This page covers only what the **window and
the programs** do with it.

## What is used, and where

| Asset | Used for | How |
|---|---|---|
| `assets/themes/themes.css` | The window's colors in the light and dark theme | Copied into `crates/bitdinghy-app/src/brand.rs` as the `DARK` and `LIGHT` palettes, with the same names (`accent`, `danger`, ...) |
| `assets/brand/colors.json` | The raw brand colors | A test in `brand.rs` fails if the code's colors ever differ from this file |
| `assets/logos/horizontal-logo-dark.png`, `...-light.png` | The still logo, shown only if the moving logo's pictures can't be read | Cropped and shrunk by `tools/make-app-images.ps1` into `crates/bitdinghy-app/assets/logo-on-dark.png` and `logo-on-light.png` |
| The website's animated logo (`static/brand/horizontal-logo-dark-animated.svg` in the website repository) | The moving logo at the top of the sidebar | Cut into five still drawings in `tools/logo-layers/*.svg` (same shapes and colors); `tools/make-logo-layers.ps1` turns them into `crates/bitdinghy-app/assets/logo/*.png` with Edge, and `crates/bitdinghy-app/src/logo.rs` moves them the way the SVG does (egui can't play an SVG's own animation). The name "BitDinghy" beside it is drawn as text |
| Inter (Regular and SemiBold, version 4.1, from github.com/rsms/inter) | All text in the window | `crates/bitdinghy-app/assets/fonts/`, with its licence; listed in `THIRD-PARTY-NOTICES.html` through `about.hbs` |
| `assets/icons/windows-app-icon-256.png` | The window's title-bar and taskbar icon | Compiled into the program and decoded at start |
| `assets/icons/windows-app.ico` | The icon of `BitDinghy.exe` and `dinghy.exe` in Explorer | Embedded by each program's `build.rs` (Windows builds only) |
| (from `Cargo.toml`) | Product name, description, version, copyright on the file's **Details** tab | Also set by `build.rs`. Code signing later requires these |

Everything is compiled into the programs, so there are no extra files to carry around.

## Light and dark

The window is **dark unless the person chooses otherwise**: Settings > Look offers Dark (the
standard), Light, and "Like Windows" (follows Windows' own light/dark setting). v0.1.0 was dark
only; the choice came with the redesigned window. The dark theme uses the bright
`#57F000` green; the light theme uses the darker `#1F8F46`, because the bright green is hard to read
on white (the pack's own README says the same).

Status colors follow the pack's tokens: **teal** for "in progress", **green** for "finished",
**amber** for waiting and notices, **red** for errors, **gray** for paused.

## The window's layout (the redesign after v0.1.0)

Agreed with the owner from a mock-up (2026-11-04), after the brand pack's own previews
(`assets/previews/dark-theme-preview.png` and `ui-elements-preview.png`):

- **A sidebar** with the animated logo, the three pages (Downloads, History, Settings; the chosen
  one in green on a raised background), and at the bottom the total speeds, the speed limits and
  the version.
- **Downloads:** the link box and the green **Download** button (the one main button on the page),
  then **one card per download**: its name and a **status badge** (Downloading, Queued, Paused,
  Finished, Problem ...), the progress bar, and the details on the left with the buttons grouped
  on the right (the main action filled, the others outlined).
- **History:** four tiles with today's and the last 30 days' totals, then the two charts and the
  removed downloads, each in a card.
- **Settings:** groups in cards (Downloads, Speed, Sharing, Connections, Magnet links and
  .torrent files, Look, About). The result of a change shows in a line along the bottom.
- **Notices** (a crash report, a problem, an offer from another program, the one-time question
  about opening links) are cards with a colored stripe on the left, above whichever page is open.

The building blocks are in `crates/bitdinghy-app/src/widgets.rs` (card, badge, banner, buttons,
sidebar item). Never use egui's "strong" text color: the brand makes it dark navy (the text on the
green button), so it vanishes on the dark background.

## The moving logo

It moves only while something is downloading (the owner's choice), and never when Windows' "Show
animations in Windows" is off. While still it keeps the pose it stopped in. To change it, edit the
drawings in `tools/logo-layers/` (keep the `viewBox`, so the layers stay lined up), run
`powershell -File tools\make-logo-layers.ps1`, and rebuild.

## Changing a logo

1. Replace the file in `assets/logos/`.
2. Run `powershell -File tools\make-app-images.ps1`.
3. Rebuild.

## Still to do

- **The original source boards** (two large AI-made images) are not in the public repository
  (the owner's decision, 2026-11-01); the vector logos and icons in `assets/` are what is used.
- **The website's screenshots** show the v0.1.0 window and need taking again (same clean demo copy
  method as before).
