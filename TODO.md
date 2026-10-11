# TODO

Things noticed along the way. They are written here instead of being built early, because the
project is built **one feature at a time**.

## Owner checks still open
- Done: M2 tested OK by the owner, window included (2026-10-10).
- Done: `assets/source/` stays in git history (owner's decision, 2026-10-10).
- Done: the project folder is renamed to `BitDinghy`.
- Done: the M1 download was checked against the published SHA256 and matched (2026-10-09).

## Revisit at the end (owner's decision, 2026-10-10)
- The window is **dark only** for now. The light theme and the "follow Windows' light/dark setting"
  behaviour are still in the code (`brand.rs`, one line in `apply`: `ctx.set_theme(Dark)`), so
  bringing them back means choosing: follow the system, a Settings choice, or stay dark. Decide
  before the first release (M18). Update `docs/BRAND.md`, `CLAUDE.md` and the website wording then.

## Noticed while building M10
- In a test, a leecher that connected while BitDinghy was still downloading never learned about
  the pieces BitDinghy got afterwards (it stayed at 0%). Connected when BitDinghy already had
  everything, it downloaded normally. Check whether librqbit announces new pieces ("have"
  messages) to peers connected earlier; if not, report it upstream.
- A per-download sharing limit (different from the setting for all) could come later.

## Noticed while building M6
- The proper home for the leftovers of unticked files: a hidden "parts" file of BitDinghy's own
  (like qBittorrent), so unticked files never appear in the folder at all. Needs our own storage
  layer under librqbit (its `StorageFactory`), and moving data between the parts file and the
  real file when a file is ticked or unticked. Until then they are marked hidden (owner's
  decision, 2026-10-11).
- Linux has no "hidden" mark; the leftovers stay visible there. Decide with the Linux port (the
  parts file above would solve it everywhere).
- FAT32 and exFAT drives can't hold "sparse" files, so a big unticked file next to a ticked one
  takes its full size on such a drive. Say so in plain language with the drive warnings (M12).
- The Files list adds up file sizes (4.3 MiB) while the progress total counts whole pieces
  (4.4 MiB), because a piece can spill into an unticked neighbour. Explain it if people ask.
- A big torrent's file list could use a folder tree, a filter box, and "tick this folder".

## Noticed while building M5
- Web links to `.torrent` files check addresses as WRITTEN (like trackers): a public-looking name
  that resolves to a home-network address would still be fetched. (The M5 review's other
  written-address gaps are closed: IPv4-mapped IPv6, `localhost.`, dotless names, 100.64/10.) Closing it needs a custom DNS
  resolver in `reqwest` that refuses private addresses (`reqwest::dns::Resolve`). Do it with the
  tracker gap.
- Done in M8: `dinghy settings set` reaches an open window straight away (allow-http-links and
  max-active-downloads; the download folder still applies at the next start).
- The window's Settings section shows its own copy of the settings: a change made with `dinghy`
  while the window is open applies, but the window's boxes show the old value until it restarts.
- Opening `.torrent` files / magnet links from Explorer or a browser is M16 (default app).
- `dinghy get` following a download in the open window shows a paused download as
  `76.6% ... 0 B/s  ETA unknown  peers 0`, which looks stuck (found in the M5 guided test). Say
  "paused at 76.6%" like `dinghy list` does (`progress_line` in the CLI). Fits M12.

## Noticed while building M4
- Another user on the same PC could create a pipe with BitDinghy's channel name before the window
  opens. The window then says its channel is in use (it never shares it), but `dinghy` would talk
  to the impostor, which could see the links sent and answer falsely. Checking that the pipe
  belongs to the same user (`GetNamedPipeServerProcessId` + the process's user) closes it.
  Low risk on a one-person PC; do it before the first release. Simplest check (from the M4
  security review): read the pipe's owner with `GetSecurityInfo` on the client handle and require
  the current user's SID. The pipe name isn't secret (anyone can list pipes), and the client
  already uses SECURITY_IDENTIFICATION, so an impostor can't act as the user.
- Linux: the socket gets mode 0600 just after `bind`; for the Linux port, create it inside a
  0700 folder or check the caller with `SO_PEERCRED`.
- `dinghy get --no-dht/--no-listen` with the window open: the window's settings apply (a note is
  printed). If they should be strict, refuse them like `--dir`.
- Linux: a socket file's path is limited to about 108 bytes. A deeply nested portable data folder
  could exceed it; fall back to a short path under `$XDG_RUNTIME_DIR` with the Linux port.
- `dinghy get` while a `dinghy get` runs in another terminal is still refused ("already
  running"); only the window answers requests. Fine for now.

## Noticed while building M3
- Done in M7: "Use the files already there" (window button, `dinghy get --use-existing-files`)
  answers both the re-add after "Remove, keep the files" and `dinghy get` without a data folder.
- Removing with "delete the files" can hold the window for up to two seconds when Windows keeps
  a file busy. Fine for now; revisit if it ever shows.
- `dinghy get --no-dht` on a link with no trackers and no `--peer` waits forever ("Looking for
  people...") although it has no way to find anyone. Say so straight away (plain-language errors,
  M12).
- `rusqlite` with `bundled` compiles SQLite into both programs. Re-measure the exe sizes against
  the M0b baseline at the first release.

## Noticed while building M2
- Changing the download folder only applies on the next start. Applying it live fits M3, when the
  engine and the saved list get reworked.
- No folder-picker button yet: the folder is typed. A native dialog (`rfd`) is the way, but its
  Linux back end needs GTK or a portal, so decide with the Linux port.
- The Inter font from the brand sheet: no font file in the pack, so egui's built-in font is used.
- Linux AppImage: the program sits inside a read-only image, so the portable data folder must be
  placed next to the `.AppImage` file itself (use the `APPIMAGE` environment variable), not next to
  the program inside it.
- The icon and Details tab are added by `build.rs`; if the Windows resource compiler is missing it
  only warns. The release workflow should fail instead, and check the Details tab exists.
- `toml_parser` 1.1.5 and `toml_datetime` 1.1.2 were published a day before they were added, and the
  `toml` crate we chose requires them. Re-check them with `cargo deny` and a fresh look at the
  advisories after a couple of weeks.
- Brand pack extras not used yet: `web/` (favicons, OG image, manifest) and `themes/*.css` are for
  the website milestone.

## Planned later (from the original brief)
- "Check for updates" button that looks at the website.
- "Only download while my VPN is connected".

## Features noted during planning
- Let the window attach to an engine already running in the command line (v1 just says so).
- Warn in plain language when the download drive can't hold the file (FAT32 has a 4 GB file
  limit; the dev USB stick is FAT32). Belongs with the plain-language errors milestone (M12).
- Encryption (MSE) once upstream librqbit merges it (rqbit PR #633). v1 ships without, and
  Settings says so honestly.
- "Auto" speed becomes "Recommended settings", learned from real downloads. No speed-test server,
  because that would break the privacy promise.
- Optional download source types (web links, PC-to-PC) can reuse the same engine slot. Not planned.

## Noticed while building M1
- Names that are fine on Linux but impossible on Windows (for example `Album: Title.mp3`) are
  currently refused with a clear message. A friendlier fix is to rename them while saving, which
  needs a custom storage layer in `dinghy-core`.
- Overwrite policy: files that already exist in the download folder are reused or overwritten,
  which is what makes resuming work. Consider a folder per torrent for single-file torrents, or
  remembering which files we created (M3).
- Seeds versus peers: librqbit reports connected and known peers but not which of them are seeds.
  The brief asks for both, so compute it from the peers' progress if the engine allows.
- Cancel/remove a download (M3 together with pause and resume).
- Report to the librqbit project that `Magnet::parse` accepts an unbounded `so=` range (memory
  exhaustion). We already work around it; whether to report it is the owner's call.
- Keep the DHT's saved contacts in the data folder (M2). A cold start sometimes takes 30 s or more
  to find peers.
- Tracker filter gap (found in the M1 security review): we drop trackers written as a local
  address, but a normal-looking host name that an attacker points at `192.168.x.x` still gets
  through, and the engine would send a web request to it. Closing it properly needs control over
  the engine's own connections (a custom connector that checks the address it resolved to).
  Peers handed out by trackers, the DHT or PEX can also be local addresses; that is true of every
  BitTorrent client.
- Allow trackers on a private network (we drop them by default).
- Same-name overwrite (M1 security review): because downloads reuse files that already exist
  (needed for resuming), a torrent that has a file with the same name as an earlier download in
  the same folder will replace it. Fix with M3: remember which files each torrent created, or give
  every torrent its own folder.
- "Open folder" button on finished downloads.
- The first run triggers a Windows Firewall prompt. Explain it in the app when port and UPnP
  settings arrive (M11).
- librqbit reserves each file's full size on disk at the start. On FAT32, or when the disk is
  nearly full, that fails; give a plain-language message (M12).

## Engineering
- Decide on accessibility (`accesskit` feature of eframe) before the first release.
- Test the window on a VM or Remote Desktop session without a GPU. If OpenGL fails, add a `wgpu`
  fallback.
- Tune the release profile (`opt-level`, `strip`, `panic`) at the first release. Remember crash
  reports need function names, so stripping has a cost. Report real size and memory numbers.
- Add an MSRV (minimum Rust version) CI job.
- Add Dependabot or Renovate for crates and GitHub Actions.
- Native crash handling (access violations) beyond Rust panics.
- Hardening: reject tracker URLs that point at localhost or the LAN, cap `.torrent` size and
  file counts, and fuzz the torrent and magnet parsers.
- Generate the third-party license notices file with `cargo-about` before the first release.
- Privacy check before the first release: confirm with a network monitor that BitDinghy only
  talks to peers, trackers, the DHT and the router.
- Optional: rename the local folder `E:\BitDinghy` to `BitDinghy`. The dev drive is a FAT32 USB stick;
  builds already go to `C:\cargo-target`.

## Website and release milestone
- Make the repo public. Required for free signing and free GitHub Pages and Releases.
- Apply for free code signing from SignPath Foundation. It needs an OSI license, a code signing
  policy page with a privacy statement, and manual approval of each release. Fallback: a paid OV
  certificate or Certum open-source certificate. Microsoft's Artifact Signing is not available in
  every country.
- Donations: GitHub Sponsors plus Ko-fi. Ask an accountant about
  tax on donations. No pop-ups, only a small "Support BitDinghy" link in Settings/About and on the site.
- Website on Cloudflare Pages (free, works with private repos), DNS for `bitdinghy.com` at
  Cloudflare, and a `bitdingy.com` redirect for the common misspelling. Keep the wording neutral:
  a fast, general download tool.
- Trademark: do a free search (WIPO Global Brand Database, USPTO) before release. A local trademark filing
  costs a fee per class and takes years, so registering is optional.
- Enable GitHub private vulnerability reporting once the repo is public.
- Checksums shown on the download page for every file, built automatically on GitHub.

## Linux port (after all Windows features)
- AppImage as the portable format (note: needs FUSE 2 on newer Ubuntu).
- Linux "make default for magnet and .torrent" via a `.desktop` file and `xdg-mime`.
- Local channel uses a Unix socket in the runtime folder instead of a Windows named pipe.
