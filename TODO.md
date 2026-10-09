# TODO

Things noticed along the way. They are written here instead of being built early, because the
project is built **one feature at a time**.

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
- Optional: rename the local folder `E:\Ferry` to `BitDinghy`. The dev drive is a FAT32 USB stick;
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
