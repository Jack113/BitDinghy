# Changelog

All notable changes are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

Saved settings and download lists from older versions must keep working. Any change to a saved
format is called out here.

## [Unreleased]

### Added
- **Download from a magnet link**, in both the window and the command line
  (`dinghy get "<link>"`). Shows progress, speed, time left and how many people you are connected
  to. An interrupted download carries on where it stopped when you run it again.
- **File-name safety.** Every file name in a torrent is checked before anything is written, so a
  torrent can never put files outside your download folder. This covers Windows-only tricks that
  the download engine itself misses, such as drive-letter names (`C:evil`) and device names
  (`NUL`, `CON`).
- **Magnet links are cleaned before use.** Only the torrent fingerprint, a tidied name and a few
  ordinary internet trackers are kept. This blocks a hostile `so=` file-range that could make the
  engine run out of memory, and trackers that point at your own computer or local network.
- Project skeleton: a Cargo workspace with the shared engine (`dinghy-core`), the `dinghy`
  command line and the `BitDinghy` window.
- Automatic checks on Windows and Linux: formatting, lints, tests, plus a license and
  vulnerability check of all dependencies.
- Dual license: MIT OR Apache-2.0.

### Known limits
- No pause/resume buttons, no saved download list, no `.torrent` files, no settings yet.
- The torrent engine does not support encrypted connections yet.
