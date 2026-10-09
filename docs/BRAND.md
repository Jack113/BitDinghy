# How BitDinghy uses the brand assets

The brand pack lives in [`assets/`](../assets/README.md). That README describes the pack itself
(it is written for the website and installers too). This page covers only what the **window and
the programs** do with it.

## What is used, and where

| Asset | Used for | How |
|---|---|---|
| `assets/themes/themes.css` | The window's colors in the light and dark theme | Copied into `crates/bitdinghy-app/src/brand.rs` as the `DARK` and `LIGHT` palettes, with the same names (`accent`, `danger`, ...) |
| `assets/brand/colors.json` | The raw brand colors | A test in `brand.rs` fails if the code's colors ever differ from this file |
| `assets/logos/horizontal-logo-dark.png`, `...-light.png` | The logo at the top of the window | Cropped and shrunk by `tools/make-app-images.ps1` into `crates/bitdinghy-app/assets/logo-on-dark.png` and `logo-on-light.png` |
| `assets/icons/windows-app-icon-256.png` | The window's title-bar and taskbar icon | Compiled into the program and decoded at start |
| `assets/icons/windows-app.ico` | The icon of `BitDinghy.exe` and `dinghy.exe` in Explorer | Embedded by each program's `build.rs` (Windows builds only) |
| (from `Cargo.toml`) | Product name, description, version, copyright on the file's **Details** tab | Also set by `build.rs`. Code signing later requires these |

Everything is compiled into the programs, so there are no extra files to carry around.

## Light and dark

The window follows the computer's own light/dark setting. The dark theme uses the bright
`#57F000` green; the light theme uses the darker `#1F8F46`, because the bright green is hard to read
on white (the pack's own README says the same).

Status colors follow the pack's tokens: **teal** for "in progress", **green** for "finished",
**amber** for notices, **red** for errors.

## Changing a logo

1. Replace the file in `assets/logos/`.
2. Run `powershell -File tools\make-app-images.ps1`.
3. Rebuild.

## Still to do

- **Inter typeface.** The brand font is Inter, but the pack ships no font file, so the window uses
  egui's built-in font. Adding Inter (SIL OFL license) costs about 300 KB per weight and its license
  text has to go into the third-party notices.
- **`assets/source/`** holds two large original boards (about 1.4 MB each). They are tracked in git
  as committed; because git keeps every version of a file forever, consider removing them from the
  history before the repository goes public.
- **Brand-colored widgets.** Only the basics are themed (backgrounds, text, borders, buttons,
  progress, status colors). Tags, cards and the navigation style from the UI preview come with the
  tabbed layout (Downloads, History and charts, Settings).
