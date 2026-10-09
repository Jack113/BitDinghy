# Changelog

All notable changes are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

Saved settings and download lists from older versions must keep working. Any change to a saved
format is called out here.

## [Unreleased]

### Added
- Project skeleton: a Cargo workspace with the shared engine (`dinghy-core`), the `dinghy`
  command line and the `BitDinghy` window. None of them download anything yet.
- Automatic checks on Windows and Linux: formatting, lints, tests, plus a license and
  vulnerability check of all dependencies.
- Dual license: MIT OR Apache-2.0.
