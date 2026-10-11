# Making a release

The steps for a BitDinghy release, in order. Nothing here happens by itself: each step that
makes something public (marked **owner's decision**) is done by hand, after a yes.

## 1. Before: tested and merged

1. The big guided test in [TESTING.md](TESTING.md) passed (every milestone since the last
   release), and the security review found nothing open.
2. The feature branches are merged into `main` (one pull request each, in order, CI green).
   **Owner's decision.**

## 2. The files that go with the programs

1. **Third-party notices.** In the repo folder:
   `cargo about generate about.hbs --output-file THIRD-PARTY-NOTICES.html`
   (needs `cargo install cargo-about --locked` once). Check that it lists the fonts' OFL and
   Ubuntu-font licences (egui's built-in fonts) and commit it. The release workflow fails if the
   file is missing, on purpose.
2. **Changelog.** Rename `## [Unreleased]` to `## [0.1.0] - <date>` in `CHANGELOG.md` and start a
   new, empty `## [Unreleased]` above it.
3. **Numbers.** Build with `cargo build --release --bins` and write the sizes, the start time and
   the memory use in the release notes (see `CLAUDE.md` for how they were measured).

## 3. The draft release

1. **Tag** the merged `main`: `git tag v0.1.0`, then `git push origin v0.1.0`. **Owner's
   decision.**
2. The **Release** workflow (`.github/workflows/release.yml`) builds on Windows, checks that both
   programs carry their icon and Details information, and makes a **draft** release with:
   - `BitDinghy-0.1.0-windows-x64.zip` (both programs, README, CHANGELOG, licences, notices),
   - `SHA256SUMS.txt` (the zip's checksum),
   - `BitDinghy-0.1.0-windows-x64-debug-symbols.zip` (the `.pdb` files: keep them, they turn
     crash-report backtraces into function names; there's no need to publish them).
3. Download the zip from the draft, unzip it on a clean Windows PC (or a virtual machine without
   a graphics card, to check the window still opens), run both programs once.

## 4. Signing (once the project qualifies)

Windows SmartScreen warns about unsigned programs. **SignPath Foundation** signs open-source
programs for free; it needs a public repository, an OSI licence (we have MIT/Apache-2.0), a
code-signing policy page with a privacy statement on the website, and a person approving each
release. Until then the release is unsigned, and the website says how to check the checksum.
**Owner's decision** (applying, and making the repository public).

## 5. Publishing

1. Make the repository public, if not yet. **Owner's decision.**
2. Publish the draft release. **Owner's decision.**
3. Put the download link, the checksum and the numbers on the website.

## Never

- Never publish a release that wasn't built by the workflow from a tag.
- Never upload the `.pdb` files to a public place they could be confused with the programs.
