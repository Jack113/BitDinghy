# How to test BitDinghy yourself

One section per feature, newest first. Only use content you are allowed to download. The links
below are official, openly licensed Linux downloads.

## The redesigned window (after v0.1.0)

Data-free: the test torrent is one nobody shares, and its made-up files are already on your PC.
It also makes a **release copy** in a folder that stays put and makes that copy the program
Windows opens magnet links and `.torrent` files with (your choice, 2026-11-04). That copy has its
own `data` folder, so its download list starts empty; the development copy keeps yours.

**Step 0: get ready**
1. In the repo folder: `cargo build --release --bins` (it takes several minutes).
2. Make the folder `C:\Apps\BitDinghy` (or another folder you will keep) and copy
   `BitDinghy.exe` and `dinghy.exe` from `C:\cargo-target\release` into it.
3. `cargo run -p dinghy-core --example make_test_torrent` (makes the test torrent and its files
   in `%TEMP%\bitdinghy-m6-test`).
4. Point the new copy's download folder at those files:
   `C:\Apps\BitDinghy\dinghy.exe settings set download-dir "$env:TEMP\bitdinghy-m6-test"`

**Test 1: the new look, and the one-time question**
- Do: start `C:\Apps\BitDinghy\BitDinghy.exe`.
- You should see: a sidebar on the left with the logo (the boat stands still), **Downloads**
  (chosen, in green), **History** and **Settings**, and at the bottom `↓ 0 B/s`, `↑ 0 B/s`,
  "No speed limits" and "Version 0.1.0". Above the page, a card with a teal stripe: "Open magnet
  links and .torrent files with BitDinghy?" with **Yes** and **Not now**. (It asks because
  Windows still opens them with the development copy.) Don't answer yet.

**Test 2: what Windows opens links with**
- Do: click **Settings** in the sidebar and scroll to **Magnet links and .torrent files**.
- You should see: two orange lines, "Magnet links open in another copy of BitDinghy:
  C:\cargo-target\debug\BitDinghy.exe" and the same for ".torrent files".

**Test 3: Yes**
- Do: press **Yes** on the question card.
- You should see: the card is replaced by "Registered. Windows' Default apps is open: choose
  BitDinghy for .torrent and for MAGNET there." with **OK**, and Windows' Default apps opens.
  The Magnet links line now turns green ("Magnet links open in this BitDinghy."), because
  Windows already had BitDinghy chosen for magnet links.
- Then in Default apps: "Choose default apps by file type" > `.torrent` > pick **BitDinghy**,
  and check that "Choose default apps by protocol" > **MAGNET** shows BitDinghy. If two
  BitDinghy entries are offered for `.torrent`, pick one and look at BitDinghy's Settings line: if
  it still says "another copy", pick the other one.
- You should see: back in BitDinghy's Settings, both lines green: "... open in this BitDinghy."
  Close BitDinghy and start it again: no question this time.

**Test 4: a download card, and the boat sails**
- Do: on **Downloads**, drag `%TEMP%\bitdinghy-m6-test\queue-1.torrent` onto the window.
- You should see: a card "Queue test 1" with a teal **Downloading** badge, an empty bar at 0.0%,
  "... 0 people connected ..." (nobody shares it, so nothing is downloaded), and on the right
  **Files**, **Check files**, **Pause**, **Remove**. The boat in the logo now rocks and bobs and
  the digits blow towards the sail; the sidebar's Downloads shows 1.
- Do: press **Pause**.
- You should see: a gray **Paused** badge, the button says **Resume**, and the boat stops where
  it was (no jump).
- Do: **Remove** > **Keep the files**.

**Test 5: finishing from files already there**
- Do: drag `%TEMP%\bitdinghy-m6-test\bitdinghy-m6-test.torrent` onto the window.
- You should see: a card "BitDinghy M6 test" with a red **Problem** badge, the message that a
  file with the same name is already there, and **Use the files already there** next to
  **Remove**.
- Do: press **Use the files already there**.
- You should see: a teal **Checking** badge for a moment, then a full green bar, 100.0%, a green
  "Finished: ..." line and a green **Finished, sharing** badge.

**Test 6: History and the groups in Settings**
- Do: click **History**, then **Settings**.
- You should see: History has four tiles (Today, downloaded / uploaded; 30 days, downloaded /
  uploaded), the speed chart and the daily chart, and "Removed downloads". Settings shows cards:
  Downloads, Speed, Sharing, Connections, Magnet links and .torrent files, Look, About.

**Test 7: light colors**
- Do: Settings > **Look** > **Light**, then **Like Windows**, then **Dark**.
- You should see: the whole window turns light at once (white cards, dark text, the progress bar
  still visible), then follows Windows' own setting, then is dark again. Each time, a teal line
  at the bottom: "Saved: the new colors are used from now on."

**Test 8: stop once "Check files" finds it complete**
- Do: Settings > **Sharing** > choose "Stop it once "Check files" finds it complete and correct".
  Go to **Downloads** and press **Check files** on the test download.
- You should see: after a moment the badge says **Paused**, and a teal note "Checked: every
  chosen file is complete and correct. Paused, as chosen in Settings (resume it to share it
  again)." The button now says **Resume**.

**Test 9: stop right away**
- Do: Settings > Sharing > **Stop it right away**. On Downloads, press **Resume**.
- You should see: within a second or two the badge says **Finished** and the line under it
  starts with "Stopped sharing, as chosen in Settings"; the button says **Share again**.
- Do: Settings > Sharing > **Keep sharing it with others while BitDinghy runs (the standard)**.
- You should see: on Downloads, within a second or two the badge says **Finished, sharing** again.

**Test 10: the same setting from the command line**
- Do: `C:\Apps\BitDinghy\dinghy.exe settings`
- You should see: a line `when-finished:  keep-sharing  (finished downloads share with others)`.
- Do: `C:\Apps\BitDinghy\dinghy.exe settings set when-finished sometimes`
- You should see: `when-finished is "keep-sharing", "stop" or "stop-after-check".`

**Test 11: the folder buttons and the Support link**
- Do: on Downloads press **Open folder**; in Settings > About press **Open data folder**, then
  click **Support BitDinghy**.
- You should see: Explorer opens the test folder, then `C:\Apps\BitDinghy\data`; your browser
  opens bitdinghy.com/support/.

**Test 12 (optional): less motion**
- Do: drag `queue-1.torrent` onto the window again (the boat sails). Then Windows Settings > Ease
  of Access (Windows 11: Accessibility) > Display > turn off **Show animations in Windows**.
- You should see: the boat stops. Turn the Windows setting back on: it sails again. Remove
  "Queue test 1" with **Keep the files**.

**Last step: tidy up**
- Do: on Downloads, **Remove** the test download with **Keep the files**. In Settings >
  Downloads, press **Use the standard folder** (from the next start, downloads go to your normal
  Downloads\BitDinghy folder). `C:\Apps\BitDinghy` stays: it is now the copy Windows opens links
  with. The development copy (`C:\cargo-target\debug`) asks the same question once, the next time
  it starts, because links now open in the other copy: press **Not now** there.

### Results

| Test | Result |
|---|---|
The owner's test, 2026-11-04 (on the release build of branch `ui-redesign`):

| Test | Result |
|---|---|
| 1. The new look, and the one-time question | Passed |
| 2. What Windows opens links with | Passed (checked after Yes: both lines green) |
| 3. Yes | Passed |
| 4. A download card, and the boat sails | Passed |
| 5. Finishing from files already there | Passed (redone after the files were deleted by mistake with "Delete the files too", which is correct behaviour) |
| 6. History and the groups in Settings | Passed |
| 7. Light colors | Passed |
| 8. Stop once "Check files" finds it complete | Passed |
| 9. Stop right away | Passed on a fresh download; the first try stayed sharing because that download had been "shared again" (by design, see `TODO.md`) |
| 10. The same setting from the command line | Passed |
| 11. The folder buttons and the Support link | Passed after a fix: the link did nothing (eframe's link opening is off in this build), now Windows opens it |
| 12. Less motion (optional) | Skipped |

## M18: the release build

Data-free. This checks what a person downloading BitDinghy would get.

**Step 0: get ready**
1. In the repo folder: `cargo build --release --bins` (it takes several minutes).

**Test 1: the version**
- Do: `C:\cargo-target\release\dinghy.exe --version`, and right-click
  `C:\cargo-target\release\BitDinghy.exe` > Properties > **Details**.
- You should see: `dinghy 0.1.0`; Details shows product name BitDinghy, version 0.1.0 and a
  description.

**Test 2: the window, as released**
- Do: copy `BitDinghy.exe` and `dinghy.exe` from `C:\cargo-target\release` to a new empty folder
  (for example on your Desktop) and start `BitDinghy.exe` there.
- You should see: the window opens quickly, with no black console window behind it; a `data`
  folder appears next to the programs (portable). Close it. Delete that test folder.
- Windows Firewall may ask about BitDinghy first ("Publisher: Unknown", because the program isn't
  code-signed yet). That's Windows' own question for a program that accepts incoming connections
  from a new folder. For this test press **Cancel**; in real use, tick **Private networks** only.

**Test 3: the notices**
- Do: open `THIRD-PARTY-NOTICES.html` (in the repo folder) in your browser.
- You should see: a long list of the libraries BitDinghy is built from with their licences,
  including the SIL Open Font License and the Ubuntu Font Licence (the window's fonts).

**Test 4 (optional): a screen reader**
- Do: start Windows Narrator (Ctrl+Windows+Enter), open the window, press Tab a few times.
- You should see/hear: Narrator reads the buttons ("Download", "Open .torrent file...") as they
  get the focus. Ctrl+Windows+Enter stops Narrator.

### Results

| Test | Result |
|---|---|
| 1. The version | Passed (2026-11-01) |
| 2. The window, as released | Passed (Windows Firewall asked first; Cancel) |
| 3. The notices | Passed |
| 4. A screen reader (optional) | Skipped by the owner (Narrator started) |

## M17: encryption (a note, until the engine supports it)

Data-free.

**Test 1: the note**
- Do: `.\dinghy.exe settings`, then open the window's **Settings**.
- You should see: in `dinghy settings`, "encryption:  not available yet" and the note "Connections
  to other people aren't encrypted yet: the engine BitDinghy is built on doesn't support it. Your
  internet provider can see BitTorrent traffic, including which torrents. Encryption comes when
  the engine supports it."; in the window the same note, small, under the Connections settings.

### Results

| Test | Result |
|---|---|
| 1. The note | Passed |

## M16: opening magnet links and .torrent files from other programs

Data-free. This changes your Windows settings for `.torrent` and magnet links (only for your
Windows user); test 6 undoes it. It uses the queue test torrents from M8.

**Step 0: get ready**
1. Close the window, stop any `dinghy`, `cargo build --workspace`, PowerShell in
   `C:\cargo-target\debug`. Make the test torrents if needed.
2. `.\dinghy.exe default-app` says "BitDinghy isn't set up to open magnet links and .torrent
   files."

**Test 1: the setting**
- Do: open the window and **Settings**.
- You should see: "Magnet links and .torrent files:", "BitDinghy isn't set up to open them yet.",
  and a button **Make BitDinghy the default...** (hovering it explains that you choose it in
  Windows' Default apps).

**Test 2: registering, and choosing it in Windows**
- Do: press **Make BitDinghy the default...**.
- You should see: "Registered. Windows' Default apps is open: choose BitDinghy for .torrent and for
  MAGNET there." and Windows Settings opens on Default apps. In Windows 10: **Choose default apps
  by file type**, find `.torrent`, choose **BitDinghy**; then **Choose default apps by protocol**,
  find **MAGNET**, choose **BitDinghy**. (Windows 11: BitDinghy's page lists both.) Back in
  BitDinghy, Settings now says "BitDinghy is registered to open them. To make it the default,
  choose it in Windows' Default apps." with **Open Default apps** and **Undo**.

**Test 3: a .torrent file opened from Explorer, window closed**
- Do: close the window. In Explorer, go to `%TEMP%\bitdinghy-m6-test` and double-click
  `queue-1.torrent`.
- You should see: BitDinghy opens with "Another program asked BitDinghy to open this. Download
  it?", "The .torrent file C:\Users\...\queue-1.torrent", and **Download** / **Not now**.
  Press **Not now**: the question goes and nothing is added.

**Test 4: a .torrent file opened while the window is open**
- Do: with the window open (put another window in front of it), double-click `queue-2.torrent`.
- You should see: BitDinghy comes to the front with the same question for queue-2.torrent. Press
  **Download**: Queue test 2 is added (and waits at 0%: nobody shares it).

**Test 5: a magnet link from the browser**
- Do: in your browser's address bar, paste
  `magnet:?xt=urn:btih:5a8b2f0c9e1d4b7a6c3e8f0a1b2c3d4e5f6a7b8c&dn=Browser%20test` and press
  Enter. The browser asks whether to open BitDinghy: allow it.
- You should see: BitDinghy asks: "A magnet link: "Browser test"". Press **Not now**.

**Test 6: undo**
- Do: `.\dinghy.exe default-app` (it names `C:\cargo-target\debug\BitDinghy.exe`), then in the
  window's Settings press **Undo**, then `.\dinghy.exe default-app` again.
- You should see: "BitDinghy is no longer registered. Windows asks which program to use the next
  time you open a magnet link or a .torrent file." and "BitDinghy isn't set up to open magnet
  links and .torrent files." Double-clicking `queue-1.torrent` now makes Windows ask which program
  to use (close that question).

**Tidy up:** remove Queue test 2 (**Delete the files too**).

### Results

| Test | Result |
|---|---|
| 1. The setting | Passed |
| 2. Registering, and choosing it in Windows | Failed on Windows 10 (Explorer opened a folder), fixed in 3a9dcb9, then passed |
| 3. A .torrent file from Explorer, window closed | Passed |
| 4. A .torrent file while the window is open | Passed |
| 5. A magnet link from the browser | Passed |
| 6. Undo | Passed |

## M15: recommended settings

Tests 1 and 2 are data-free. **Test 3 is optional and uses data**: as much as your line downloads
in about 15 seconds at full speed (on a fast line that can be several hundred MB). Skip it if data
is precious; the automated tests cover the learning.

**Step 0: get ready**
1. Close the window, stop any `dinghy`, `cargo build --workspace`, PowerShell in
   `C:\cargo-target\debug`.

**Test 1: before anything is learned**
- Do: `.\dinghy.exe recommend`, then open the window and its **Settings**.
- You should see: "BitDinghy learns recommended settings from your own downloads (it never runs a
  speed test). Download something for a minute or two, without speed limits, then look again." in
  both places (in Settings under "Recommended settings (learned from your own downloads):").
  (If you did test 3 before, you see the learned speeds instead: go on to test 2.)

**Test 2: `--apply` refuses nothing it can't use**
- Do: `.\dinghy.exe recommend --apply`
- You should see: with nothing learned, the same message, and nothing changes in
  `.\dinghy.exe settings`.

**Test 3 (optional, uses data): learning and using the recommendation**
- Do: make sure `.\dinghy.exe settings` shows `download-limit:  0  (no limit)`. Add the Arch link
  (`https://archlinux.org/releng/releases/2026.10.01/torrent/`) in the window, let it run for 15
  seconds at full speed, then **Pause** it.
- You should see: in Settings, "Fastest seen: downloading X, uploading Y (10-second averages,
  without limits)." and "Download limit ..., upload limit ..., N downloads at the same time." with
  a **Use these** button and a small line explaining 90% and 80%. Press **Use these**: "The
  recommended settings are used from now on.", and the Download limit and Downloads at the same
  time boxes change. `.\dinghy.exe recommend` shows the same, with "Use them with: dinghy
  recommend --apply".
- Tidy up: set the Download and Upload limits back to 0 (or keep them), remove Arch (**Delete the
  files too**).

### Results

| Test | Result |
|---|---|
| 1. Before anything is learned | Not seen: speeds were already learned (the automated tests cover it) |
| 2. `--apply` with nothing learned | Not seen, as test 1; `--apply` with learned speeds passed |
| 3. Learning and using the recommendation (optional) | Passed (learned from the M9 Arch download; Use these and --apply) |

## M14: history and charts

Data-free, but best done after the M9 test on the same day (its short Arch download gives the
day's total something to show).

**Step 0: get ready**
1. Close the window, stop any `dinghy`, `cargo build --workspace`, PowerShell in
   `C:\cargo-target\debug`.

**Test 1: the History section**
- Do: open the window. Under the link box there is a folded **History**; open it.
- You should see: "Speed in the last 10 minutes (MiB/s):" with a small chart (two flat lines,
  Downloading and Uploading, while nothing runs); "Today: downloaded X, uploaded Y. Last 30 days:
  downloaded X, uploaded Y." (after the M9 test, today shows about 10 to 15 MiB downloaded);
  a bar chart "days ago (MiB per day)"; and "Removed downloads:" with the downloads you removed
  in the earlier tests, newest first, like "Queue test 1 (585.9 KiB), not finished, removed
  2026-10-11, 0 B uploaded".

**Test 2: removing a download puts it in the history**
- Do: add `%TEMP%\bitdinghy-m6-test\queue-1.torrent` (Open .torrent file...), then **Remove**,
  **Delete the files too**.
- You should see: within a second, "Queue test 1 (585.9 KiB), not finished, removed <today>, 0 B
  uploaded" at the top of "Removed downloads:".

**Test 3: `dinghy history`**
- Do: `.\dinghy.exe history`, then `.\dinghy.exe history --days 30`, then
  `.\dinghy.exe history --days 0`.
- You should see: "Downloaded and uploaded per day (last 7 days):" with a line per day that had
  something (today's date, "downloaded ... uploaded ..."), a "Together" line, and "Removed
  downloads (newest first):" with the same entries as the window (or "Nothing was downloaded or
  uploaded in the last 7 days." if nothing was). `--days 30` covers 30 days. `--days 0` is
  refused: "invalid value '0' for '--days <DAYS>': 0 is not in 1..=400".

**Test 4: the speed chart moves (optional, uses data)**
- Do: if you have a paused Arch download, resume it for 20 seconds, then pause it.
- You should see: the Downloading line rises while it runs and falls back to 0.

### Results

| Test | Result |
|---|---|
| 1. The History section | Passed |
| 2. Removing a download puts it in the history | Passed |
| 3. `dinghy history` | Passed |
| 4. The speed chart moves (optional) | Passed |

## M13: crash reports

Data-free. The crashes are made on purpose with a setting only used for testing.

**Step 0: get ready**
1. Close the window, stop any `dinghy`, `cargo build --workspace`, PowerShell in
   `C:\cargo-target\debug`.

**Test 1: `dinghy` crashes and says where the report is**
- Do: `$env:BITDINGHY_CRASH_TEST=1; .\dinghy.exe list` then `Remove-Item Env:BITDINGHY_CRASH_TEST`
- You should see: "dinghy ran into a problem it can't carry on from. A report was saved in
  C:\cargo-target\debug\data\crash-reports\crash-...-0.txt (it stays on this computer)." and
  Rust's own "panicked at ... a crash on purpose, because BITDINGHY_CRASH_TEST is set".

**Test 2: what a report says**
- Do: open that file in Notepad.
- You should see: "dinghy crash report", the line "This report stays on your computer. It may
  contain folder names (such as your user name). If you send it to anyone to help find the
  problem, read it first.", "Program: dinghy 0.1.0", "System: Windows 10 Pro ..., build ...",
  "Problem: ... a crash on purpose ...", and a "Where (backtrace):" list.

**Test 3: the window crashes, and says so next time**
- Do: `$env:BITDINGHY_CRASH_TEST=1; .\BitDinghy.exe` (the window closes at once, or never
  appears), then `Remove-Item Env:BITDINGHY_CRASH_TEST` and `.\BitDinghy.exe`.
- You should see: at the top of the window, in orange: "BitDinghy closed unexpectedly last time.
  A report was saved in C:\cargo-target\debug\data\crash-reports. It stays on this computer; if
  you report the problem, sending it along helps to find the cause." with **Open the folder** and
  **OK**. **Open the folder** opens Explorer there. Press **OK**: the message goes. Close and
  reopen the window: it doesn't come back.

**Test 4: `dinghy settings`**
- Do: `.\dinghy.exe settings`
- You should see: a line "Crash reports: 2 in C:\cargo-target\debug\data\crash-reports (they
  stay on this computer)".

**Tidy up:** delete the `crash-reports` folder in `C:\cargo-target\debug\data` if you like.

### Results

| Test | Result |
|---|---|
| 1. `dinghy` crashes and says where the report is | Passed |
| 2. What a report says | Passed |
| 3. The window crashes, and says so next time | Passed |
| 4. `dinghy settings` | Passed |

## M12: problems in plain words

Data-free. Test 2 uses your FAT32 USB stick (the one the repo is on); skip it if you have none.

**Step 0: get ready**
1. Close the window, stop any `dinghy`, `cargo build --workspace`, and make the test torrents
   (`cargo run -p dinghy-core --example make_test_torrent`): its last line is "Big-file test
   torrent (one 5 GiB file, nothing to download): ...\big-file-test.torrent".
2. Terminal in `C:\cargo-target\debug`.

**Test 1: no way to find anyone, said at once**
- Do: `.\dinghy.exe get "magnet:?xt=urn:btih:5a8b2f0c9e1d4b7a6c3e8f0a1b2c3d4e5f6a7b8c" --no-dht`
- You should see, straight away: "With --no-dht, BitDinghy can only find people through
  trackers in the link or with --peer, and this link has no trackers. Leave out --no-dht (or add
  --peer IP:PORT)."

**Test 2: a 5 GB file on a FAT32 drive**
- Do: make an empty folder on the USB stick, for example `D:\fat-test` (your stick's letter),
  then `.\dinghy.exe get "$env:TEMP\bitdinghy-m6-test\big-file-test.torrent" --dir D:\fat-test --no-dht --no-listen --peer 127.0.0.1:9`
- You should see: "BitDinghy didn't start this download because the drive D:\ uses FAT32, which
  can't hold files of 4 GB or more, and "big.iso" is 5.0 GiB. Choose a download folder on a drive
  formatted as NTFS in Settings (most internal drives are), or untick that file." The folder
  stays empty.
- Then: `.\dinghy.exe list` shows big.iso; remove it with `.\dinghy.exe remove <n>`, and delete
  the empty folder.

**Test 3: a file another program holds**
- Do: copy `%TEMP%\bitdinghy-m6-test\BitDinghy M6 test` into `Downloads\BitDinghy`. In a second
  PowerShell, hold one file open:
  `$f=[IO.File]::Open("$HOME\Downloads\BitDinghy\BitDinghy M6 test\Episode 2.bin",'Open','ReadWrite','None')`
  Then open the window and add `bitdinghy-m6-test.torrent` (Open .torrent file...), and press
  **Use the files already there**.
- You should see: a red message starting "Couldn't start this download: Another program has one
  of this download's files open (a video player or a virus scanner, for example). Close it, then
  try again. Details: error opening ...Episode 2.bin... (os error 32)".
- Then: in the second PowerShell `$f.Close()`, and press **Try again**: it checks the files and
  says Finished.

**Test 4: a paused download followed from the command line**
- Do: press **Pause** on BitDinghy M6 test. Run
  `.\dinghy.exe get "$env:TEMP\bitdinghy-m6-test\bitdinghy-m6-test.torrent"`.
- You should see: "BitDinghy's window is open, so the download was added there.", then
  "BitDinghy M6 test: paused at 100.0% (6.2 MiB of 6.2 MiB). Resume it in the window or with
  dinghy resume." (not a line with "0 B/s ETA unknown"). Press Ctrl+C.

**Tidy up:** remove BitDinghy M6 test (**Delete the files too**).

### Results

| Test | Result |
|---|---|
| 1. No way to find anyone, said at once | Passed (seen during M7) |
| 2. A 5 GB file on a FAT32 drive | Passed |
| 3. A file another program holds | Passed |
| 4. A paused download followed from the command line | Passed |

## M11: port and UPnP

Data-free. The window accepts connections as before; this only changes how.

**Step 0: get ready**
1. Close the window, stop any `dinghy`, `cargo build --workspace`, terminal in
   `C:\cargo-target\debug`.
2. `.\dinghy.exe settings` shows `accept-incoming:  on  (...)`, `listen-port:  0  (automatic: a
   different free port at each start)` and `upnp:  off  (the router is left alone)`.

**Test 1: the connection settings in the window**
- Do: open the window and **Settings**.
- You should see: "Connections (used the next time you start BitDinghy):", a ticked "Accept
  connections from other people (sharing works much better)", "Port: 0 (0 = automatic: a
  different free port at each start)", an unticked "Ask my router to forward this port (UPnP),
  ...", and small lines "Right now: accepting connections on port N." (some number) and "The
  first time BitDinghy accepts connections, Windows Firewall may ask ...".

**Test 2: a fixed port**
- Do: set Port to 51413 (click, type, Enter). Close the window and open it again.
- You should see: first "Saved. It is used the next time you start BitDinghy."; after reopening,
  "Right now: accepting connections on port 51413." (Close and reopen again: still 51413.)

**Test 3: a port another program uses**
- Do: close the window. In PowerShell (a second one), occupy the port:
  `$l=[System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::IPv6Any,51413); $l.Start()`.
  Open the window.
- You should see: an orange line "BitDinghy couldn't use port 51413 for incoming connections
  (...), so it uses port N this time. Another program may be using that port; choose another in
  Settings." and "Right now: accepting connections on port N." Close the window, then in the
  second PowerShell `$l.Stop()`.

**Test 4: not accepting connections**
- Do: open the window, untick "Accept connections from other people", close and reopen.
- You should see: the Port box and the UPnP box are greyed out, and "Right now: not accepting
  connections from other people."

**Test 5: the command line**
- Do: `.\dinghy.exe settings set accept-incoming on`, `.\dinghy.exe settings set upnp on`,
  `.\dinghy.exe settings set listen-port 80`.
- You should see: with the window open, "Saved. BitDinghy's open window uses it the next time it
  starts (close it and open it again)."; `upnp:  on  (BitDinghy asks the router to forward the
  port)`; and for port 80: "The port must be 0 (automatic) or between 1024 and 65535."
- Then (optional, only if your router shows UPnP forwardings): reopen the window and look in the
  router's page for a forwarding of port 51413 to this PC.

**Tidy up:** `.\dinghy.exe settings set upnp off`, `.\dinghy.exe settings set listen-port 0`.

### Results

| Test | Result |
|---|---|
| 1. The connection settings in the window | Passed |
| 2. A fixed port | Passed |
| 3. A port another program uses | Failed (BitDinghy shared the busy port), fixed in f631737, then passed |
| 4. Not accepting connections | Passed |
| 5. The command line | Passed |

## M10: sharing limits

Data-free: the M6 test torrent, finished at once from its own made-up files (nobody else is
involved, so nothing is uploaded; the time limit still counts).

**Step 0: get ready**
1. Close the window, stop any `dinghy`, `cargo build --workspace`, terminal in
   `C:\cargo-target\debug`. Make the test torrents if needed (`cargo run -p dinghy-core --example
   make_test_torrent`, from the repo folder).
2. Copy `%TEMP%\bitdinghy-m6-test\BitDinghy M6 test` into `Downloads\BitDinghy` (if it isn't
   there), and make sure BitDinghy M6 test is not in the list.
3. `.\dinghy.exe settings` shows `stop-sharing-at-ratio:  none  (finished downloads share while
   BitDinghy runs)` and `stop-sharing-after:  none`.

**Test 1: what a finished download says about sharing**
- Do: `.\dinghy.exe get "$env:TEMP\bitdinghy-m6-test\bitdinghy-m6-test.torrent" --use-existing-files --no-listen`
  (it says "Done: ..."), then open the window. (Not `--no-dht`: this torrent has no trackers, so
  BitDinghy would refuse at once, see M12 test 1.)
- You should see: BitDinghy M6 test, green "Finished: 6.2 MiB saved in ...", and under it
  "Sharing back to 0 people (0 B uploaded, ratio 0.00, for 3s). Closing BitDinghy stops sharing."
  with the time going up.

**Test 2: a time limit**
- Do: in **Settings**, set "Stop sharing a finished download at ratio 0.00 or after [1] minutes"
  (click the minutes number, type 1, Enter).
- You should see: "A finished download stops sharing after 1 minute of sharing, from now on."
  Within a minute the line under the download changes to "Stopped sharing: it reached the sharing
  limit (0 B uploaded, ratio 0.00, for 1m 0s). Resume shares it again." (or "1m 1s") and its
  button says
  **Share again**.

**Test 3: it stays stopped after a restart**
- Do: close the window, run `.\dinghy.exe list`, open the window again.
- You should see: `[active]` in `dinghy list` (the window is closed), and in the window the same
  "Stopped sharing..." line and the **Share again** button. With the window open, `.\dinghy.exe
  list` says `finished, stopped sharing at ratio 0.00 (0 B uploaded)`.

**Test 4: share again**
- Do: press **Share again**.
- You should see: "Sharing back to 0 people (...)" again, and it keeps sharing after a minute:
  the limit no longer applies to this download.

**Test 5: the command line**
- Do: `.\dinghy.exe settings set stop-sharing-at-ratio 1.5`, then
  `.\dinghy.exe settings set stop-sharing-after none`.
- You should see: "Saved. BitDinghy's open window uses it straight away." each time, and the
  settings lines `stop-sharing-at-ratio:  1.5  (a finished download stops sharing once it has
  shared that many times its size)` and `stop-sharing-after:  none`.

**Tidy up:** `.\dinghy.exe settings set stop-sharing-at-ratio none`; in the window remove
BitDinghy M6 test (**Delete the files too**).

### Results

| Test | Result |
|---|---|
| 1. What a finished download says about sharing | Passed (without --no-dht, see the test) |
| 2. A time limit | Passed |
| 3. It stays stopped after a restart | Passed |
| 4. Share again | Passed |
| 5. The command line | Passed |

## M9: speed limits

**This test uses about 10 to 15 MB of download data** (the official Arch Linux torrent, held to a
low speed and paused after half a minute).

**Step 0: get ready**
1. Close the window, stop any `dinghy`, `cargo build --workspace`, terminal in
   `C:\cargo-target\debug`. No Arch `.iso` in `Downloads\BitDinghy`, Arch not in the list.
2. `.\dinghy.exe settings` shows `download-limit:  0  (no limit)` and `upload-limit:  0  (no
   limit)`.

**Test 1: the totals line and the limit boxes**
- Do: open the window and open **Settings**.
- You should see: under "Saving to: ..." a line "In total: downloading 0 B/s, uploading 0 B/s.";
  in Settings "Download limit: 0  Upload limit: 0  (KiB/s, for all downloads together; 0 = no
  limit, else at least 32)".

**Test 2: too low is refused**
- Do: click the Download limit number, type 10, press Enter.
- You should see: a red line "The download limit must be 0 (no limit) or at least 32 KiB/s (and
  at most 4000000 KiB/s). Less than 32 KiB/s would stop downloads altogether." and the box goes
  back to 0.

**Test 3: a limit**
- Do: type 200 in the Download limit box, press Enter.
- You should see: "Speed limits, from now on: download 200.0 KiB/s, upload none." and the totals
  line ends "...downloading 0 B/s (limit 200.0 KiB/s), uploading 0 B/s."

**Test 4: the limit holds (uses data)**
- Do: paste `https://archlinux.org/releng/releases/2026.10.01/torrent/` into the link box, press
  Download, and watch for about 30 seconds.
- You should see: Arch's speed settles at about 200 KiB/s and never goes far above it (it may jump
  for a second at the start); the totals line says the same. Don't pause yet.

**Test 5: changing it from the command line**
- Do: `.\dinghy.exe settings set download-limit 500`
- You should see: "Saved. BitDinghy's open window uses it straight away." and the settings, with
  `download-limit:  500  KiB/s (500.0 KiB/s)`. In the window, within a few seconds Arch's speed
  climbs to about 500 KiB/s and the totals line says "(limit 500.0 KiB/s)". (The Settings box
  still shows 200 until the window restarts.) Now press **Pause** on Arch.

**Test 6: back to no limit**
- Do: `.\dinghy.exe settings set download-limit none`
- You should see: `download-limit:  0  (no limit)`, and in the window the totals line has no
  "(limit ...)" any more.

**Tidy up:** remove Arch (**Remove**, **Delete the files too**).

### Results

| Test | Result |
|---|---|
| 1. The totals line and the limit boxes | Passed |
| 2. Too low is refused | Passed |
| 3. A limit | Passed |
| 4. The limit holds | Passed |
| 5. Changing it from the command line | Passed |
| 6. Back to no limit | Passed |

## M8: the queue

Data-free: three little test torrents nobody shares, so each download simply waits at 0%.

**Step 0: get ready**
1. Close the window, stop any `dinghy`. `cargo build --workspace`, then
   `cargo run -p dinghy-core --example make_test_torrent`. It ends with "Queue test torrents:
   queue-1.torrent, queue-2.torrent and queue-3.torrent in ...\bitdinghy-m6-test".
2. Terminal in `C:\cargo-target\debug`. Run `.\dinghy.exe settings set max-active-downloads 1`.
   You should see "Saved. It will be used from now on, in the window too." and the settings, with
   the line `max-active-downloads:  1  (the others wait in the queue)`.
3. The window's list should be empty (remove anything left from earlier tests).

**Test 1: one at a time**
- Do: open the window. Open **Settings**: "Downloads at the same time: 1" with "(0 = no limit. The
  others wait in the queue; finished downloads that are sharing don't count.)". Press **Open
  .torrent file...**, go to `%TEMP%\bitdinghy-m6-test`, select `queue-1`, `queue-2` and `queue-3`
  together (Ctrl+click) and open them.
- You should see: **Queue test 1** with a progress bar and `0 B of 585.9 KiB ... 0 people
  connected`; **Queue test 2** and **Queue test 3** say "Queued: it starts when another download
  finishes (Settings: downloads at the same time)." Every row has two small buttons, ⏶ and ⏷,
  before its name.

**Test 2: moving changes which one runs**
- Do: press ⏶ on Queue test 3 twice.
- You should see: Queue test 3 is now at the top and has the progress bar; Queue test 1 (now
  second) says "Queued...". Hovering ⏶ says "Move up the list (downloads start in list order)".

**Test 3: pausing frees the place**
- Do: press **Pause** on Queue test 3.
- You should see: Queue test 3 "Paused at 0 B of 585.9 KiB..."; Queue test 1 starts (progress
  bar). Press **Resume** on Queue test 3: being first in the list, it takes the place back and
  Queue test 1 is "Queued" again.

**Test 4: two at a time**
- Do: in Settings, change "Downloads at the same time" to 2 (click the number, type 2, press
  Enter).
- You should see: "2 downloads run at the same time, from now on." and two downloads with
  progress bars; the third is queued.

**Test 5: the command line**
- Do: `.\dinghy.exe list`, then `.\dinghy.exe move 3 top`, then `.\dinghy.exe list` again.
- You should see: the list shows the third one as `[queued: starts when another download
  finishes]`. After the move: "Moved: Queue test 2 (in BitDinghy's open window)." (or the name
  that was third), it is first in the window, and it runs.

**Test 6: the order is remembered**
- Do: close the window, run `.\dinghy.exe list`, then open the window again.
- You should see: the same order in `dinghy list` (all three `[active]`) and in the window, with the
  same two running and one queued.

**Test 7: a setting from the command line reaches the open window**
- Do: with the window open, `.\dinghy.exe settings set max-active-downloads 0`.
- You should see: "Saved. BitDinghy's open window uses it straight away." and in the window all
  three downloads run (no "Queued" left). (The window's Settings box still shows 2 until the
  window is restarted.)

**Tidy up:** remove the three queue tests (**Remove**, **Delete the files too**), and
`.\dinghy.exe settings set max-active-downloads 3`.

### Results

| Test | Result |
|---|---|
| 1. One at a time | Passed |
| 2. Moving changes which one runs | Passed |
| 3. Pausing frees the place | Passed |
| 4. Two at a time | Passed |
| 5. The command line | Passed |
| 6. The order is remembered | Passed |
| 7. A setting from the command line reaches the open window | Passed |

## M7: checking files, and using files already there

Data-free like M6: it uses the M6 test torrent, whose files exist on your PC. Nobody shares it, so
nothing is ever downloaded; the files already there do all the work.

**Step 0: get ready**
1. Close the window, stop any `dinghy`. `cargo build --workspace`. If you haven't yet, make the
   test torrent: `cargo run -p dinghy-core --example make_test_torrent`.
2. Terminal in `C:\cargo-target\debug`. `BitDinghy M6 test` is not in the window's list, and
   `Downloads\BitDinghy` has no `BitDinghy M6 test` folder.
3. Copy the folder `%TEMP%\bitdinghy-m6-test\BitDinghy M6 test` (the made-up files) into
   `Downloads\BitDinghy`.

**Test 1: use the files already there**
- Do: open the window. **Open .torrent file...**, `%TEMP%\bitdinghy-m6-test`,
  `bitdinghy-m6-test.torrent`.
- You should see: the row turns red: "BitDinghy didn't start this download because a file with
  the same name is already in the download folder: ...\BitDinghy M6 test\Episode 1.bin.
  BitDinghy never overwrites files it didn't download. Move or rename that file, then try again."
  Under it, a new button **Use the files already there**; hovering it explains that the files are
  checked and completed, and then count as this download's files.
- Then: press it. For a moment "Checking files already on disk... N%", then "Finished: 6.2 MiB
  saved in ...". Nothing was downloaded (nobody shares this torrent): the files you copied were
  complete.

**Test 2: Check files**
- Do: press **Check files** (next to Pause).
- You should see: "Checking files already on disk... N%" with a spinner, then green "Finished"
  again and a teal line "Checked: every chosen file is complete and correct."

**Test 3: a damaged file is found**
- Do: in PowerShell, damage the start of Episode 2 (this writes 100 bytes into it):
  `$f=[IO.File]::Open("$HOME\Downloads\BitDinghy\BitDinghy M6 test\Episode 2.bin",'Open','Write','ReadWrite'); $f.Write([byte[]](1..100),0,100); $f.Close()`
  Then press **Check files**.
- You should see, in orange: "Checked: 256.0 KiB of the chosen files were missing or damaged. BitDinghy
  downloads them again." The progress bar shows 96.0%, with 0 people connected: nobody shares
  this test torrent, so the damaged piece can't come back. (With a real torrent it would.)

**Test 4: `dinghy check` with the window open**
- Do: `.\dinghy.exe list` (note the number `<n>`), then `.\dinghy.exe check <n>`.
- You should see: "Checking the files of "BitDinghy M6 test" in BitDinghy's window...", a
  "checking files already on disk... N%" line, then "Checked: 256.0 KiB of the chosen files were
  missing or damaged. BitDinghy downloads them again." The window shows the same.

**Test 5: `dinghy check` with the window closed**
- Do: close the window. `.\dinghy.exe check <n>`.
- You should see: "Checking the files of "BitDinghy M6 test" (nothing is downloaded)...", then
  "Checked: 256.0 KiB of the chosen files are missing or damaged. They are downloaded again the
  next time BitDinghy runs this download."

**Test 6: `dinghy get --use-existing-files`**
- Do: `.\dinghy.exe remove <n>` (keeps the files). Then
  `.\dinghy.exe get "$env:TEMP\bitdinghy-m6-test\bitdinghy-m6-test.torrent" --no-listen`.
  (Not `--no-dht`: this torrent has no trackers, so with `--no-dht` BitDinghy says at once that it
  could find nobody, see M12 test 1. The DHT only learns that nobody has this made-up torrent.)
- You should see: the same "a file with the same name is already in the download folder" message,
  and "To use the file that is already there instead, run the same command again with
  --use-existing-files (BitDinghy checks it first and completes it)."
- Then: run it again with `--use-existing-files` at the end: "Saving to ...", a progress line at
  96%, `peers 0` (the damaged piece from test 3 can't come back). Press Ctrl+C.

**Tidy up:** `.\dinghy.exe remove <n> --delete-files`, answer `yes`.

### Results

| Test | Result |
|---|---|
| 1. Use the files already there | Passed |
| 2. Check files | Passed |
| 3. A damaged file is found | Passed (the orange colour from 3c44473 is re-checked at the end) |
| 4. `dinghy check`, window open | Passed |
| 5. `dinghy check`, window closed | Passed |
| 6. `dinghy get --use-existing-files` | Passed (without --no-dht, see the test) |

## M6: choosing which files to download

Tests are done one at a time: do the test, compare with "You should see", then go to the next.
(The big guided test of M6 to M18 ran from 2026-10-11 to 2026-10-14; the results tables show it.)
**No download data is used:** the test torrent is made on your PC and nobody shares it, so it
stays at 0%. The window still knows its four files at once, because they are in the `.torrent`.
(That only the chosen files' bytes arrive, byte for byte, is checked by the automated tests.)

**Step 0: get ready**
1. Close the BitDinghy window and stop any `dinghy` (Ctrl+C).
2. In the repo folder: `cargo build --workspace`.
3. Still in the repo folder, make the test torrent: `cargo run -p dinghy-core --example make_test_torrent`.
   It prints `Test torrent: C:\Users\<you>\AppData\Local\Temp\bitdinghy-m6-test\bitdinghy-m6-test.torrent`,
   "It holds 4 made-up files..." and "Nobody shares it, so adding it to BitDinghy downloads nothing."
4. Open a terminal in `C:\cargo-target\debug` (`cd C:\cargo-target\debug`).
5. Make sure `Downloads\BitDinghy` has no folder called `BitDinghy M6 test`.
6. In Explorer, turn **hidden items off** (Windows 10: **View** tab, untick **Hidden items**;
   Windows 11: **View > Show**, untick **Hidden items**). Some tests turn them on for a moment.

**Test 1: the Files button**
- Do: open the window (`.\BitDinghy.exe`), press **Open .torrent file...**, type
  `%TEMP%\bitdinghy-m6-test` in the dialog's address bar, press Enter, and open
  `bitdinghy-m6-test.torrent`.
- You should see: a row **BitDinghy M6 test** with a progress bar at 0.0% and the line
  `0 B of 6.2 MiB   |   0 B/s   |   unknown left   |   0 people connected (0 known)` (it may
  say "Checking files already on disk..." for a moment first). Next to **Pause** there is
  a new button **Files**. (A one-file torrent, like Arch, has no Files button: nothing to choose.)

**Test 2: the file list**
- Do: press **Files**.
- You should see: the button now says **Hide files**. Under the name, a box: "4 of 4 files chosen.
  Unticked files are not downloaded.", a small line "What is left of unticked files in the folder
  is hidden (the engine needs it to check the files next to them).", and four ticked rows:
  `Episode 1.bin` (2.9 MiB, 0%), `Episode 2.bin` (1.9 MiB, 0%), `Extras\Making of.bin`
  (1.4 MiB, 0%), `Extras\Poster.txt` (40 B, 0%).

**Test 3: untick a file**
- Do: untick **Episode 2.bin**.
- You should see: "3 of 4 files chosen." and a **Choose all** button beside it. The line under the
  progress bar now says `0 B of 4.4 MiB`. (The files add up to 4.3 MiB; the total counts whole
  pieces, and a piece can spill a little into the unticked file next to it.) Press **Hide files**:
  the button says **Files (3 of 4)**. Press it again to open the list.

**Test 4: the last file can't be unticked**
- Do: untick **Episode 1.bin** and **Making of.bin**, so only **Poster.txt** is ticked. Move the
  mouse over Poster.txt's tick box.
- You should see: "1 of 4 files chosen.", Poster.txt's tick box greyed out, and the tip "At least
  one file stays chosen. To stop the whole download, pause or remove it."

**Test 5: unticked files are hidden in the folder**
- Do: open `Downloads\BitDinghy\BitDinghy M6 test` in Explorer (hidden items off).
- You should see: only the **Extras** folder; inside it only **Poster.txt**. The other three files
  are there but hidden: turn hidden items on and they appear, faded. Turn hidden items off again.
- Then: in the window press **Choose all**. Explorer (press F5) shows all four files again, and
  the list says "4 of 4 files chosen."

**Test 6: the choice is remembered**
- Do: untick **Episode 2.bin**. Close the window. Run `.\dinghy.exe list` and note the number of
  BitDinghy M6 test (written `<n>` below; it is 1 if it is the only download). Run
  `.\dinghy.exe files <n>`.
- You should see:
  ```
  BitDinghy M6 test: 3 of 4 files chosen (4.3 MiB):
    [x]   1. Episode 1.bin  2.9 MiB
    [ ]   2. Episode 2.bin  1.9 MiB
    [x]   3. Extras\Making of.bin  1.4 MiB
    [x]   4. Extras\Poster.txt  40 B
  Choose with: dinghy files <n> --pick 1,3-4   (numbers and ranges, or "all")
  ```
  (No percentages: with the window closed, nothing is running.)

**Test 7: choose with the command line, window closed**
- Do: `.\dinghy.exe files <n> --pick 2`
- You should see: "BitDinghy M6 test: now downloading 1 of 4 files (1.9 MiB)." and "It is used the
  next time BitDinghy runs this download."
- Then: open the window. The button says **Files (1 of 4)**, the line says `0 B of 2.2 MiB`, and
  Explorer (F5) shows only **Episode 2.bin**: the other files, and the Extras folder (nothing in
  it is ticked), are hidden.

**Test 8: choose with the command line, window open**
- Do: with the window open, run `.\dinghy.exe files <n> --pick all`, then `.\dinghy.exe files <n>`.
- You should see: "BitDinghy M6 test: now downloading 4 of 4 files (6.2 MiB)." and "BitDinghy's
  open window uses it straight away." The list shows four `[x]` rows, each ending in `0%` (the
  window is running it, so it knows how far each file got). The window's button says **Files**
  and the line `0 B of 6.2 MiB`; Explorer shows all four files.

**Test 9: mistakes are refused**
- Do: `.\dinghy.exe files <n> --pick 9`, then `.\dinghy.exe files <n> --pick x`.
- You should see: "There is no file number 9: this download has 4 files." and then
  `"x" isn't a file number. Use numbers and ranges such as 1,3-5 (or the word all); ...`.
  Nothing changes.

**Test 10: Remove, delete the files too (hidden ones included)**
- Do: in the window untick **Episode 1.bin** (it becomes hidden). Press **Remove**.
- You should see: the question has a new last sentence: '"Keep the files" also shows any hidden
  leftovers of unticked files again.' Press **Delete the files too**: "Removed "BitDinghy M6 test"
  and deleted 4 files it downloaded." The `BitDinghy M6 test` folder is gone (check with hidden
  items on, then turn them off again).

**Test 11: Remove, keep the files, shows everything again**
- Do: add the test torrent again (Open .torrent file..., same file). Press **Files** and untick
  **Episode 1.bin** and **Making of.bin**. Explorer (F5): only Episode 2.bin and Extras\Poster.txt
  show. Now press **Remove**, then **Keep the files**.
- You should see: "Removed "BitDinghy M6 test". Its files are still in ...", and in Explorer
  (F5, hidden items off) all four files show again: BitDinghy no longer looks after them, so it
  leaves nothing hidden.

**Tidy up:** delete the `Downloads\BitDinghy\BitDinghy M6 test` folder yourself (it is no longer
in BitDinghy's list), and `%TEMP%\bitdinghy-m6-test` if you like.

### Results

| Test | Result |
|---|---|
| 1. The Files button | Passed |
| 2. The file list | Passed |
| 3. Untick a file | Passed |
| 4. The last file can't be unticked | Passed |
| 5. Unticked files are hidden in the folder | Passed |
| 6. The choice is remembered | Passed |
| 7. Choose with the command line, window closed | Passed |
| 8. Choose with the command line, window open | Passed |
| 9. Mistakes are refused | Passed |
| 10. Remove, delete the files too | Passed |
| 11. Remove, keep the files | Passed |

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
6. Arch downloads fast (over 1 GB in under a minute on a good line): press **Pause** as soon as it
   appears in each test.

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
`Downloads\BitDinghy`, Arch not in the list). The link used here is the official Arch one:
`https://archlinux.org/releng/releases/2026.10.01/torrent/` (fetching it uses about 90 KB).
Arch downloads fast: after test 1, keep it in the list, **paused**. The later tests then fetch the
`.torrent` again without downloading Arch itself.

**Test 1: paste a web link in the window**
- Do: open the window, paste the Arch link into the link box, press **Download**.
- You should see: for a moment a teal spinner and "Fetching the .torrent file from
  archlinux.org...", then the Arch download appears (no "Looking for people" wait for the file
  list). The link box is empty again. Press **Pause** straight away.

**Test 2: plain http links are refused by default**
- Do: paste the same link but starting with `http://` instead of `https://`, press Download.
- You should see: no spinner (nothing is contacted), and a red line "BitDinghy only opens secure
  (https://) links to .torrent files, because a plain http:// link can be changed on the way. You
  can allow plain http links in Settings." Arch is unchanged.

**Test 3: the setting**
- Do: open **Settings**. Tick "Also open plain http:// links to .torrent files...".
- You should see: "Plain http:// links are opened too, from now on." Press Download again with the
  `http://` link: now it is fetched (spinner; archlinux.org sends BitDinghy on to its https address,
  which is fine), and there is still one Arch row, still paused (same torrent). Untick the box:
  "Only secure https:// links are opened, from now on." The `http://` link is refused again.

**Test 4: links to your own PC or home network are refused**
- Do: paste `https://192.168.1.1/x.torrent` and press Download.
- You should see: no spinner, and "BitDinghy doesn't open links to this computer or the local
  network (192.168.1.1)." Nothing is contacted. (`https://localhost/x.torrent` is refused the same
  way.)

**Test 5: a link that isn't a .torrent**
- Do: paste `https://archlinux.org/download/` (the download PAGE, not the torrent) and press
  Download.
- You should see: a spinner for a moment, then "That link didn't lead to a .torrent file
  (archlinux.org sent something else)."

**Test 6: `dinghy get` with a web link (window closed)**
- Do: close the window, run
  `.\dinghy.exe get "https://archlinux.org/releng/releases/2026.10.01/torrent/" --no-dht --no-listen`
  (without the DHT it finds nobody, so no download data is used).
- You should see: "Fetching the .torrent file from archlinux.org...", "Saving to ...", then Arch's
  progress line straight away (`0 B/s`, `peers 0`). Press Ctrl+C: "Stopped. Files already saved
  stay in place; run the same command again to continue."
- Then: `dinghy get` means "carry on", so Arch is no longer paused. Run `.\dinghy.exe pause 1`:
  "Paused: archlinux-2026.10.01-x86_64.iso. It stays paused, in the window too, until you resume
  it."

**Test 7: `dinghy get` with a web link while the window is open**
- Do: open the window (Arch is paused) and run
  `.\dinghy.exe get "https://archlinux.org/releng/releases/2026.10.01/torrent/"`.
- You should see: after a moment (the window fetches the file), "BitDinghy's window is open, so the
  download was added there", "Saving to ...", and the terminal follows Arch (`0 B/s`, because it is
  paused in the window). The window still has one Arch row, paused. Ctrl+C: "Stopped following.
  The download carries on in BitDinghy's window."

**Test 8: the setting from the command line**
- Do: close the window. Run `.\dinghy.exe settings`.
- You should see: a line `allow-http-links:  off  (only secure https:// links)`. Then
  `.\dinghy.exe settings set allow-http-links on` says "Saved. It will be used from now on, in the
  window too." and shows it `on`. Open the window: in **Settings** the http box is ticked. Close
  the window; `.\dinghy.exe settings set allow-http-links off` puts it back.

**Tidy up:** remove Arch (Delete the files too).

### Results (the owner's test, 2026-10-10)

| Step | Test | Result |
|---|---|---|
| 1 | 1. "Open .torrent file..." button | Passed |
| 1 | 2. The same torrent twice is one download | Passed |
| 1 | 3. Drag and drop | Passed |
| 1 | 4. Pasting a path | Passed |
| 1 | 5. A file that isn't a torrent | Passed |
| 1 | 6. `dinghy get` with a .torrent file, window closed | Passed |
| 1 | 7. `dinghy get` with only the file name, window open | Passed |
| 2 | 1. A web link in the window | Passed after a fix: the spinner was invisible (dark on dark); it is teal now |
| 2 | 2. Plain http refused by default | Passed |
| 2 | 3. The http setting in the window | Passed |
| 2 | 4. Home-network links refused | Passed |
| 2 | 5. A link that isn't a .torrent | Passed |
| 2 | 6. `dinghy get` with a web link, window closed | Passed |
| 2 | 7. `dinghy get` with a web link, window open | Passed |
| 2 | 8. The setting from the command line | Passed |

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
