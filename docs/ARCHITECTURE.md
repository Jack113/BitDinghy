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
  channel that is not a network port, so other computers and web pages cannot reach it. With the
  window closed, `dinghy list` reads the saved list and `dinghy get <link>` downloads in the
  terminal. Letting the window attach to a command-line engine is on the TODO list.
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

## Where each piece appears

| Milestone | What lands |
|---|---|
| M1 | Engine wrapper and the path guard |
| M3 | Database and the saved download list |
| M4 | The channel between the window and the command line |
| M8-M10 | Queue and limits |
| M14 | History and charts |
