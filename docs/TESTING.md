# How to test BitDinghy yourself

One section per feature, newest first. Only use content you are allowed to download. The links
below are official, openly licensed Linux downloads.

## M1: download from a magnet link

### What you need
- A build: run `cargo build --workspace` once (or `cargo run`, which builds as needed).
- A legal test link. This is the official Arch Linux ISO, from the "Magnet link" on
  <https://archlinux.org/download/> (about **1.5 GB**, so mind any data cap; you can stop at any
  time with Ctrl+C):

  ```
  magnet:?xt=urn:btih:9b521219bc48550140a666f74a29048ed78e933b&dn=archlinux-2026.10.01-x86_64.iso
  ```

  Arch publishes a new ISO every month. If this one is gone, copy the current magnet link from
  that page instead.
- Save somewhere on `C:`, because the dev USB stick (`E:`) is FAT32 and cannot hold files over
  4 GB. By default files go to `Downloads\BitDinghy`.

### 1. The command line
Run this in the `E:\Ferry` folder (in the VS Code terminal):
```
cargo run --bin dinghy -- get "<the magnet link in quotes>" --dir C:\BitDinghy-test
```
`dinghy` is not installed anywhere yet, so we start it through `cargo`. (The built program is also
at `C:\cargo-target\debug\dinghy.exe`, so `C:\cargo-target\debug\dinghy.exe get "<link>"` works
from any folder.) The `--` separates cargo's own options from `dinghy`'s.

You should see:
1. `Saving to C:\BitDinghy-test`
2. `Looking for people sharing this torrent... 3s` (usually under 10 seconds)
3. A single line that updates in place: name, percent, size, speed, time left, and people
   connected.
4. `Done: archlinux-... (1.5 GiB) is in C:\BitDinghy-test\...` and the program exits.

Also try:
- **Stop and resume:** press Ctrl+C part-way. It prints "Stopped...". Run the same command again:
  it first says "checking files already on disk" and then carries on from where it stopped.
- **A typo:** `cargo run --bin dinghy -- get hello` should print "That isn't a magnet link..." at
  once and create nothing.
- **Check the file is intact** (after it finishes). *Not done by the owner yet as of the M1
  merge.* Compare with the SHA256 on the Arch page (at the time of writing it was
  `684ded26c63240ff4a41e8c25ee84ea6da233f557364821f13d12c2b0a9059a5`):
  ```
  Get-FileHash C:\BitDinghy-test\archlinux-2026.10.01-x86_64.iso -Algorithm SHA256
  ```
  (Torrents already verify every piece while downloading; this is an independent check.)

### 2. The window
```
cargo run --bin BitDinghy
```
- **Windows Firewall will ask about BitDinghy the first time.** It listens for incoming
  connections so you can share back. Ticking *Private networks* is fine. If you cancel, downloads
  still work; you just can't accept incoming connections.
- Paste the magnet link into the box and press **Download** (or Enter).
- You should see a spinner and "Looking for people...", then a progress bar with speed, time left
  and how many people you are connected to. When it finishes it says **Finished** and where the
  files are. It keeps sharing back until you close the window.
- Try pasting rubbish: a red message should explain what is wrong. Pasting the same link twice
  shows one download.

### What M1 does NOT do yet
Pause and resume buttons, remembering the list after you close the window, `.torrent` files, picking
files inside a torrent, speed limits, and settings all come in later milestones. Closing the
window forgets the list (the files stay on disk).

### Already tested automatically (no internet needed)
`cargo test --workspace` runs 56 tests, including a real download between two copies of the engine
on this computer, checked byte for byte, and hostile torrents (names such as `C:evil`, `NUL`, or
containing control characters) that must be refused with nothing written to disk.
