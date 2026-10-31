# Packages for winget and Scoop

Ready to submit once a release is **published** on GitHub (they point at its zip). Nothing here
is used by the build.

## After publishing a release

1. Fill in the zip's checksum (read from the release's `SHA256SUMS.txt`):
   `.\packaging\fill-hashes.ps1 0.1.0`
2. winget: check, then try it locally.
   `winget validate .\packaging\winget` and `winget install --manifest .\packaging\winget`
   Then submit: a pull request to `microsoft/winget-pkgs` with the three files in
   `manifests/b/BitDinghy/BitDinghy/0.1.0/` (or `wingetcreate submit .\packaging\winget`).
3. Scoop: `scoop install .\packaging\scoop\bitdinghy.json` to try it. Submit it to the
   `ScoopInstaller/Extras` bucket (a pull request adding `bucket/bitdinghy.json`), or keep our own
   bucket repo.

For a new version: change the version number and the `0.1.0` in the paths and addresses, then
run the script again. (Scoop's `autoupdate` section lets the Extras bucket do this by itself.)

## How BitDinghy's data folder survives updates

- **Scoop** keeps the `data` folder between versions (`"persist": "data"`), so the saved list and
  settings stay.
- **winget** puts portable programs in its own folder. **Check before submitting** whether
  `winget upgrade` keeps the `data` folder next to the exe; if not, the README must tell winget
  users, or BitDinghy should use the user's settings folder when it runs from winget's folder.
