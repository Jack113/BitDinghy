# Contributing to BitDinghy

Thanks for helping. This project is deliberately built **one small feature at a time**, so the
most useful contributions are focused and easy to review.

## Ground rules

- **One feature per branch, one branch per pull request.** Name it after the feature, for
  example `m3-pause-resume`.
- **A feature is finished when** it works in both the window and the command line, has tests, is
  documented, and passes all automatic checks.
- **Privacy is a rule, not a preference.** No telemetry, no analytics, no network calls except
  BitTorrent traffic, the user's own router (UPnP), and anything the user explicitly asks for.
- **Treat everything from the network as untrusted**, including torrent files, file names, peer
  messages and web responses. Never write outside the download folder.
- **Anything that touches the network gets a security review** before it is merged.

## Setup

1. Install Rust 1.95 or newer from <https://rustup.rs>.
2. Windows: install the Visual Studio "Desktop development with C++" workload.
   Linux: install a C compiler and `pkg-config libxkbcommon-dev libwayland-dev libx11-dev libgl1-mesa-dev`.
3. Optional but recommended: `cargo install cargo-deny --locked`.

## Before you open a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

CI runs the same commands on Windows and Linux.

## Where does my code go?

- Logic that either program needs goes in `crates/dinghy-core`. It is the **only** crate allowed
  to call the BitTorrent engine directly.
- `crates/dinghy-cli` and `crates/bitdinghy-app` stay thin: read input, call the core, show the
  result.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full picture.

## Commits

Use short, clear messages in the style `type: summary`, for example `feat: pause and resume`,
`fix: reject torrents with absolute paths`, `docs: explain the queue`, `ci: cache builds`.

## Saved data must survive upgrades

Settings and the download list are versioned. If you change a saved format, add a migration and a
test that loads a file saved by the previous version.

## License of contributions

By contributing you agree that your work is licensed under MIT OR Apache-2.0, the same as the
project.
