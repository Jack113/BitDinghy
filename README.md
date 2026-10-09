# BitDinghy

A fast, lightweight, portable download client for BitTorrent and magnet links, written in Rust.

> **Status: early development.** You can download from a magnet link in the window or with
> `dinghy get`. Everything else is being added one feature at a time (see
> [CHANGELOG.md](CHANGELOG.md) and [TODO.md](TODO.md)). To try it, see
> [docs/TESTING.md](docs/TESTING.md).

## Goals

- **Portable and self-contained.** Download, unzip, run. No installer, no admin rights, and
  nothing else to install. Settings and your download list live next to the program, so it also
  runs from a USB stick.
- **Private.** No tracking, no ads, no data collection. BitDinghy sends nothing anywhere except
  the BitTorrent traffic itself.
- **Safe.** Everything received from the internet is treated as untrusted, and files are never
  written outside the download folder you chose.
- **Light and quick.** Small download, low memory use, fast start. Real numbers are published
  with every release.
- **Free and open source.** Donations are welcome and never pushed on you.

Planned features: magnet links, `.torrent` files and links, choosing files inside a torrent,
speed and connection settings, seeding limits, a download queue, pause/resume that survives
crashes, file verification, speed history charts, plain-language error messages, and matching
`dinghy` command-line commands for everything the window does.

## Two programs, one engine

| Program | What it is |
|---|---|
| `BitDinghy` | The window. |
| `dinghy` | The command line, for example `dinghy add <link>`, `dinghy list`. |

Both are thin front-ends over the same shared engine (`dinghy-core`). See
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
and [egui](https://github.com/emilk/egui) (MIT OR Apache-2.0) for the window. A full
third-party license notice file is added before the first release.

"BitTorrent" is a trademark of its owner. BitDinghy is an independent project and is not
affiliated with or endorsed by them.
