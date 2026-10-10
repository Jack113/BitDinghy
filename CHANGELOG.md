# Changelog

All notable changes are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

Saved settings and download lists from older versions must keep working. Any change to a saved
format is called out here.

## [Unreleased]

### Added
- **Portable data folder.** BitDinghy keeps its files in a `data` folder next to the program, so you
  can copy the whole folder to a USB stick. If that folder can't be written to, it uses your user
  settings folder instead. `BitDinghy.exe` and `dinghy.exe` share it.
- **Remembered settings.** The download folder is remembered between runs, from the window
  (**Settings**) or the command line (`dinghy settings`, `dinghy settings set download-dir <folder>`).
  The settings file is versioned, saved safely, and never destroyed: a damaged file is kept aside,
  and a file from a newer BitDinghy is left untouched. `dinghy get --dir` still overrides the folder
  for one run.
- The DHT (the shared phone book of peers) now keeps its saved contacts inside the data folder.
- **Brand look.** The window uses the BitDinghy logo, colors and icon, in the brand's dark theme
  (dark only for now). `BitDinghy.exe` and `dinghy.exe` have the brand icon and a
  Details tab with the product name, description and version.
- **Download from a magnet link**, in both the window and the command line
  (`dinghy get "<link>"`). Shows progress, speed, time left and how many people you are connected
  to. An interrupted download carries on where it stopped when you run it again.
- **File-name safety.** Every file name in a torrent is checked before anything is written, so a
  torrent can never put files outside your download folder. This covers Windows-only tricks that
  the download engine itself misses, such as drive-letter names (`C:evil`) and device names
  (`NUL`, `CON`).
- **Magnet links are cleaned before use.** Only the torrent fingerprint, a tidied name and a few
  ordinary internet trackers are kept. This blocks a hostile `so=` file-range that could make the
  engine run out of memory, and trackers written as an address on your own computer or local
  network. (A host name that is secretly pointed at a local address is not caught yet.)
- Project skeleton: a Cargo workspace with the shared engine (`dinghy-core`), the `dinghy`
  command line and the `BitDinghy` window.
- Automatic checks on Windows and Linux: formatting, lints, tests, plus a license and
  vulnerability check of all dependencies.
- Dual license: MIT OR Apache-2.0.

### Fixed
- Closing the window no longer crashes the program (it panicked while shutting down).

### Known limits
- A new download folder is used from the next start, not immediately.
- No pause/resume buttons, no saved download list, no `.torrent` files yet, and only one setting.
- The torrent engine does not support encrypted connections yet.
