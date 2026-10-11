# How BitDinghy works

A high-level tour of what the program does, how it is organised, and why.

## What a download actually is

A magnet link is a fingerprint of the content, not the content itself. Roughly:

1. The client asks the **DHT** (a distributed phone book spread across all users) and the link's
   trackers: "who has this fingerprint?"
2. It connects to those **peers** and first fetches the torrent's description: the file list and
   a hash for every chunk.
3. It downloads **chunks (pieces)** from many peers at once and checks each one against its hash.
   A bad chunk is thrown away. This is how "the files arrived intact" works.
4. It hands chunks to other peers (**seeding**).

BitDinghy does not implement any of this. [librqbit](https://github.com/ikatson/rqbit) does.
BitDinghy builds everything around that engine: the queue, limits, history, safety checks, the
command line and the window.

## How the program is organised

```
  BitDinghy.exe (window)        dinghy.exe (command line)
          |                               |
          |            same-PC-only channel (named pipe / socket file)
          v                               v
      +------------------ dinghy-core ------------------+
      |  commands in -> engine -> snapshots out          |
      |  queue, limits, seeding rules, safety checks     |
      +-------+------------------+-----------------------+
              |                  |
          librqbit       settings.toml + downloads.db
       (the internet)        (next to the program)
```

Crate names are the Cargo packages in `crates/`:

| Crate | Produces | Role |
|---|---|---|
| `dinghy-core` | library | The shared engine. The only code allowed to talk to librqbit. |
| `dinghy-cli` | `dinghy` | Thin command-line front-end. |
| `bitdinghy-app` | `BitDinghy` | Thin window front-end (egui). |

### Design decisions

- **One shared library, two thin front-ends.** Every feature lives in `dinghy-core`, so "works in
  both the command line and the window" comes for free.
- **Commands in, snapshots out.** A front-end sends a request ("add this link", "pause 3"). The
  engine publishes a snapshot (list, progress, speeds) that the front-end draws. This is the same
  for both front-ends, which is also what lets a future download type plug into the same slot.
- **One engine at a time.** The window owns the engine. The command line talks to it over a
  channel that is not a network port, so other computers and web pages cannot reach it (M4, see
  below). With the window closed, `dinghy list` reads the saved list and `dinghy get <link>`
  downloads in the terminal. Letting the window attach to a command-line engine is on the TODO
  list.
- **Everything saved is ours, and versioned.** Settings go in a readable TOML file. The download
  list, queue order and history go in one SQLite database with a version number. We also keep what
  is needed to re-add each torrent, so upgrading librqbit can never lose the list.
- **Portable.** Data lives next to the program. If that folder is not writable, it falls back to
  the user's normal settings folder.

### Why this shape

- Wrapping an engine gives a working, tested BitTorrent client from day one.
- A single crate touching librqbit means version changes, the missing encryption and the missing
  "re-check" function are handled in one place.
- Torrents are written by strangers, so inputs are checked at one door: a path guard keeps every
  file inside the download folder, and size caps limit what is accepted.
- Privacy by construction: nothing in the design opens a connection except peers, trackers, the
  DHT and the user's router (UPnP).

## How a download starts (M1)

Everything a magnet link or torrent says comes from a stranger, so it passes three gates before
anything is written to disk:

1. **Clean the link** (`magnet.rs`). The pasted text is never given to the engine as it is. We read
   out the fingerprint (strictly validated), a tidied name and a few ordinary internet trackers,
   then build a fresh minimal link from only those.
2. **Ask for the file list only** (`engine.rs`). The engine finds peers and fetches the list of
   file names and sizes. It creates nothing on disk in this step.
3. **Check every file name** (`safety.rs`). Names that could end up outside the download folder, or
   that Windows treats specially (`C:evil`, `NUL`, ...), stop the download with a plain message.
   Only after this does the real download start, reusing the file list already fetched.

While it runs, `Engine::snapshot()` gives a plain picture of every download (progress, smoothed
speed, time left, people connected) that the window and the command line both draw from.

## Where BitDinghy keeps its files (M2)

```
 BitDinghy.exe   dinghy.exe        data\                      <- next to the programs
        \          /                 settings.toml  (versioned, readable)
         Profile::load()  ------->   dht.json       (the DHT's saved contacts)
   (finds the data folder,           downloads.db   (the saved download list, M3)
    loads the settings)              downloads.db.lock  (held while BitDinghy runs)
```

- **`data_dir.rs`** decides the folder: the `data` folder next to the programs if it can be
  written to (proved by creating and deleting a small file), otherwise the user's own settings
  folder. `BITDINGHY_DATA_DIR` overrides both.
- **`settings.rs`** reads and writes `settings.toml` and carries the "never lose your data" rules:
  - the file records the settings-format **version**, and old versions are upgraded step by step;
  - a file that can't be understood is **kept aside** (`settings.toml.broken-<time>`), never deleted;
  - a file from a **newer** BitDinghy is never overwritten;
  - saving writes a temporary file and swaps it in, so a crash can't leave half a file.
- **`profile.rs`** is the one call both programs make at start-up. It never fails: if something is
  wrong, BitDinghy carries on with standard settings and shows a notice.

## The saved download list (M3)

```
  add a link --> saved_list: remember it -----------------+
       |                                                  |
       v                                                  v
  1. file list only --> 2. safety check --> remember the checked file list
                                 |                        |
                                 v                        |
                        3. start for real                 |
                                                          |
  next start: every saved download goes through 1-2-3 <--+
              again, from the saved file list (no need to ask anyone)
```

- **`saved_list.rs`** keeps `downloads.db`, an SQLite file, with the same promises as the
  settings: a version number inside the file and step-by-step upgrades, a damaged file kept aside
  as `downloads.db.broken-<time>`, a list from a newer BitDinghy never changed. SQLite writes each
  change completely or not at all, so a crash or a pulled USB stick can't leave half a change.
- For each download it keeps the fingerprint, the name, the cleaned magnet link, the **checked
  file list** (once known), the download folder it was started in, the exact folder its files are
  in, and whether it is paused. A remembered download stays in its own folder even if the
  download-folder setting changes later.
- **Why not librqbit's own saved list?** It exists, but a damaged file stops its engine from
  starting at all, its format isn't ours to version, and it re-adds torrents without our safety
  check. So it stays off, and each start re-checks the files already on disk instead (a few
  seconds per GB).
- **One BitDinghy at a time.** Opening the list for changes takes a lock on `downloads.db.lock`.
  Windows releases it when the program ends, even after a crash. A second window or `dinghy get`
  gets a plain "BitDinghy is already running" message. `dinghy list` only reads, so it works any
  time.
- The window brings back every saved download when it opens. `dinghy get` only starts the one it
  was given: if that link is already in the list, it carries on in its own folder from the saved
  file list.

### Pause and resume

- Each download carries a **"wants to be paused" wish**, saved in the list. The engine acts on it
  whenever it can: right away if the torrent is running, or the moment it starts (a torrent whose
  file list was still on its way starts paused). Because the wish is stored first and the torrent
  is checked second, a pause pressed at any moment is never lost.
- A download paused before its file list arrived is "stopped": after a restart nothing runs for
  it and nobody is contacted until it is resumed. A paused download whose file list is known is
  added paused, so its files are checked and its progress shows, but no one is contacted.
- **Resume** on a download that failed tries it again from the start, through the safety check.
- `dinghy pause <n>` / `dinghy resume <n>` change the saved list while BitDinghy isn't running
  (they need the lock, like an engine). With the window open they are refused until M4.

### Remove, and "only ever delete our own files" (`owned_files.rs`)

BitDinghy deletes files only when the person asks ("Delete the files too"), and then only files it
downloaded itself. Two rules make that true:

1. **A new download never takes over an existing file.** Before a new download starts, every
   path in its checked file list is looked up. If any already exists, the download is refused
   with a message naming the file. (A download continuing from its own saved file list skips
   this: the files there are its own.) So every file at one of a download's paths was written by
   that download.
2. **What gets deleted comes from the download's own checked file list**, never from looking
   around in the folder. The list goes through the same safety guard again, and its folder must
   be the download folder itself (one file) or one folder directly inside it (several files).
   After the files, folders they leave empty are removed, deepest first; a folder that still holds
   anything is left alone, and the download folder itself is never removed.

**A download never changes folder.** Rule 1 relies on a continuing download being in the same
folder it was checked in. librqbit can name a torrent's folder differently from one start to the
next (for a torrent with several files and no name it used the magnet link's `dn=` the first
time and the biggest file's name the next), so the M3 security review found a way to move a
download into a folder holding someone else's files. Two fixes: a continuing download always
uses its **saved** folder (librqbit's new choice is ignored, and the saved folder must have the
shape above), and a torrent with several files but **no name** is refused outright (the
BitTorrent standard requires one), so a magnet link's `dn=` never decides a folder.

On Windows a file can stay "in use" for a moment after the engine lets go of it, so deleting
tries again for up to two seconds. The engine is told to forget the torrent first, so it closes
its files. A removal that happens while the download is still starting is caught when the start
finishes, and the torrent is taken straight back out.

## The channel between the command line and the window (M4, `remote.rs`)

```
 dinghy list / get ...                         BitDinghy (window, owns the engine)
        |                                                 |
        |  1. is a window listening on this data          |  listens on a channel named
        |     folder's channel?                           |  after its data folder
        |---- one JSON line: {"version":2,"command":..} ->|
        |<--- one JSON line: the answer ------------------|  same engine calls as its buttons
        |
        +-- nobody listening: work alone, as before (own engine, or the saved list)
```

- **Only this computer can use it.** On Windows the channel is a *named pipe* (a named local
  message channel) created with "reject remote clients", so connections over the network are
  refused by Windows itself. Windows' standard protection for a new pipe lets only this user (plus
  the system and administrators) send to it; other users can at most open it to read, and the
  window never speaks first. On Linux it is a socket file in the data folder that only its owner
  may open. It is never a network port or a local web server, so web pages can't reach it.
- **One channel per data folder.** The pipe's name is made from the data folder's path and the
  user's name, so a second portable copy of BitDinghy has its own channel. The window opens the
  channel as its "first instance": if anything already uses that name, it says so instead of
  sharing it.
- **Plain, versioned messages.** Each request and answer is one line of JSON. Every request says
  which message version it speaks; a different version gets a plain answer ("use the dinghy.exe
  next to BitDinghy.exe"), never a guess. Nonsense gets "didn't understand", and the conversation
  goes on. A message over 4 MB ends the conversation.
- **No new way around the rules.** A request runs exactly the engine call the window's own
  buttons run (`add_magnet`, `snapshot`...), with all the same checks.
- With the window open, `dinghy get` hands the link to the window and follows its progress; Ctrl+C
  only stops following. `--dir` is refused (the window decides the folder), and `--no-dht` /
  `--no-listen` don't apply.
- `dinghy pause/resume/remove <n>` name the download by its number in the window's list (the same
  numbers `dinghy list` shows while the window is open). `remove --delete-files` asks the window
  which files it would delete (a request that changes nothing), shows them, and only after "yes"
  sends the removal, naming the download by its **fingerprint**: if the list changed in between,
  it can't hit a different download.
- Which side answers: the window, if it is listening; otherwise, if something holds the saved list
  (that is `dinghy get` in another terminal), changes are refused with "busy in another terminal";
  otherwise `dinghy` changes the saved list itself.

## .torrent files (M5, `torrent_file.rs` and `input.rs`)

- **`input.rs`** decides what was typed, pasted or dropped: a magnet link (`magnet:`), a web link
  (`http(s)://`), or else a `.torrent` file path (Windows' "Copy as path" quotation marks are
  ignored). `Engine::add` takes any of them; `dinghy get`, the link box, the file dialog,
  drag & drop and the window's channel all go through it.
- **`torrent_file.rs`** treats a `.torrent` like a magnet link: untrusted, so it is never given to
  the engine as it is. It is read (at most 16 MB), checked as a valid file list, and **rebuilt**
  from the info part (byte for byte, so the fingerprint stays the same) plus the trackers that
  pass `magnet.rs`'s filter (ordinary internet addresses only). Everything else in the file
  (comments, web-seed addresses, the original trackers) is left out. The rebuilt file is checked
  again and must have the same fingerprint. A clean magnet link is built too, for the saved list.
- From there it is a normal download: the same safety guard, the same "never overwrite" rule, and
  it is remembered with its rebuilt file list.
- `dinghy get` turns a file path into a full path before handing it to the window, because the
  window runs in another folder.
- **Web links (`web_link.rs`).** librqbit could fetch them itself, but with no limits at all, so
  BitDinghy fetches them (with `reqwest`, already in the build): https only unless the
  `allow_http_links` setting is on; the address, and every address a redirect leads to, must be
  an ordinary internet one (the same rule as the tracker filter, `magnet::is_internet_host`); at
  most 5 redirects, 30 seconds and 16 MB. The result goes through `torrent_file::check` like any
  `.torrent`. `Engine::add_any` handles all three kinds of input; it is `async` because fetching
  takes time, and the window runs it in the background with a "Fetching..." line. With the window
  open, `dinghy get <link>` lets the window fetch, with the window's own setting.

## Choosing files (M6, `file_choice.rs`)

- Every download starts with **all its files chosen**. The window's **Files** button (shown when a
  torrent has more than one file) opens a list with tick boxes; `dinghy files <n>` prints the same
  list and `dinghy files <n> --pick 1,3-5` (or `all`) changes it. A change applies straight away
  while a download runs or is paused, and is remembered. At least one file stays chosen; to stop
  everything, pause or remove the download.
- **Numbers for people, indexes for the engine.** A torrent's file list can hold "padding"
  entries that are never written to disk. People see only real files, numbered 1, 2, 3... The
  saved list stores the torrent's own index of each **unticked** file (`skipped_files`, for
  example `"1,3"`; empty means everything, which is also what an older list means). A saved
  choice is checked against the real file list before use: indexes that aren't real files are
  dropped, and "everything unticked" becomes "nothing unticked".
- The engine gets the choice at step 3 (`only_files`), and through `update_only_files` while a
  download runs. The progress, the total and "finished" then count only the chosen files.
  Choosing is refused for a moment while a download is starting or checking its files (the engine
  can't change it then), with a plain "try again in a moment".
- **What librqbit leaves on disk for unticked files.** It creates every file of a torrent, ticked
  or not. An unticked file is normally left empty, but where a piece crosses from a ticked file
  into an unticked one, that part of the unticked file is written too. On Windows (NTFS) librqbit
  marks the files "sparse", so the unused rest takes no real disk space, though Explorer shows the
  full size.
- **Leftovers are hidden, not deleted** (owner's decision). Deleting them was tried: it works even
  while librqbit holds them open, but at the next start librqbit creates them again, and the
  pieces shared with a ticked neighbour fail their check (half their data is gone), so a finished
  download goes back to "downloading" until someone sends those pieces again, forever if nobody
  shares it any more. And a deleted file ticked again would be written into the deleted copy.
  So BitDinghy marks unticked files **hidden** (Windows' file mark), and folders holding only
  unticked files too (`owned_files::leftover_marks`, `apply_hidden`): once librqbit has set a
  download up (`wait_until_initialized`), and after every change of choice. Ticked files are
  made visible. "Remove, keep the files" makes everything visible again (nothing BitDinghy no
  longer looks after stays hidden); "Remove, delete the files too" deletes them like any other
  file of the download. The paths come from the checked file list, with the same checks as
  deleting; links are never touched. Linux has no hidden mark (nothing happens there).
- A better way, for later: keep the bits of unticked files in a hidden "parts" file of
  BitDinghy's own (as qBittorrent does), so they never appear at all. It needs a storage layer of
  our own under librqbit (see `TODO.md`).
- The saved list is now **version 2** (one new column, added by an upgrade step); the channel's
  messages are **version 2** (`Files`, `ChooseFiles`, and file counts in each download).

## Checking files, and using files already there (M7)

- **"Check files"** (window) and **`dinghy check <n>`** read a download's files on disk again and
  compare every piece with the torrent's fingerprints. librqbit has no "check again" call, but it
  checks everything already on disk whenever a torrent is added, so `Engine::check` takes the
  download out of librqbit (its files stay) and adds it again from its saved file list. When
  librqbit has finished checking, the download gets a `note`: "every chosen file is complete and
  correct", or how much was missing or damaged (that part is downloaded again; a paused download
  stays paused). While it checks, the window and `dinghy` show a percentage.
- With the window closed, `dinghy check` runs an engine of its own with no DHT, no listening and
  no local search, adds just that download paused (`Engine::check_saved`; the saved list is not
  changed), reports, and stops. Nothing is downloaded and nobody is contacted.
- **"Use the files already there"** answers M3's "never take over a file" refusal: when a new
  download is refused because a file with one of its names exists, the window offers this button
  (`dinghy get <link> --use-existing-files` on the command line). The files are checked like any
  files on disk, and completed; parts that don't match are replaced. From then on they count as
  the download's own files, so "Remove, delete the files too" deletes them. It is always the
  person's explicit choice.
- **Never through a link.** Before a download starts, every folder and file on the way to its
  files (inside the download folder) is looked at: a symbolic link or a junction to another place
  is refused (`owned_files::refuse_links`, `Error::LinkInTheWay`), because writing "into" it
  would write outside the download folder. Cloud placeholders (OneDrive) are not links. The
  download folder itself may be a link: it is the person's own choice.

## The queue (M8)

- At most **"downloads at the same time"** downloads run at once (setting `max_active_downloads`,
  standard 3, 0 = no limit). The others wait as **Queued**, in list order. Finished downloads that
  are only sharing, downloads the person paused, and failed ones take no place.
- The engine's list order IS the queue order. **Up/Down** in the window and `dinghy move <n>
  up|down|top|bottom` change it; it is saved (saved list **version 3** adds a `position` column;
  older lists keep their oldest-first order when upgraded).
- `Shared::rebalance` decides, after every change (add, remove, pause, resume, move, a new limit)
  and once a second (so a finished download lets the next one start). A queued download either
  hasn't started at all (`EntryState::Waiting`: nobody is contacted) or is paused inside librqbit
  by the queue (`Entry::held`, shown as Queued, not Paused). Downloads that are fetching their file
  list or checking files can't be held back, so they keep their place.
- To make this possible the download list moved into `Shared` (with the runtime handle), so the
  once-a-second task can reach it; `spawn_start`, `start_again` and `rebalance` live there.
- Settings changed with `dinghy settings set` reach an open window straight away (new channel
  requests `SetMaxActiveDownloads`, `SetAllowHttpLinks`), and saving a setting starts from the
  file as it is now, so the window and `dinghy` can't undo each other's changes.

## Speed limits (M9)

- Two settings, `download_limit_kib` and `upload_limit_kib` (KiB/s, 0 = no limit), limit all
  downloads together. They go to librqbit's session-wide limiter at start (`SessionOptions::
  ratelimits`) and can change while running (`session.ratelimits.set_*_bps`, via
  `Engine::set_speed_limits`); the window's Settings and `dinghy settings set download-limit /
  upload-limit` apply them at once (the CLI tells an open window with `SetSpeedLimits`).
- **The lowest limit is 32 KiB/s.** librqbit waits for the limiter one data block (16 KiB) at a
  time and fails outright for a block bigger than the limit's whole budget, which would cut every
  connection; `profile::check_speed_limit` refuses 1 to 31 (and more than 4,000,000).
- The window shows a totals line under "Saving to": "In total: downloading X, uploading Y" with
  the limits in force.
- A test of the source itself (`tests/source_text.rs`) fails when a message has a long gap of
  spaces, the sign of a lost `\` line join.

## Sharing limits (M10)

- Two settings: a finished download stops sharing at a **ratio** (`stop_sharing_ratio_percent`,
  100 = it has uploaded as much as its size) and/or after **minutes of sharing**
  (`stop_sharing_after_minutes`); 0 turns either off, and both are off unless chosen.
- librqbit forgets its upload counter at every start (and a "check files" re-adds the torrent,
  which starts the counter again), so the engine counts itself: once a second `Shared::account`
  adds what librqbit's counter grew by (per torrent id) to each download's total, adds sharing
  time while a finished download is live, and notes when it finished. The saved list
  (**version 4**) keeps `uploaded_bytes`, `shared_seconds`, `finished_at` and `sharing`; the
  engine saves changes every 15 seconds, at once when the sharing state changes, and when it
  shuts down.
- When a limit is reached the download is paused in librqbit and marked `StoppedByLimit`: the
  queue doesn't wake it, it starts paused after a restart, and it shows as Finished with
  "Stopped sharing: it reached the sharing limit (...)". **Resume** ("Share again" in the window)
  marks it `KeepSharing`: limits no longer apply to it. If the limits are raised or turned off,
  downloads they stopped share again within a second.
- These totals are the start of the history in M14.

## Port and UPnP (M11)

- Three settings, used when the engine starts (librqbit can't change its listener while
  running): `accept_incoming` (on), `listen_port` (0 = automatic: librqbit picks a different free
  port at each start) and `upnp` (off: BitDinghy doesn't change the router unless asked; when
  on, librqbit asks the router to forward the port).
- If the chosen port can't be used (usually another program has it), the engine starts again
  with an automatic port and shows a notice naming both ports, instead of not starting at all.
- The window's Settings show the port in use right now ("Right now: accepting connections on
  port N") and explain the Windows Firewall question that comes the first time.
- `EngineConfig::listen_ip` is every network normally; the tests listen on this computer's own
  address only, so running them never brings up a firewall question.
- `dinghy get` now follows the "accept connections" setting (`--no-listen` still turns it off
  for one run).

## Plain-language problems (M12)

- **`plain.rs`** turns the engine's technical problems into a plain sentence with what to do,
  keeping the technical text after "Details:" (useful when asking for help). It recognises a
  full disk, a file too big for the drive, no permission to write, a file another program holds,
  and a drive that went away, by the system's error number ("os error 112"; Windows and Linux
  numbers are kept apart because they differ) or its words. Unknown problems pass unchanged. It
  is applied to errors when a download starts and to errors librqbit reports later.
- **`drive.rs`** asks Windows what drive the download folder is on (its format) and how much
  room is free. Before a download starts, a file of 4 GB or more on a **FAT32** drive is refused
  with a message naming the drive, the file and its size (nothing is written); a **new**
  download bigger than the free room gets a warning (`DownloadSnapshot::warning`), shown under
  the download, by `dinghy list` and once by `dinghy get`. Other systems skip these checks.
- `dinghy get --no-dht` with no trackers in the link and no `--peer` says at once that it can't
  find anyone, instead of waiting forever. A paused download that `dinghy get` follows says
  "paused at N%" instead of looking stuck.

## Where each piece appears

| Milestone | What lands |
|---|---|
| M1 | Engine wrapper and the path guard |
| M3 | Database and the saved download list |
| M4 | The channel between the window and the command line |
| M5 | .torrent files and web links to them |
| M6 | Choosing files |
| M7 | Checking files, using files already there |
| M8 | The queue |
| M9 | Speed limits |
| M10 | Sharing limits |
| M11 | Port and UPnP |
| M12 | Plain-language problems, drive checks |
| M14 | History and charts |
