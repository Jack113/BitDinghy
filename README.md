# BitDinghy

A fast, lightweight, portable download client for BitTorrent and magnet links, written in Rust.

> **Status: version 0.1.0, getting ready for its first release.** Windows first; Linux follows.
> What's new is in [CHANGELOG.md](CHANGELOG.md); how to try every feature by hand is in
> [docs/TESTING.md](docs/TESTING.md).

## Goals

- **Portable and self-contained.** Download, unzip, run. No installer, no admin rights, and
  nothing else to install. Settings and your download list live next to the program, so it also
  runs from a USB stick.
- **Private.** No tracking, no ads, no data collection. BitDinghy sends nothing anywhere except
  the BitTorrent traffic itself (and, when you give it one, the web link to a `.torrent` file).
  Crash reports stay on your computer.
- **Safe.** Everything received from the internet is treated as untrusted, files are never
  written outside the download folder you chose, and files BitDinghy didn't download are never
  overwritten or deleted.
- **Light and quick.** Small download, low memory use, fast start.
- **Free and open source.** Donations are welcome and never pushed on you.

## What it does

- Downloads from **magnet links**, **`.torrent` files** (open, drag and drop, or paste a path) and
  **web links to `.torrent` files** (secure `https://` only, unless you allow plain `http://`).
- **Choose which files** to download, at any time.
- **Pause, resume, remove** (keeping or deleting only the files it downloaded); the list survives
  restarts and crashes.
- **Check files** on disk again, and **use files that are already there**.
- A **queue** (how many downloads run at once, in an order you choose), **speed limits**,
  **sharing limits** (ratio or time), and a **port** setting with optional **UPnP**.
- **Recommended settings** learned from your own downloads (never a speed test).
- **History**: speeds of the last ten minutes, daily totals for 30 days, removed downloads.
- **Plain-language problems** (a full disk, a file another program holds, a FAT32 drive...).
- **Crash reports** that stay on your computer.
- Opens **magnet links and `.torrent` files from your browser and Explorer**, if you choose it in
  Windows' Default apps (it always asks before adding).
- Not yet: **encryption** of connections (the engine it is built on doesn't support it yet).

## Two programs, one engine

| Program | What it is |
|---|---|
| `BitDinghy` | The window. |
| `dinghy` | The command line: `dinghy get <link>`, `dinghy list`, `pause`, `resume`, `remove`, `files`, `check`, `move`, `history`, `recommend`, `default-app`, `settings`. `dinghy --help` lists them all. |

Both are thin front-ends over the same shared engine (`dinghy-core`). With the window open,
`dinghy` talks to it over a channel that only this computer can use. See
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Building from source

You need [Rust](https://rustup.rs) 1.95 or newer. On Windows you also need the Visual Studio
"Desktop development with C++" tools. On Linux you need a C compiler plus
`pkg-config libxkbcommon-dev libwayland-dev libx11-dev libgl1-mesa-dev`.

```sh
cargo build --workspace
cargo run --bin dinghy -- --version
cargo run --bin BitDinghy
```

Run the same checks CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

## Using it responsibly

BitDinghy is a general-purpose download tool. Only download and share content you have the right
to. Free, legal examples include Linux distribution images and other openly licensed files.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at
your option. Unless you state otherwise, any contribution you submit is dual-licensed the same
way, with no additional terms.

BitDinghy uses [librqbit](https://github.com/ikatson/rqbit) (Apache-2.0) as its BitTorrent engine
and [egui](https://github.com/emilk/egui) (MIT OR Apache-2.0) for the window. The licenses of
everything BitDinghy is built from are in [THIRD-PARTY-NOTICES.html](THIRD-PARTY-NOTICES.html)
(made with `cargo about`, see [docs/RELEASE.md](docs/RELEASE.md)).

"BitTorrent" is a trademark of its owner. BitDinghy is an independent project and is not
affiliated with or endorsed by them.
