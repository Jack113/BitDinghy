# Security policy

BitDinghy talks to strangers on the internet, so security and privacy are part of its design.

## Reporting a vulnerability

Please do **not** open a public issue for a security problem. Report it privately instead:

- **On GitHub:** [report a vulnerability](https://github.com/Jack113/BitDinghy/security/advisories/new)
  (the repository's **Security** tab, **Report a vulnerability**). Only the maintainer sees it.
- **By email:** the address on [bitdinghy.com/security](https://bitdinghy.com/security/), with
  "Security" in the subject.

Please include what you found, how to reproduce it, and the version (`dinghy --version`).

You get a reply within **7 days**, usually sooner. The problem is checked, fixed in a new release
and explained in the changelog; you're told before the fix is published, and thanked by name if
you want. Please give reasonable time for a fix before telling anyone else.

While the project is at the 0.x stage, only the **latest version** gets security fixes.

## What counts

Anything that could put BitDinghy's users at risk, for example: writing or reading files outside
the download folder, overwriting or deleting files BitDinghy didn't download, crashes or
unbounded memory or disk use caused by a malformed torrent, link or peer message, the channel
between `dinghy` and the window being reachable from other computers or other users, any
unexpected outgoing connection, or a release file that doesn't match its checksum or build proof.

## How BitDinghy is designed

These are design goals, carefully tested; BitDinghy is provided "as is", without warranty (see
the licences), and reports of any way around them are very welcome.

- No tracking, no ads, no data collection: it's built to send only BitTorrent traffic (peers,
  trackers, the DHT) and requests the user makes (for example, fetching a `.torrent` link they
  pasted).
- Everything received from the network is treated as untrusted and checked before use.
- It's designed to write only inside the download folder the user chose, whatever file names a
  torrent contains, and to refuse a download that would overwrite files it didn't download.
- The channel between the command line and the window is local only (a named pipe, not a network
  port), and set up for the same Windows user.
- Crash reports are saved on the user's PC and not sent anywhere.
- Releases are built by GitHub from this repository, with a SHA-256 checksum and a signed build
  proof (`gh attestation verify <zip> --repo Jack113/BitDinghy`).
