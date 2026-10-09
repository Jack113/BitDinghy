# Security policy

BitDinghy talks to strangers on the internet, so security and privacy are part of the design.

## Our promises

- No tracking, no ads, no data collection. The program sends nothing anywhere except BitTorrent
  traffic (peers, trackers, the DHT) and requests the user makes (for example, fetching a
  `.torrent` link they pasted).
- Everything received from the network is treated as untrusted.
- Files are never written outside the download folder the user chose, whatever file names a
  torrent contains.
- Any link between the command line and the window works only on the same computer. It is not a
  network port and cannot be reached from other machines.
- Crash reports are saved on the user's PC and are never uploaded.

## Reporting a vulnerability

Please do **not** open a public issue for a security problem.

Use GitHub's private reporting: open the repository's **Security** tab and choose
**Report a vulnerability**. Include what you found, how to reproduce it, and the version
(`dinghy --version`).

While the project is at the 0.x stage, only the latest version is supported.

## What counts

Examples we care about: writing or reading files outside the download folder, crashes or
excessive memory use caused by a malformed torrent or peer message, the local command-line link
being reachable from other computers, and any unexpected outgoing connection.
