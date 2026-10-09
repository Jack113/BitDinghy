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
          librqbit        settings.toml + dinghy.db
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

## Where each piece appears

| Milestone | What lands |
|---|---|
| M1 | Engine wrapper and the path guard |
| M3 | Database and the saved download list |
| M4 | The channel between the window and the command line |
| M8-M10 | Queue and limits |
| M14 | History and charts |
