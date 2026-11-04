# BitDinghy

[![CI](https://github.com/Jack113/BitDinghy/actions/workflows/ci.yml/badge.svg)](https://github.com/Jack113/BitDinghy/actions/workflows/ci.yml)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/Jack113/BitDinghy/badge)](https://scorecard.dev/viewer/?uri=github.com/Jack113/BitDinghy)
[![Latest release](https://img.shields.io/github/v/release/Jack113/BitDinghy)](https://github.com/Jack113/BitDinghy/releases/latest)

A fast, lightweight, portable download tool for **magnet links** and **`.torrent` files**, for
Windows 10 and 11, written in Rust. Free and open source. Website and guide:
**[bitdinghy.com](https://bitdinghy.com)**.

## Download

1. Get **`BitDinghy-<version>-windows-x64.zip`** from the
   **[latest release](https://github.com/Jack113/BitDinghy/releases/latest)** (or from
   [bitdinghy.com/download](https://bitdinghy.com/download/)).
2. Optional but recommended: check it is the real file. In PowerShell, in the folder with the zip:
   `Get-FileHash .\BitDinghy-*-windows-x64.zip -Algorithm SHA256`, and compare the result with
   `SHA256SUMS.txt` on the release page.
3. Unzip it anywhere: your Desktop, a folder of your own, or a USB stick. There is no installer
   and no admin rights are needed.

BitDinghy isn't code-signed yet, so the first time Windows may say "Windows protected your PC".
After checking the checksum, click **More info**, then **Run anyway**.

## Quick start

1. Run **`BitDinghy.exe`**.
2. Copy a **magnet link** (it starts with `magnet:?`) from a website you trust, paste it into the
   box at the top, and press **Download**. A `.torrent` file works too: drag it onto the window
   or click **Open .torrent file...**.
3. BitDinghy fetches the file list, then downloads to a **BitDinghy** folder inside your
   **Downloads** folder (change it in **Settings**).
4. Use **Pause**, **Resume**, **Files** (choose which files) and **Remove** on each download.

Windows Firewall may ask about BitDinghy the first time: allowing it on **private** networks lets
more people connect to you, which usually means faster downloads.

The full **[guide](https://bitdinghy.com/guide/)** covers every setting, and the
**[FAQ](https://bitdinghy.com/faq/)** answers the common questions.

### The command line

`dinghy.exe` comes in the same zip. Open a terminal in that folder:

```powershell
.\dinghy.exe get "magnet:?xt=urn:btih:..."   # download, with progress, until done
.\dinghy.exe list                            # the saved download list
.\dinghy.exe --help                          # every command
```

With the window open, `dinghy` adds to and controls the window's list.

## What it does

- Downloads from **magnet links**, **`.torrent` files** and **web links to `.torrent` files**
  (`https://` by default; plain `http://` only if you allow it).
- **Choose which files** to download, at any time.
- **Pause, resume, remove** (keeping the files, or deleting only the files it downloaded); the
  list is kept between runs.
- **Check files** on disk again, and **use files that are already there**.
- A **queue**, **speed limits**, **sharing limits** (ratio or time), a **port** setting and
  optional **UPnP**.
- **When a download is finished:** keep sharing it (the standard), stop it right away, or stop
  it once **Check files** finds it complete.
- **Dark or light** colors, or like Windows.
- **Recommended settings** learned from your own downloads (it doesn't run speed tests).
- **History**: speeds of the last ten minutes, daily totals for 30 days, removed downloads.
- **Problems in plain language** (a full disk, a file another program holds, a FAT32 drive...).
- **Crash reports** that are saved on your computer and not sent anywhere.
- Opens **magnet links and `.torrent` files from your browser and Explorer**, if you choose it in
  Windows' Default apps; it asks before adding anything.
- Not yet: **encryption** of connections (the engine it's built on doesn't support it so far),
  and a **Linux** version (planned).

## How it's built

- **Portable.** Settings and the download list live in a `data` folder next to the program.
- **Built for privacy.** No tracking, no ads, no accounts, no automatic update checks. It's
  designed to send only the BitTorrent traffic itself (and, when you give it one, the request for
  a web link to a `.torrent` file). See the [privacy page](https://bitdinghy.com/privacy/).
- **Careful with your files.** Everything received from the internet is treated as untrusted. It's
  designed to write only inside the download folder you choose, and to refuse a download that
  would overwrite files it didn't download.
- **Two programs, one engine.** `BitDinghy` (the window) and `dinghy` (the command line) are thin
  front-ends over the shared engine `dinghy-core`. With the window open, `dinghy` talks to it over
  a channel set up for this computer and this Windows user only. See
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

These are design goals, carefully tested, but BitDinghy is provided **"as is"**, without warranty
(see the licences below). Found a problem? See [Contributing](#contributing).

## Contributing

- **Bugs and ideas:** open an [issue](https://github.com/Jack113/BitDinghy/issues), or use the
  [feedback form](https://bitdinghy.com/feedback/) (no account needed).
- **Security problems:** please report them privately, see [SECURITY.md](SECURITY.md).
- **Code:** see [CONTRIBUTING.md](CONTRIBUTING.md). What changed in each version is in
  [CHANGELOG.md](CHANGELOG.md).

### Building from source

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

BitDinghy is a general-purpose download tool. It doesn't provide, host or search for any content:
you choose what to download, and you're responsible for having the right to download and share
it. Free, legal examples include Linux distribution images and other openly licensed files.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at
your option. Unless you state otherwise, any contribution you submit is dual-licensed the same
way, with no additional terms.

BitDinghy uses [librqbit](https://github.com/ikatson/rqbit) (Apache-2.0) as its BitTorrent engine
and [egui](https://github.com/emilk/egui) (MIT OR Apache-2.0) for the window. The licences of
everything BitDinghy is built from are in [THIRD-PARTY-NOTICES.html](THIRD-PARTY-NOTICES.html)
(made with `cargo about`, see [docs/RELEASE.md](docs/RELEASE.md)).

"BitTorrent" is a trademark of its owner. BitDinghy is an independent project and is not
affiliated with or endorsed by them.
