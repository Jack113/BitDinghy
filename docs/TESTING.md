# How to test BitDinghy yourself

One section per feature, newest first. Only use content you are allowed to download. The links
below are official, openly licensed Linux downloads.

## M5: .torrent files and links

Tests are done one at a time: do the test, compare with "You should see", then go to the next.

### Step 1: .torrent files from your PC

**Step 0: get ready**
1. Close the BitDinghy window and stop any `dinghy` (Ctrl+C).
2. In the repo folder: `cargo build --workspace`.
3. Open a terminal in `C:\cargo-target\debug` (`cd C:\cargo-target\debug`). `BitDinghy.exe` is
   the window; `dinghy.exe` is the command line and needs a command (`.\dinghy.exe list`).
4. Get the official Arch `.torrent` file: open
   <https://archlinux.org/releng/releases/2026.10.01/torrent/> in your browser. It saves
   `archlinux-2026.10.01-x86_64.iso.torrent` (88 KB) in your `Downloads` folder (NOT in
   `Downloads\BitDinghy`).
5. Make sure there is no Arch `.iso` in `Downloads\BitDinghy` (move or delete it; otherwise test 1
   is refused, by design).

**Test 1: the "Open .torrent file..." button**
- Do: open the window (`.\BitDinghy.exe`). Next to **Download** there is a new button,
  **Open .torrent file...**, and under the link box "You can also drag .torrent files onto this
  window." Press the button. Windows' own file dialog opens, showing only `.torrent` files. Choose
  the Arch `.torrent` in `Downloads` and press Open.
- You should see: the Arch download appears at once, with its full name, then "Checking files..."
  or a progress bar (no "Looking for people..." wait for the file list, because the file already
  holds it). Press **Pause** to save data.

**Test 2: the same torrent twice is one download**
- Do: press **Open .torrent file...** again and choose the same file.
- You should see: still one Arch row, no duplicate, no error.

**Test 3: drag and drop**
- Do: remove Arch (**Remove**, then **Delete the files too**). Drag the `.torrent` file from
  Explorer onto the window and hold it there for a moment, then let go.
- You should see: while you hold it over the window, a teal line "Drop the .torrent files here to
  add them."; after letting go, Arch appears. Pause it.

**Test 4: pasting a path**
- Do: remove Arch again (Delete the files too). In Explorer, Shift+right-click the `.torrent`,
  choose **Copy as path**, paste it into the link box (with its quotation marks), press
  **Download**.
- You should see: Arch appears. Pause it.

**Test 5: a file that isn't a torrent**
- Do: in `Downloads`, create a text file and rename it `notes.torrent` (accept Windows' warning
  about the extension). Open it with the button.
- You should see: a red line "...notes.torrent isn't a valid .torrent file." and nothing added.

**Test 6: `dinghy get` with a .torrent file (window closed)**
- Do: close the window. Run
  `.\dinghy.exe get "C:\Users\<you>\Downloads\archlinux-2026.10.01-x86_64.iso.torrent"`.
- You should see: "Saving to ...", then progress straight away (the file list is in the file), as
  it carries on the paused Arch download. Press Ctrl+C after a few seconds.

**Test 7: `dinghy get` with a .torrent file while the window is open**
- Do: open the window. In the terminal, go to your Downloads folder (`cd ~\Downloads`) and run
  `C:\cargo-target\debug\dinghy.exe get archlinux-2026.10.01-x86_64.iso.torrent` (just the file
  name).
- You should see: "BitDinghy's window is open, so the download was added there", and the window
  shows Arch carrying on. The window found the file even though only its name was typed. Press
  Ctrl+C, then Pause in the window.

**Tidy up:** remove Arch (Delete the files too), and delete `notes.torrent`.

### Step 2: web links to .torrent files

**Step 0: get ready** as in step 1 (build, terminal in `C:\cargo-target\debug`, no Arch `.iso` in
`Downloads\BitDinghy`, the window closed). The link used here is the official Arch one:
`https://archlinux.org/releng/releases/2026.10.01/torrent/` (fetching it uses about 90 KB).

**Test 1: paste a web link in the window**
- Do: open the window, paste the Arch link into the link box, press **Download**.
- You should see: for a moment a spinner and "Fetching the .torrent file from archlinux.org...",
  then the Arch download appears (no "Looking for people" wait for the file list). The link box is
  empty again. Press **Pause** to save data.

**Test 2: plain http links are refused by default**
- Do: remove Arch (Delete the files too). Paste the same link but starting with `http://` instead
  of `https://`, press Download.
- You should see: a red line "BitDinghy only opens secure (https://) links to .torrent files,
  because a plain http:// link can be changed on the way. You can allow plain http links in
  Settings." Nothing is added.

**Test 3: the setting**
- Do: open **Settings**. Tick "Also open plain http:// links to .torrent files...".
- You should see: "Plain http:// links are opened too, from now on." Press Download again with the
  `http://` link: now it is fetched (archlinux.org sends BitDinghy on to its https address, which
  is fine) and Arch appears. Pause it. Untick the box: "Only secure https:// links are opened,
  from now on."

**Test 4: links to your own PC or home network are refused**
- Do: paste `https://192.168.1.1/x.torrent` and press Download.
- You should see: "BitDinghy doesn't open links to this computer or the local network
  (192.168.1.1)." Nothing is contacted.

**Test 5: a link that isn't a .torrent**
- Do: paste `https://archlinux.org/download/` (the download PAGE, not the torrent) and press
  Download.
- You should see: after a moment, "That link didn't lead to a .torrent file (archlinux.org sent
  something else)."

**Test 6: `dinghy get` with a web link (window closed)**
- Do: remove Arch in the window (Delete the files too), close the window, run
  `.\dinghy.exe get "https://archlinux.org/releng/releases/2026.10.01/torrent/"`.
- You should see: "Fetching the .torrent file from archlinux.org...", "Saving to ...", then
  progress. Press Ctrl+C after a few seconds.

**Test 7: `dinghy get` with a web link while the window is open**
- Do: open the window and run the same command.
- You should see: "BitDinghy's window is open, so the download was added there", and the terminal
  follows it. Press Ctrl+C, then Pause in the window.

**Test 8: the setting from the command line**
- Do: close the window. Run `.\dinghy.exe settings`.
- You should see: a line `allow-http-links:  off  (only secure https:// links)`. Then
  `.\dinghy.exe settings set allow-http-links on` shows it `on`, and
  `.\dinghy.exe settings set allow-http-links off` puts it back.

**Tidy up:** remove Arch (Delete the files too).

## M4: the command line talks to the open window

While the window is open it owns the only engine, so `dinghy` now passes requests to it over a
local channel that only this computer can use. With the window closed, `dinghy` works on its own
as before. The programs are in `C:\cargo-target\debug\` (build first with
`cargo build --workspace`).

### Step 1: `dinghy list` and `dinghy get` with the window open

1. **The live list.** Open the window. In a terminal run `dinghy.exe list`. It says "BitDinghy's
   window is open and has no downloads" (or, if the window has downloads, it lists them numbered
   with their state right now: "looking for people sharing it", "12.3% of 1.5 GiB, 8.0 MiB/s, 3m
   left", "paused at 40.0%", "finished, sharing").
2. **Hand a link to the window.** Run `dinghy.exe get "<the Arch magnet link>"`. It says "BitDinghy's
   window is open, so the download was added there", and the download **appears in the window at
   once**. The terminal follows it with the usual progress line.
3. **Ctrl+C only stops following.** Press Ctrl+C. It says "Stopped following. The download carries
   on in BitDinghy's window." The window keeps downloading. Run `dinghy.exe list`: it shows the
   live percentage. Pause the download in the window to save data.
4. **`--dir` is refused while the window is open.** `dinghy.exe get "<link>" --dir C:\Somewhere`
   says to leave out `--dir` or close the window, and nothing is added.
5. **The window closing is explained.** Resume Arch in the window, run `dinghy.exe get "<the Arch
   magnet link>"` again (it follows the same download), then close the window. The terminal says
   the window was closed and the download carries on the next time it opens.
6. **With the window closed, `dinghy` works alone as before.** `dinghy.exe list` shows the saved
   list again (`[active]`, `[paused]`...).

### Step 2: pause, resume and remove with the window open

Keep the window open for all of these, with at least one download in it (the Arch download is
fine; pause it in the window first to save data). Run `dinghy.exe list` to see the numbers.

1. **Pause and resume.** `dinghy.exe pause 1` says "Paused: ... (in BitDinghy's open window)", and
   within a second the window shows it paused (the button says **Resume**). `dinghy.exe resume 1`
   says "Resumed: ..." and the window carries on. Pause it again.
2. **Wrong numbers.** `dinghy.exe pause 99` says there is no download number 99 in the window.
3. **Remove, keep the files.** Add the "Nobody Has It" link from the M3 walkthrough (part 4) in the
   window. `dinghy.exe list` shows its number. `dinghy.exe remove <its number>` says "Removed:
   Nobody Has It." and it disappears from the window.
4. **Remove and delete the files, asked first.** Put `notes.txt` in `Downloads\BitDinghy`. Run
   `dinghy.exe remove 1 --delete-files` (1 = the Arch download). It lists the `.iso` and asks you to
   type `yes`. Type `no`: "Nothing was removed or deleted", and the window still shows it. Run it
   again and type `yes`: it says "Deleted 1 file it downloaded", Arch disappears from the window,
   the `.iso` is gone, and `notes.txt` is still there.
5. **`dinghy get` in another terminal.** Close the window. In one terminal start
   `dinghy.exe get "<the Arch magnet link>"` and leave it running. In a second terminal run
   `dinghy.exe pause 1`: it says BitDinghy is busy in another terminal and to stop it with Ctrl+C
   first. Press Ctrl+C in the first terminal; now `dinghy.exe pause 1` works (window closed, so
   it changes the saved list).

## M3: the saved list, pause, resume and remove (built in three steps)

### The window, everything in M3, step by step

This walkthrough covers everything M3 added to the window. It uses the official Arch Linux
download (the magnet link is in the M1 section below; about 1.5 GB in total). Pause it whenever a
step doesn't need it running, and the whole walkthrough uses a few hundred MB. Expect "Looking for
people sharing this torrent..." to take anywhere from a few seconds to a minute the first time.

**Before you start**
- Build once: `cargo build --workspace` (in `D:\BitDinghy` or `E:\BitDinghy`). The window is
  `C:\cargo-target\debug\BitDinghy.exe`; its list is `C:\cargo-target\debug\data\downloads.db`.
- Look in `Downloads\BitDinghy`. Files left there by earlier tests are NOT in the new list: adding
  one of those links again is refused on purpose (see part 6). If an Arch `.iso` is already there,
  move it somewhere else first.
- Keep File Explorer open on `Downloads\BitDinghy`, so you can watch files appear and disappear.

**1. Downloads are remembered**
1. Open the window. Below the link box it says "Nothing downloading yet".
2. Paste the Arch link, press **Download**. The row shows the name, a **Pause** and a **Remove**
   button on the right, then "Looking for people sharing this torrent..." with a spinner, then a
   green progress bar with "X of Y | speed | time left | people connected".
3. Let it reach a few percent, then close the window with the X. It closes at once.
4. Open it again. The Arch download is **back by itself**: first "Checking files already on
   disk..." (BitDinghy checks what is already saved), then it carries on from about the same
   percentage, not from zero.

**2. Only one BitDinghy at a time**
1. With the window open, start `BitDinghy.exe` a second time.
2. The second window opens with **Settings** unfolded and a red line: "BitDinghy is already
   running (in its window or in another terminal). Use that one, or close it first." It shows no
   downloads and changes nothing. Close the second window; the first one keeps going.

**3. Pause and resume**
1. While Arch downloads, press **Pause**. Within a second or two the line under the bar says
   "Paused at X of Y. Nothing is downloaded or shared until you resume it." and the button says
   **Resume**. The speed, time left and people connected are gone from the row. (The `.iso` in
   Explorer shows its full size from the start, because space for the whole file is set aside
   at once, so its size doesn't tell you anything.)
2. Close the window and open it again. After "Checking files already on disk..." it comes back
   **still paused**.
3. Press **Resume**: it says "Looking"/progress again and carries on.
4. Pause it again for the next parts.

**4. Pause before the file list arrives** (no download data used)
1. Paste this link, which nobody shares, and press Download:
   `magnet:?xt=urn:btih:8bb9ffd92dc9584ae9923bf5422c285a8e31cdcc&dn=Nobody%20Has%20It`
   It says "Looking for people sharing this torrent..." and keeps looking.
2. Press **Pause**. It shows a 0% bar and "Paused. BitDinghy will look for its file list when you
   resume it."
3. Close and reopen the window: it is still paused, straight away (nobody is contacted for it).
4. Keep it for part 5.

**5. Remove: Cancel, and Keep the files**
1. On "Nobody Has It", press **Remove**. A box appears under it: "Remove "Nobody Has It" from the
   list?" in amber, a short note, and three buttons: **Keep the files**, **Delete the files too**,
   **Cancel**.
2. Press **Cancel**: the box closes and nothing changes.
3. Press **Remove** again, then **Keep the files**. The row disappears, and a message at the top of
   the list says it was removed and where its files are (it never had any).
4. On the paused Arch download, press **Remove**, then **Keep the files**. The row disappears; the
   message names the folder; in Explorer the `.iso` **is still there**.
5. Close and reopen the window: both stay gone.

**6. Your files are never overwritten**
1. The Arch `.iso` from part 5 is still in `Downloads\BitDinghy`, but BitDinghy has forgotten it,
   so it is no longer BitDinghy's file. Paste the Arch link again and press Download.
2. After it finds the file list, the row turns red: "BitDinghy didn't start this download because a
   file with the same name is already in the download folder: ...archlinux....iso. BitDinghy never
   overwrites files it didn't download. Move or rename that file, then try again." The button says
   **Try again**. In Explorer, the `.iso`'s size and date haven't changed.
3. Move the `.iso` out of `Downloads\BitDinghy` (for example to your Desktop), then press **Try
   again**. It starts, from zero this time. Let it get a few MB, then **Pause**.

**7. Remove and delete the files**
1. Put a small file of your own in `Downloads\BitDinghy`, for example `notes.txt`.
2. On the Arch download press **Remove**, then **Delete the files too**.
3. The row disappears and the message says it "deleted 1 file it downloaded". In Explorer the
   new `.iso` is gone; **`notes.txt` and the `BitDinghy` folder are still there**, and so is the
   `.iso` you moved to the Desktop (it was never touched).

**8. A remembered download keeps its folder**
1. Start the Arch download again (it will be refused if an Arch `.iso` is in `Downloads\BitDinghy`;
   move it away first). Let it get a few MB, then **Pause**.
2. Open **Settings**, type another folder (for example `C:\BitDinghy-test3`), press **Save**.
   Close and reopen the window.
3. "Saving to:" now shows the new folder (for NEW downloads), but the Arch download is still in
   `Downloads\BitDinghy` and carries on there when resumed. Nothing appears in the new folder.
4. Press **Use the standard folder** in Settings to go back.

**9. A damaged list is kept, never thrown away**
1. Close the window. Open `C:\cargo-target\debug\data\downloads.db` in Notepad, replace everything
   with the word `broken`, and save.
2. Open the window: an amber note says BitDinghy couldn't read the saved download list, started a
   new, empty one, and **kept the old file** as `downloads.db.broken-<number>` (look in the data
   folder). The window works normally.

**10. Tidy up**
After part 9 the list is empty, so BitDinghy no longer knows about the paused Arch `.iso` from
part 8. Delete it from `Downloads\BitDinghy` yourself, along with `C:\BitDinghy-test3`, the `.iso`
on your Desktop, and the `downloads.db.broken-<number>` file in the data folder.

If anything looks different from this, a screenshot is the most useful thing to send back.

### Step 1: downloads are remembered (and only one BitDinghy runs at a time)

This uses the internet and some data: it starts the Arch Linux download from the M1 section
below (about 1.5 GB). You only need it to run for a few seconds each time, so stop early.
Development builds keep their list in `C:\cargo-target\debug\data\downloads.db`.

First build once: `cargo build --workspace`. The programs are in `C:\cargo-target\debug\`.

1. **Nothing saved yet.** `C:\cargo-target\debug\dinghy.exe list` says "No saved downloads yet".
2. **The window remembers.** Open `BitDinghy.exe`, paste the Arch magnet link and press Download.
   When it shows progress, close the window. Open it again: the download is **back by itself**. It
   first shows "Checking files already on disk..." and then carries on from where it was.
3. **Only one BitDinghy at a time.** With the window open, run
   `dinghy.exe get "<the Arch magnet link>"`. It refuses with "BitDinghy is already running", and
   nothing else happens.
4. **`dinghy list` works any time.** With the window still open, run `dinghy.exe list`: it shows
   the Arch download, numbered, with its folder.
5. **The command line carries on the same download.** Close the window, then run
   `dinghy.exe get "<the Arch magnet link>"`. It says "Saving to" the same folder and continues
   from where the window stopped (it checks the files first). Press Ctrl+C after a few seconds.
6. **A damaged list is never thrown away.** With everything closed, open
   `C:\cargo-target\debug\data\downloads.db` in Notepad, replace everything with `broken`, and save.
   Open the window: a note says the list couldn't be read and **where the old file was kept**
   (`downloads.db.broken-<number>`), and the window starts with an empty list.

### Step 2: pause and resume

This also uses the Arch download from step 1 (a few seconds at a time).

1. **Pause in the window.** Open the window. Each download has a button on the right of its name.
   Press **Pause** while it downloads: the speed drops to nothing, the line says "Paused at ... of
   ...", and the button becomes **Resume**.
2. **Paused stays paused.** Close the window and open it again. The download comes back
   **paused** (after "Checking files already on disk..."). Press **Resume**: it carries on.
3. **Pause before the file list arrives.** Paste the link of a new download and press Pause at
   once, while it still says "Looking for people...". It shows as paused; Resume starts it.
4. **The command line, with the window closed.** Close the window. `dinghy.exe list` shows the
   numbers. `dinghy.exe pause 1` says "Paused: ..." and `list` now shows `[paused]`. Open the
   window: it is paused there too. Close it again and run `dinghy.exe resume 1`.
5. **Not while the window is open.** With the window open, `dinghy.exe pause 1` refuses and says
   to pause it in the window (until M4 lets the two talk).
6. **Wrong numbers are explained.** `dinghy.exe pause 99` says there is no download number 99 and
   points to `dinghy list`.
7. **`dinghy get` means "carry on".** Pause the Arch download, close the window, then run
   `dinghy.exe get "<the Arch magnet link>"`: it resumes it and continues. Press Ctrl+C.

### Step 3: remove, and never overwriting your files

This also uses the Arch download (a few seconds at a time).

1. **Remove and keep the files.** In the window, press **Remove** on the Arch download. A question
   appears under it. Press **Keep the files**: it disappears from the list, a message says where
   its files still are, and the file is still in `Downloads\BitDinghy`. Close and reopen the
   window: it stays gone.
2. **A new download never takes over your file.** Paste the Arch link again and press Download.
   It is **refused**: the message names the `.iso` that is already there and says BitDinghy never
   overwrites files it didn't download (after "Keep the files", that file is no longer
   BitDinghy's). Nothing is changed. Move the `.iso` somewhere else and press **Try again**: it
   starts, from the beginning this time (so pause it after a few seconds to save data).
3. **Remove and delete the files.** Put a small file of your own (for example `notes.txt`) in
   `Downloads\BitDinghy`. Press **Remove** on the Arch download, then **Delete the files too**.
   The message says it deleted 1 file. The `.iso` is gone; **`notes.txt` and the folder are still
   there**.
4. **Cancel does nothing.** Start a download, press **Remove**, then **Cancel**: it carries on.
5. **The command line, with the window closed.** Start the Arch download again, close the window,
   then:
   - `dinghy.exe remove 1` removes it from the list and says where the files still are.
   - Start it again (window or `dinghy get`) after moving the file away, close the window, then
     `dinghy.exe remove 1 --delete-files`: it lists the file it will delete and asks you to type
     `yes`. Type `no` and nothing happens; run it again and type `yes`, and it is deleted.
6. **Not while the window is open.** With the window open, `dinghy.exe remove 1` refuses and says
   to remove it in the window.

## M2: portable data folder, remembered settings, and the branded window

Everything here is **safe to try**: it uses no internet (except the optional last step) and only
changes BitDinghy's own settings. Run the commands in the project folder (`E:\BitDinghy`) in the
VS Code terminal. A development build keeps its files in `C:\cargo-target\debug\data`, because
that is where the programs are built.

First build once: `cargo build --workspace`.

### A. The command line

1. **See where things are.**
   ```
   cargo run --bin dinghy -- settings
   ```
   You should see `Data folder: C:\cargo-target\debug\data (next to the program, so it travels with
   it)`, the settings file path, and `download-dir: ... (standard folder)`.

2. **Change the download folder.**
   ```
   cargo run --bin dinghy -- settings set download-dir C:\BitDinghy-test2
   ```
   It says `Saved.` and creates the folder. Open `C:\cargo-target\debug\data\settings.toml` in a
   text editor: it is a short readable file with `version = 1` and `download_dir = ...`.

3. **Check it is remembered.** Run step 1 again (a brand-new run of the program). It now shows your
   folder, without `(standard folder)`.

4. **Go back to the standard folder.**
   ```
   cargo run --bin dinghy -- settings set download-dir default
   ```

5. **A folder that can't work is refused.**
   ```
   cargo run --bin dinghy -- settings set download-dir C:\Windows\notepad.exe
   ```
   It says `...that is a file, not a folder` and changes nothing.

6. **A damaged file is never thrown away.** Open `settings.toml`, replace everything with the text
   `this is [broken`, save, and run step 1. You should see a `Note:` saying BitDinghy couldn't read
   your settings. Look in the data folder: your damaged file was **kept** as
   `settings.toml.broken-<number>`, and the program carried on with standard settings.

7. **A file from a newer BitDinghy is protected.** Put `version = 99` in `settings.toml` and run
   step 1: a note says the settings were saved by a newer BitDinghy. Then try step 2: it **refuses**
   to save, and the file is untouched. (Delete `settings.toml` afterwards.)

8. **It really is portable.** Copy `dinghy.exe` and the `data` folder from
   `C:\cargo-target\debug` to a new folder (for example `C:\Temp\copy`), then run
   `C:\Temp\copy\dinghy.exe settings`. The data folder shown is the one next to *that* copy, with the
   settings you had. Copying the folder to a USB stick works the same way.

9. **`get` uses the remembered folder.** Set a folder (step 2), then start a download from
   `docs\TESTING.md`'s M1 section and press Ctrl+C after a few seconds. The first line says
   `Saving to <your folder>`. (`--dir` still overrides it for one run.)

### B. The window

```
cargo run --bin BitDinghy
```

1. **The brand look.** Deep navy background, the BitDinghy logo at the top with the version beside
   it, and the sailboat icon in the title bar and on the taskbar.
2. **Dark only.** The window is always dark, for now. Change Windows' own mode (*Settings >
   Personalization > Colors > Choose your mode*) to light: the window must **stay** navy with white
   lettering and a lime-green progress bar, and keep the white-lettered logo.
3. **Settings.** Click **Settings**. It shows where BitDinghy keeps its files. Type a folder and
   press **Save**: a message says it was saved and will be used from the next start. Close the
   window, open it again: the new folder shows next to "Saving to:". **Use the standard folder**
   puts it back. Typing a path that can't work (for example a file) shows a red message.
4. **Closing.** Close the window with the X: it closes at once and quietly. (Earlier versions
   crashed on close; that is fixed and now has a test.)
5. **The program's icon in Explorer.** Open `C:\cargo-target\debug`: `BitDinghy.exe` and
   `dinghy.exe` have the sailboat icon. Right-click `BitDinghy.exe` > Properties > **Details**: the
   product name is BitDinghy, the version is 0.0.1, and there is a description.
6. **Optional: the colors with real downloads** (uses data). Paste the magnet link from the M1
   section below, watch the teal "Looking for people..." message, the lime-green progress bar and
   the green "Finished" line, and close the window after a few seconds.

### Already tested automatically
`cargo test --workspace` runs 95 tests. For this feature they include: the fallback to the user
profile when the folder next to the program can't be written; a version-1 settings file that must
load forever; damaged and newer files being left alone; the remembered folder surviving a restart
in a separate program run; and the window shutting down without crashing.

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
Run this in the `E:\BitDinghy` folder (in the VS Code terminal):
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
