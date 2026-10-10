# Changelog

All notable changes are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

Saved settings and download lists from older versions must keep working. Any change to a saved
format is called out here.

## [Unreleased]

### Added
- **Choose which files to download.** A torrent with several files gets a **Files** button in the
  window: a list with tick boxes, each file's size and how far it got. Untick the files you don't
  want, at any time, while the download runs or is paused. On the command line,
  `dinghy files <n>` shows the list and `dinghy files <n> --pick 1,3-5` (or `all`) chooses. The
  choice is remembered. At least one file stays chosen (to stop everything, pause or remove it).
  The engine still creates unticked files in the folder (empty, or holding a little data it needs
  to check the files next to them), so BitDinghy marks them **hidden**, and folders holding only
  unticked files too. Ticking a file again shows it. **Remove, keep the files** now also shows
  any hidden leftovers again.
- The saved download list moves to **version 2** (it remembers unticked files). Older lists are
  upgraded the first time BitDinghy opens them; `dinghy list` can still read an older list without
  changing it. The messages between `dinghy` and the window are version 2 too, so use the
  `dinghy.exe` that came with your `BitDinghy.exe`.
- **.torrent files.** Open them with the window's new **Open .torrent file...** button (Windows'
  own file dialog), drag them onto the window, paste their path into the link box, or give them to
  `dinghy get`. Every `.torrent` is rebuilt from checked parts only (its file list, plus trackers
  that pass the same filter as magnet links), so trackers pointing at your own PC or home network
  are dropped, and the extra fields in the file are left out.
- **Web links to .torrent files**, in the window's link box and with `dinghy get`. BitDinghy
  fetches them itself, carefully: secure `https://` links only (plain `http://` only if you allow
  it in Settings or with `dinghy settings set allow-http-links on`), never addresses on your own
  PC or home network (also after a redirect), at most 5 redirects, 30 seconds and 16 MB. The
  window fetches in the background, so it never freezes.
- New setting `allow-http-links` (off unless turned on). Older settings files load unchanged.
- `.torrent` files on network locations (`\\server\share\...`) are refused: opening one makes
  Windows contact that computer. Copy the file to this PC first.
- **The command line works while the window is open.** `dinghy list` shows the window's live list
  (with each download's state right now), and `dinghy get <link>` hands the link to the window and
  follows its progress in the terminal (Ctrl+C only stops following). They talk over a local
  channel that only this computer can use: a named pipe that refuses network connections, never a
  network port. With the window closed, `dinghy` works on its own as before.
  `dinghy pause`, `resume` and `remove` work through the open window too. `remove --delete-files`
  first shows exactly which files the window would delete and asks for `yes`; the removal then
  names the download by its fingerprint, so a list that changed in the meantime can't make it
  remove a different download.
- **Downloads are remembered.** Every download goes into a saved list (`downloads.db` in the data
  folder) as soon as it is added. When the window opens again, everything in the list comes back
  and carries on where it stopped, in the folder it was started in. The list is versioned, a
  damaged one is kept aside (`downloads.db.broken-<time>`) and never deleted, and a list from a
  newer BitDinghy is never changed.
- **`dinghy list`** shows the saved list, numbered, with each download's folder. It works while
  the window is open.
- **Only one BitDinghy at a time.** A second window, or `dinghy get` while the window is open, is
  refused with a plain message, so two engines can never write into the same files.
- `dinghy get` on a link that is already in the list carries on in its own folder, using the file
  list saved last time (and resumes it if it was paused).
- **Pause and resume**, in the window (a button on each download) and with `dinghy pause <n>` and
  `dinghy resume <n>` (numbers from `dinghy list`; while the window is closed). A paused download
  stays paused after a restart; one paused before its file list arrived contacts nobody until it
  is resumed. On a download that failed, the button says **Try again**.
- **Remove**, in the window (a **Remove** button that asks: keep the files, delete the files too,
  or cancel) and with `dinghy remove <n>` (add `--delete-files` to delete them; it lists them and
  asks you to type `yes`, or `--yes` for scripts). Only the files BitDinghy downloaded for that
  download are ever deleted, plus folders they leave empty; never the download folder, and never
  anything else in it.
- **Your files are never overwritten.** A new download whose file name already exists in the
  folder is refused with a message naming the file, and nothing is changed. Move the file away and
  press **Try again**. A download that carries on always stays in the folder it was started in,
  and a torrent with several files but no name for their folder is refused (found by the M3
  security review: such a torrent could otherwise move into another folder after a restart).
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
- The window's busy spinners ("Fetching...", "Looking for people...", "Checking files...") were
  invisible, dark on the dark background. They are teal now, like the line beside them.
- Closing the window no longer crashes the program (it panicked while shutting down).

### Known limits
- A new download folder is used from the next start, not immediately.
- A change made with `dinghy settings set allow-http-links` while the window is open reaches the
  window at its next start (the window's own checkbox applies at once).
- The torrent engine does not support encrypted connections yet.
