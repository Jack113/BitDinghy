//! The download engine: BitDinghy's wrapper around librqbit.
//!
//! This is the ONLY file that talks to the BitTorrent engine. Everything else in the project
//! (the window, the command line) sees only the plain types defined here, so upgrading or
//! replacing the engine later means changing this file and nothing else.
//!
//! How a download starts (see `start_download`):
//! 1. Ask the engine for the torrent's file list ONLY. This creates nothing on disk.
//! 2. Run the file-name guard (`safety`) over that list. Anything unsafe stops here.
//! 3. Only then start the real download, reusing the file list we already have.
//!
//! Every download is also remembered in the saved list (`saved_list.rs`): as soon as it is added,
//! and again with its file list once that has passed the guard. When the engine starts, it can
//! bring back everything in the list through the same three steps, so a remembered download is
//! checked exactly like a new one.
//!
//! Every file is downloaded unless the person unticked it (`file_choice.rs`). The choice can change
//! at any time; it is passed to the engine at step 3, and straight away while a download runs.
//! The engine still creates unticked files on disk, so BitDinghy marks them hidden once the engine
//! has set a download up, and again after every change (see `owned_files.rs`).
//!
//! **The queue** (M8): at most `max_active_downloads` downloads run at once (0: no limit). The
//! others wait, in list order, as "queued": not started yet (`EntryState::Waiting`), or paused in
//! librqbit by the queue (`Entry::held`). Downloads that only share (finished), are paused by the
//! person, or failed don't take a place. [`Shared::rebalance`] decides, after every change and once
//! a second (so a download that finishes lets the next one start).
//!
//! **Sharing limits** (M10): librqbit forgets how much a torrent uploaded at every start, so the
//! engine counts uploads and sharing time itself, once a second ([`Shared::account`]), adds them to
//! what the saved list remembers, and saves them every so often and when it shuts down. A finished
//! download whose ratio or sharing time reaches the limit in the settings is paused in librqbit
//! and marked "stopped by a limit" (it stays stopped after a restart); "Resume" shares it again,
//! and limits then no longer apply to it.
//!
//! **History** (M14): the same once-a-second count adds what was downloaded (verified pieces) and
//! uploaded to today's totals (saved with the other counts), and keeps the total speeds of the
//! last ten minutes for the window's chart. Removing a download puts it in the history.
//!
//! **Recommended settings** (M15): from the same speeds, the engine remembers the fastest
//! 10-second averages seen in each direction (only while that direction has no limit) and keeps
//! them in the saved list; `recommend.rs` turns them into suggestions.
//!
//! **Checking files** (M7): librqbit has no "check again" call, but it checks every piece already
//! on disk whenever a torrent is added. So [`Engine::check`] takes the download out of librqbit
//! (keeping its files) and adds it again from its saved file list, then says what it found.

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use librqbit::dht::DhtPersistenceConfig;
use librqbit::limits::LimitsConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, DhtSessionConfig, ListOnlyResponse,
    ListenerOptions, ManagedTorrent, Session, SessionOptions, TorrentStatsState,
};
use serde::{Deserialize, Serialize};

use crate::days;
use crate::display::format_bytes;
use crate::drive;
use crate::error::{Error, Result};
use crate::file_choice::{self, FileChoice, TorrentFile};
use crate::input::{self, Input};
use crate::magnet;
use crate::owned_files::{self, Deleted};
use crate::plain;
use crate::recommend::{self, Fastest};
use crate::safety::{self, Platform};
use crate::saved_list::{DayTotal, HistoryEntry, SavedDownload, SavedList, Sharing};
use crate::torrent_file::{self, CheckedTorrent};
use crate::web_link;

/// Identifies one download for as long as the program runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DownloadId(pub u64);

/// How the engine should start. `EngineConfig::new` gives the normal everyday settings.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Where new downloads are saved. Nothing is ever written outside a download's folder.
    pub download_dir: PathBuf,
    /// Look for people sharing a torrent through the DHT (a shared "phone book" of peers).
    pub use_dht: bool,
    /// Accept connections from other people (needed to share files back).
    pub accept_incoming: bool,
    /// Find other BitTorrent programs on the local network.
    pub local_discovery: bool,
    /// Where the DHT keeps its saved contacts between runs, so the next start finds people faster.
    /// It must be inside BitDinghy's own data folder. `None` means the contacts are not saved.
    pub dht_state_file: Option<PathBuf>,
    /// The saved download list (inside BitDinghy's data folder). `None` means downloads are not
    /// remembered. While an engine has the list open, no other engine can use it.
    pub saved_list_file: Option<PathBuf>,
    /// Bring back every download in the saved list when starting. The window does; `dinghy get`
    /// doesn't, because it only wants the one download it was asked for.
    pub restore_saved: bool,
    /// Also open plain `http://` links to `.torrent` files (the setting; https always works).
    pub allow_http_links: bool,
    /// How many downloads run at once; the others wait in the queue. 0: no limit (M8).
    pub max_active_downloads: usize,
    /// The most all downloads together may download and upload, in KiB/s. 0: no limit (M9).
    pub download_limit_kib: u32,
    pub upload_limit_kib: u32,
    /// A finished download stops sharing at this ratio, in percent (100 = 1.0), or after sharing
    /// this many minutes. 0: no such limit (M10).
    pub stop_sharing_ratio_percent: u32,
    pub stop_sharing_after_minutes: u32,
    /// The port other people connect to (0: automatic, a different one at each start), and
    /// whether to ask the router to forward it (UPnP) (M11). Only with `accept_incoming`.
    pub listen_port: u16,
    pub upnp: bool,
    /// The address to listen on. Every network (the standard); tests use this computer only, so
    /// no firewall question pops up while they run.
    pub listen_ip: IpAddr,
}

impl EngineConfig {
    pub fn new(download_dir: impl Into<PathBuf>) -> Self {
        Self {
            download_dir: download_dir.into(),
            use_dht: true,
            accept_incoming: true,
            local_discovery: true,
            dht_state_file: None,
            saved_list_file: None,
            restore_saved: true,
            allow_http_links: false,
            max_active_downloads: crate::settings::DEFAULT_MAX_ACTIVE_DOWNLOADS as usize,
            download_limit_kib: 0,
            upload_limit_kib: 0,
            stop_sharing_ratio_percent: 0,
            stop_sharing_after_minutes: 0,
            listen_port: 0,
            upnp: false,
            listen_ip: IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        }
    }
}

/// Where a download is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadStatus {
    /// Still asking other people for the torrent's file list (a magnet link starts like this).
    GettingInfo,
    /// Checking files that are already on disk.
    Checking,
    Downloading,
    /// All files arrived and were verified. The torrent keeps sharing until the program closes.
    Finished,
    Paused,
    Failed,
    /// Waiting in the queue for another download to finish (M8).
    Queued,
}

/// A picture of one download at one moment. The window and the command line redraw from these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadSnapshot {
    pub id: DownloadId,
    /// The torrent's fingerprint (40 lowercase hex characters).
    pub info_hash: String,
    /// The torrent's name once known, otherwise what the magnet link called it.
    pub name: String,
    pub status: DownloadStatus,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub download_bytes_per_second: u64,
    pub upload_bytes_per_second: u64,
    /// Estimated time left. `None` when we can't tell yet.
    pub eta: Option<Duration>,
    /// People we are connected to right now.
    pub peers_connected: u32,
    /// People we know about (connected or not).
    pub peers_known: u32,
    /// Bytes shared with others, over all runs.
    pub uploaded_bytes: u64,
    /// How long it has shared after finishing, over all runs, in seconds (M10).
    #[serde(default)]
    pub shared_seconds: u64,
    /// It finished and a sharing limit was reached, so it no longer shares (M10).
    #[serde(default)]
    pub sharing_stopped: bool,
    /// How long we have been waiting for the file list (only while `GettingInfo`).
    pub waiting_for: Option<Duration>,
    /// The folder the files are saved in, once known.
    pub save_folder: Option<PathBuf>,
    /// A plain-language problem message when `status` is `Failed`.
    pub error: Option<String>,
    /// How many files the torrent has, once its file list is known (0 until then).
    #[serde(default)]
    pub files_total: usize,
    /// How many of them are chosen (ticked). The progress counts only these.
    #[serde(default)]
    pub files_chosen: usize,
    /// What the last "check files" found, once it has finished (M7).
    #[serde(default)]
    pub note: Option<String>,
    /// It failed because a file with the same name was already there; the person can choose to
    /// use that file instead ([`Engine::use_existing_files`]).
    #[serde(default)]
    pub can_use_existing_files: bool,
    /// Something to know that isn't a failure, such as "the drive may be too full" (M12).
    #[serde(default)]
    pub warning: Option<String>,
}

/// Choices for adding a download.
#[derive(Debug, Clone, Default)]
pub struct AddOptions {
    /// Extra addresses to try directly (handy on your own network, and in tests).
    pub peers: Vec<SocketAddr>,
    /// Use files with the same names that are already in the download folder, instead of refusing
    /// the download. They are checked first and completed; afterwards they count as this
    /// download's files ("remove, delete the files too" deletes them).
    pub use_existing_files: bool,
}

impl AddOptions {
    fn with_peers(peers: Vec<SocketAddr>) -> Self {
        Self {
            peers,
            ..Self::default()
        }
    }
}

/// Where to move a download in the list (the queue order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Move {
    Up,
    Down,
    Top,
    Bottom,
}

enum EntryState {
    /// Asking other people for the file list, since this moment.
    GettingInfo(Instant),
    Active(Arc<ManagedTorrent>),
    /// Paused before it really started (its file list isn't known yet): nothing is running for
    /// it, and nobody is contacted. Resuming starts it.
    Stopped,
    /// Waiting in the queue to start (nothing runs for it yet).
    Waiting,
    Failed(String),
}

struct Entry {
    id: DownloadId,
    info_hash: String,
    label: String,
    /// The download folder this download belongs to (it keeps it even if the setting changes).
    download_dir: PathBuf,
    /// The cleaned-up magnet link (empty if the saved one was broken). Used to try again.
    magnet: String,
    /// Whether the person wants it paused. This can be set before the torrent has really started
    /// (while its file list is still being fetched); it then starts paused.
    wants_pause: AtomicBool,
    /// Extra addresses to try directly (`dinghy get --peer`), kept for trying again.
    peers: Vec<SocketAddr>,
    /// The checked `.torrent` it was added from, if it was (kept for trying again).
    known_torrent: Option<Vec<u8>>,
    /// Set when the person removes it, so a start that is still on its way doesn't go ahead.
    removed: AtomicBool,
    /// Paused in librqbit by the queue (not by the person): it carries on when its turn comes.
    held: AtomicBool,
    /// Its checked file list (a `.torrent` file) and the folder its files are in, once known.
    /// This is what "remove and delete files" uses.
    files: Mutex<Option<(Vec<u8>, PathBuf)>>,
    /// The same file list, read for choosing files (see `file_choice.rs`), once known.
    file_list: Mutex<Option<Arc<Vec<TorrentFile>>>>,
    /// The files the person unticked (the torrent's own indexes). Empty: every file.
    skipped: Mutex<BTreeSet<usize>>,
    /// The person chose to use files already in the folder (see [`AddOptions`]).
    use_existing: AtomicBool,
    /// The last start failed because a file with the same name was already there.
    name_clash: AtomicBool,
    /// A "check files" is under way: say what it found once librqbit has checked.
    check_requested: AtomicBool,
    /// Added only to be checked (`dinghy check` with the window closed): it is paused just here.
    check_only: AtomicBool,
    /// What the last check found.
    note: Mutex<Option<String>>,
    /// Something to know that isn't a failure (see [`DownloadSnapshot::warning`]).
    warning: Mutex<Option<String>>,
    /// Uploads and sharing time, over all runs (see [`Shared::account`]).
    counts: Mutex<Counts>,
    state: Mutex<EntryState>,
    /// Smooths the download speed shown to the user (see [`Smoother`]).
    speed: Mutex<Smoother>,
}

/// What a download has shared, kept up to date once a second and saved every so often.
#[derive(Debug, Clone, Default)]
struct Counts {
    uploaded_bytes: u64,
    /// In milliseconds, so the once-a-second ticks add up exactly.
    shared_millis: u64,
    finished_at: Option<u64>,
    sharing: Sharing,
    /// librqbit's upload counter as last seen, and which torrent it belonged to (a check adds the
    /// torrent again, and the new one counts from zero).
    last_seen: Option<(usize, u64)>,
    /// The same for librqbit's "fetched from people" counter, to count each day's downloads (M14).
    last_fetched: Option<(usize, u64)>,
    /// Changed since it was last saved.
    unsaved: bool,
}

impl Counts {
    fn from_saved(saved: &SavedDownload) -> Self {
        Self {
            uploaded_bytes: saved.uploaded_bytes,
            shared_millis: saved.shared_seconds.saturating_mul(1000),
            finished_at: saved.finished_at,
            sharing: saved.sharing,
            last_seen: None,
            last_fetched: None,
            unsaved: false,
        }
    }
}

/// What a new download starts with (see the fields of [`Entry`]).
struct NewEntry {
    info_hash: String,
    label: String,
    download_dir: PathBuf,
    magnet: String,
    paused: bool,
    peers: Vec<SocketAddr>,
    known_torrent: Option<Vec<u8>>,
    skipped: BTreeSet<usize>,
    use_existing: bool,
    counts: Counts,
}

/// Where a download's file list comes from.
enum Source {
    /// A (cleaned-up) magnet link: the file list must be fetched from other people.
    Magnet(String),
    /// A file list we already have (saved earlier, after it passed the guard), and the folder the
    /// download was started in. A continuing download ALWAYS stays in that folder.
    Torrent {
        bytes: Vec<u8>,
        folder: Option<PathBuf>,
    },
}

impl Source {
    /// Continue a saved download from its saved file list, if it has one.
    fn saved(saved: SavedDownload) -> Option<Source> {
        Some(Source::Torrent {
            bytes: saved.torrent?,
            folder: saved.output_folder,
        })
    }
}

/// Smooths a jumpy number over a few seconds.
///
/// A torrent's instantaneous speed bounces around (one second 89 MiB/s, the next 59 KiB/s), which
/// makes a "time left" estimate swing wildly. This is an exponential moving average: each new
/// reading nudges the value part of the way, based on how much TIME passed, so it behaves the same
/// whether the screen redraws twice or sixty times a second.
struct Smoother {
    value: f64,
    last_update: Option<Instant>,
}

impl Smoother {
    /// How many seconds it takes to move about two thirds of the way to a new steady speed.
    const TIME_CONSTANT_SECONDS: f64 = 3.0;

    fn new() -> Self {
        Self {
            value: 0.0,
            last_update: None,
        }
    }

    fn update(&mut self, reading: f64, now: Instant) -> f64 {
        match self.last_update {
            None => self.value = reading,
            Some(last) => {
                let seconds = now.saturating_duration_since(last).as_secs_f64();
                let weight = 1.0 - (-seconds / Self::TIME_CONSTANT_SECONDS).exp();
                self.value += weight * (reading - self.value);
            }
        }
        self.last_update = Some(now);
        if self.value < 1.0 {
            self.value = 0.0; // Settle at exactly zero instead of trailing off forever.
        }
        self.value
    }
}

/// Locks a mutex, carrying on even if another thread panicked while holding it (the data here
/// is simple state that stays valid).
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What the engine and its background tasks share.
struct Shared {
    session: Arc<Session>,
    /// The folder the session was started with. librqbit places every torrent relative to it; we
    /// then move that place into each download's own folder (see `vet_listing`).
    session_dir: PathBuf,
    /// The saved list. `None` when downloads aren't remembered (no data folder, a problem with the
    /// list, or after shutdown, which closes it so the next BitDinghy can open it).
    saved: Mutex<Option<SavedList>>,
    /// Messages for the person, such as "your download list was damaged".
    notices: Mutex<Vec<String>>,
    /// Every download, in list (queue) order.
    entries: Mutex<Vec<Arc<Entry>>>,
    runtime: tokio::runtime::Handle,
    /// How many downloads may run at once (0: no limit).
    max_active: AtomicUsize,
    /// Only one queue decision at a time.
    rebalancing: Mutex<()>,
    /// Set by `shutdown`: background work stops.
    stopped: AtomicBool,
    /// The sharing limits (M10): ratio in percent, and minutes. 0: no such limit.
    sharing_ratio_percent: AtomicU32,
    sharing_minutes: AtomicU32,
    /// What was downloaded and uploaded today and not saved yet (M14).
    tally: Mutex<DayTally>,
    /// The total speeds (download, upload) of the last ten minutes, one a second, oldest first.
    speeds: Mutex<VecDeque<(u64, u64)>>,
    /// The fastest 10-second averages seen (M15), as remembered in the saved list.
    fastest: Mutex<Fastest>,
}

/// How many seconds a speed is averaged over before it counts as "seen".
const FASTEST_AVERAGE_SECONDS: usize = 10;

/// Today's not-yet-saved totals.
#[derive(Debug, Clone, Copy, Default)]
struct DayTally {
    day: i64,
    downloaded: u64,
    uploaded: u64,
}

/// How many seconds of speeds the chart keeps.
const SPEED_HISTORY_SECONDS: usize = 600;

impl Shared {
    /// Runs `action` on the saved list, if there is one. A failure becomes a notice (shown once)
    /// rather than stopping the download: the file on disk matters more than the list entry.
    fn with_list<T>(&self, action: impl FnOnce(&SavedList) -> Result<T>) -> Option<T> {
        let saved = lock(&self.saved);
        let list = saved.as_ref()?;
        match action(list) {
            Ok(value) => Some(value),
            Err(error) => {
                drop(saved);
                self.notice(error.to_string());
                None
            }
        }
    }

    fn spawn_start(self: &Arc<Self>, entry: Arc<Entry>, source: Source) {
        let shared = self.clone();
        self.runtime.spawn(async move {
            match start_download(&shared, &entry, source).await {
                // Removed while it was starting: take it straight back out of the engine.
                Ok(handle) if entry.removed.load(Ordering::SeqCst) => {
                    let _ = shared.session.delete(handle.id().into(), false).await;
                }
                Ok(handle) => {
                    *lock(&entry.state) = EntryState::Active(handle.clone());
                    // A pause or resume may have arrived while the download was starting. Reading
                    // the wish only AFTER publishing the handle means that either this line sees
                    // it, or `pause`/`resume` sees the handle: one of the two always acts on it.
                    let wants_pause = entry.wants_pause.load(Ordering::SeqCst)
                        || entry.held.load(Ordering::SeqCst)
                        || lock(&entry.counts).sharing == Sharing::StoppedByLimit;
                    if wants_pause && !handle.is_paused() {
                        let _ = shared.session.pause(&handle).await;
                    } else if !wants_pause && handle.is_paused() {
                        let _ = shared.session.unpause(&handle).await;
                    }
                    // The engine creates the files while it sets the download up: hide the
                    // leftovers of unticked files once it has. A "check files" reports here too.
                    if handle.wait_until_initialized().await.is_ok()
                        && !entry.removed.load(Ordering::SeqCst)
                    {
                        if entry.check_requested.swap(false, Ordering::SeqCst) {
                            let check_only = entry.check_only.load(Ordering::SeqCst);
                            *lock(&entry.note) = Some(check_note(&handle, check_only));
                        }
                        let _ = tokio::task::spawn_blocking(move || mark_leftovers(&entry)).await;
                    }
                }
                Err(error) => {
                    entry.check_requested.store(false, Ordering::SeqCst);
                    entry
                        .name_clash
                        .store(matches!(error, Error::NameClash(_)), Ordering::SeqCst);
                    let mut state = lock(&entry.state);
                    // Paused in the meantime: the person doesn't want it running anyway, so it
                    // simply waits to be resumed (which tries again).
                    *state = if entry.wants_pause.load(Ordering::SeqCst) {
                        EntryState::Stopped
                    } else {
                        EntryState::Failed(error.to_string())
                    };
                }
            }
        });
    }

    /// Starts a download that isn't running (stopped or failed) again: from the saved file list
    /// if there is one, else from the `.torrent` it was added from, else from its magnet link.
    fn start_again(self: &Arc<Self>, entry: Arc<Entry>) {
        entry.name_clash.store(false, Ordering::SeqCst);
        let source = self
            .with_list(|list| list.get(&entry.info_hash))
            .flatten()
            .and_then(Source::saved)
            .unwrap_or_else(|| match &entry.known_torrent {
                Some(bytes) => Source::Torrent {
                    bytes: bytes.clone(),
                    folder: None,
                },
                None => Source::Magnet(entry.magnet.clone()),
            });
        self.spawn_start(entry, source);
    }

    /// The queue (see the module notes): decides which downloads may run, in list order, and
    /// starts, holds back or lets go of them.
    fn rebalance(self: &Arc<Self>) {
        let _one_at_a_time = lock(&self.rebalancing);
        let limit = self.max_active.load(Ordering::SeqCst);
        let entries = lock(&self.entries).clone();
        // Places already taken by downloads that can't be held back: fetching a file list, or
        // checking files.
        let mut running = 0;
        let mut wanting = Vec::new();
        for entry in entries {
            if entry.removed.load(Ordering::SeqCst) || entry.wants_pause.load(Ordering::SeqCst) {
                entry.held.store(false, Ordering::SeqCst);
                continue;
            }
            let handle = match &*lock(&entry.state) {
                EntryState::GettingInfo(_) => {
                    running += 1;
                    continue;
                }
                EntryState::Waiting => None,
                EntryState::Active(handle) => Some(handle.clone()),
                EntryState::Stopped | EntryState::Failed(_) => continue,
            };
            if let Some(handle) = &handle {
                let stats = handle.stats();
                match stats.state {
                    TorrentStatsState::Initializing { .. } => {
                        // Paused while checking: librqbit stopped the check. Not paused by the
                        // person (any more), nor stopped by a sharing limit: check again.
                        let stopped = lock(&entry.counts).sharing == Sharing::StoppedByLimit;
                        if handle.is_paused() && !stopped {
                            self.unpause_later(handle.clone());
                        }
                        running += 1;
                        continue;
                    }
                    TorrentStatsState::Error => continue,
                    // Only sharing: it takes no place, and (not paused by the person, nor stopped by
                    // a sharing limit) it shares.
                    _ if stats.finished => {
                        entry.held.store(false, Ordering::SeqCst);
                        let stopped = lock(&entry.counts).sharing == Sharing::StoppedByLimit;
                        if handle.is_paused() && !stopped {
                            self.unpause_later(handle.clone());
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            wanting.push((entry, handle));
        }

        for (entry, handle) in wanting {
            if limit == 0 || running < limit {
                running += 1;
                entry.held.store(false, Ordering::SeqCst);
                match handle {
                    None => {
                        let mut state = lock(&entry.state);
                        if matches!(*state, EntryState::Waiting) {
                            *state = EntryState::GettingInfo(Instant::now());
                            drop(state);
                            self.start_again(entry);
                        }
                    }
                    Some(handle) if handle.is_paused() => self.unpause_later(handle),
                    Some(_) => {}
                }
            } else {
                entry.held.store(true, Ordering::SeqCst);
                if let Some(handle) = handle
                    && !handle.is_paused()
                {
                    let shared = self.clone();
                    self.runtime.spawn(async move {
                        let _ = shared.session.pause(&handle).await;
                    });
                }
            }
        }
    }

    fn unpause_later(self: &Arc<Self>, handle: Arc<ManagedTorrent>) {
        let shared = self.clone();
        self.runtime.spawn(async move {
            let _ = shared.session.unpause(&handle).await;
        });
    }

    /// Counts uploads and sharing time (once a second, `elapsed` since the last time), and applies
    /// the sharing limits (see the module notes).
    fn account(self: &Arc<Self>, elapsed: Duration) {
        let ratio_percent = u64::from(self.sharing_ratio_percent.load(Ordering::SeqCst));
        let minutes = u64::from(self.sharing_minutes.load(Ordering::SeqCst));
        let entries = lock(&self.entries).clone();
        let (mut downloaded, mut uploaded) = (0u64, 0u64);
        let (mut download_speed, mut upload_speed) = (0u64, 0u64);
        for entry in entries {
            let handle = match &*lock(&entry.state) {
                EntryState::Active(handle) => handle.clone(),
                _ => continue,
            };
            let stats = handle.stats();
            let mut counts = lock(&entry.counts);

            // Uploads: what librqbit counted since we last looked.
            let torrent = handle.id();
            let added = match counts.last_seen {
                Some((seen, before)) if seen == torrent && stats.uploaded_bytes >= before => {
                    stats.uploaded_bytes - before
                }
                // A new torrent (first look, or added again by a check) counts from zero.
                _ => stats.uploaded_bytes,
            };
            counts.last_seen = Some((torrent, stats.uploaded_bytes));
            if added > 0 {
                counts.uploaded_bytes = counts.uploaded_bytes.saturating_add(added);
                counts.unsaved = true;
                uploaded = uploaded.saturating_add(added);
            }
            // Downloads: everything fetched from people (what used the person's data), counted
            // like uploads. Files found already on disk are never counted.
            if let Some(live) = &stats.live {
                let fetched = live.snapshot.fetched_bytes;
                let new = match counts.last_fetched {
                    Some((seen, before)) if seen == torrent && fetched >= before => {
                        fetched - before
                    }
                    _ => fetched,
                };
                counts.last_fetched = Some((torrent, fetched));
                downloaded = downloaded.saturating_add(new);
                download_speed = download_speed.saturating_add(live.download_speed.as_bytes());
                upload_speed = upload_speed.saturating_add(live.upload_speed.as_bytes());
            }

            if !stats.finished || matches!(stats.state, TorrentStatsState::Initializing { .. }) {
                continue;
            }
            if counts.finished_at.is_none() {
                counts.finished_at = Some(now_seconds());
                counts.unsaved = true;
            }
            let sharing_now = matches!(stats.state, TorrentStatsState::Live);
            if sharing_now {
                counts.shared_millis = counts
                    .shared_millis
                    .saturating_add(elapsed.as_millis() as u64);
                counts.unsaved = true;
            }

            let reached = sharing_limit_reached(&counts, stats.total_bytes, ratio_percent, minutes);
            match counts.sharing {
                Sharing::AsUsual if reached && sharing_now => {
                    counts.sharing = Sharing::StoppedByLimit;
                    counts.unsaved = true;
                    drop(counts);
                    self.save_entry_counts(&entry);
                    let shared = self.clone();
                    self.runtime.spawn(async move {
                        let _ = shared.session.pause(&handle).await;
                    });
                }
                // The limits changed (or were turned off): it may share again.
                Sharing::StoppedByLimit if !reached => {
                    counts.sharing = Sharing::AsUsual;
                    counts.unsaved = true;
                    drop(counts);
                    self.save_entry_counts(&entry);
                }
                _ => {}
            }
        }

        // Today's totals: a new day saves the old one first.
        let today = days::today();
        if lock(&self.tally).day != today {
            self.save_tally();
            lock(&self.tally).day = today;
        }
        {
            let mut tally = lock(&self.tally);
            tally.downloaded = tally.downloaded.saturating_add(downloaded);
            tally.uploaded = tally.uploaded.saturating_add(uploaded);
        }
        if !elapsed.is_zero() {
            let mut speeds = lock(&self.speeds);
            if speeds.len() == SPEED_HISTORY_SECONDS {
                speeds.pop_front();
            }
            speeds.push_back((download_speed, upload_speed));
            // The last 10 seconds' averages: the fastest seen, while nothing limited them.
            if speeds.len() >= FASTEST_AVERAGE_SECONDS {
                let recent = speeds.iter().rev().take(FASTEST_AVERAGE_SECONDS);
                let (down, up) = recent.fold((0u64, 0u64), |(d, u), (sd, su)| (d + sd, u + su));
                let average = |sum: u64| sum / FASTEST_AVERAGE_SECONDS as u64;
                drop(speeds);
                self.learn(average(down), average(up));
            }
        }
    }

    /// Remembers new fastest speeds (see [`Shared::account`]); only in a direction without a limit,
    /// and only when clearly faster (so the saved list isn't written every second).
    fn learn(&self, download: u64, upload: u64) {
        let limits = &self.session.ratelimits;
        let faster =
            |seen: Option<u64>, now: u64| now > seen.unwrap_or(0).saturating_mul(102) / 100;
        let mut fastest = lock(&self.fastest);
        let mut save = Vec::new();
        if limits.get_download_bps().is_none()
            && download >= recommend::ENOUGH_DOWNLOAD
            && faster(fastest.download, download)
        {
            fastest.download = Some(download);
            save.push((recommend::FASTEST_DOWNLOAD, download));
        }
        if limits.get_upload_bps().is_none()
            && upload >= recommend::ENOUGH_UPLOAD
            && faster(fastest.upload, upload)
        {
            fastest.upload = Some(upload);
            save.push((recommend::FASTEST_UPLOAD, upload));
        }
        drop(fastest);
        for (name, value) in save {
            self.with_list(|list| list.set_learned(name, value));
        }
    }

    /// Adds today's unsaved totals to the saved list.
    fn save_tally(&self) {
        let tally = std::mem::replace(
            &mut *lock(&self.tally),
            DayTally {
                day: days::today(),
                ..DayTally::default()
            },
        );
        if tally.downloaded > 0 || tally.uploaded > 0 {
            self.with_list(|list| list.add_to_day(tally.day, tally.downloaded, tally.uploaded));
        }
    }

    /// Saves the counts that changed since they were last saved, and today's totals.
    fn save_counts(&self) {
        self.save_tally();
        let entries = lock(&self.entries).clone();
        for entry in entries {
            if lock(&entry.counts).unsaved {
                self.save_entry_counts(&entry);
            }
        }
    }

    fn save_entry_counts(&self, entry: &Entry) {
        let counts = {
            let mut counts = lock(&entry.counts);
            counts.unsaved = false;
            counts.clone()
        };
        self.with_list(|list| {
            list.record_sharing(
                &entry.info_hash,
                counts.uploaded_bytes,
                counts.shared_millis / 1000,
                counts.finished_at,
                counts.sharing,
            )
        });
    }

    fn notice(&self, message: String) {
        let mut notices = lock(&self.notices);
        if !notices.contains(&message) {
            notices.push(message);
        }
    }
}

/// The download engine. Create one with [`Engine::start`] and keep it for the whole run.
pub struct Engine {
    shared: Arc<Shared>,
    /// The "allow plain http links" setting; the window changes it while running.
    allow_http_links: AtomicBool,
    download_dir: PathBuf,
    next_id: AtomicU64,
}

impl Engine {
    /// Starts the engine. Must be called from inside a tokio runtime.
    ///
    /// Fails with [`Error::AlreadyRunning`] if another BitDinghy already has the saved list open:
    /// two engines would download into the same files.
    pub async fn start(config: EngineConfig) -> Result<Engine> {
        let download_dir =
            std::path::absolute(&config.download_dir).map_err(|e| Error::DownloadDir {
                path: config.download_dir.clone(),
                reason: e.to_string(),
            })?;
        std::fs::create_dir_all(&download_dir).map_err(|e| Error::DownloadDir {
            path: download_dir.clone(),
            reason: e.to_string(),
        })?;

        // The saved list first: if another BitDinghy is running, stop before touching anything.
        let mut notices = Vec::new();
        let saved = match &config.saved_list_file {
            None => None,
            Some(path) => match SavedList::open(path) {
                Ok(opened) => {
                    notices.extend(opened.problem.map(|problem| problem.message()));
                    Some(opened.list)
                }
                Err(Error::AlreadyRunning) => return Err(Error::AlreadyRunning),
                Err(other) => {
                    notices.push(format!(
                        "{other} Downloads you add now won't be remembered after BitDinghy closes."
                    ));
                    None
                }
            },
        };

        let listener = |port: u16| ListenerOptions {
            listen_addr: SocketAddr::new(config.listen_ip, port),
            enable_upnp_port_forwarding: config.upnp,
            ..Default::default()
        };
        let options = |listen: Option<ListenerOptions>| SessionOptions {
            // By default the DHT would remember its contacts in a file in the user's profile.
            // BitDinghy must not write anywhere we didn't choose, so we either point it at a file
            // inside our own data folder, or (when none is given) turn remembering off.
            dht: config.use_dht.then(|| DhtSessionConfig {
                persistence: config
                    .dht_state_file
                    .clone()
                    .map(|file| DhtPersistenceConfig {
                        dump_interval: None,
                        config_filename: Some(file),
                    }),
                ..Default::default()
            }),
            // librqbit's own saved list stays off: BitDinghy keeps its own (see `saved_list.rs`).
            persistence: None,
            fastresume: false,
            listen,
            disable_local_service_discovery: !config.local_discovery,
            client_name_and_version: Some(format!("{} {}", crate::APP_NAME, crate::VERSION)),
            ratelimits: LimitsConfig {
                download_bps: bytes_per_second(config.download_limit_kib),
                upload_bps: bytes_per_second(config.upload_limit_kib),
            },
            ..Default::default()
        };

        let wanted = config.accept_incoming.then(|| listener(config.listen_port));
        let session = match Session::new_with_opts(download_dir.clone(), options(wanted)).await {
            Ok(session) => session,
            // The chosen port is probably taken by another program: carry on with a free one
            // rather than not starting at all, and say so.
            Err(error) if config.accept_incoming && config.listen_port != 0 => {
                let session =
                    Session::new_with_opts(download_dir.clone(), options(Some(listener(0))))
                        .await
                        .map_err(|e| Error::EngineStart(format!("{e:#}")))?;
                let now = session.listen_addr().map(|a| a.port()).unwrap_or_default();
                notices.push(format!(
                    "BitDinghy couldn't use port {} for incoming connections ({error:#}), so it \
                     uses port {now} this time. Another program may be using that port; choose \
                     another in Settings.",
                    config.listen_port
                ));
                session
            }
            Err(error) => return Err(Error::EngineStart(format!("{error:#}"))),
        };

        let shared = Arc::new(Shared {
            session,
            session_dir: download_dir.clone(),
            saved: Mutex::new(saved),
            notices: Mutex::new(notices),
            entries: Mutex::new(Vec::new()),
            runtime: tokio::runtime::Handle::current(),
            max_active: AtomicUsize::new(config.max_active_downloads),
            rebalancing: Mutex::new(()),
            stopped: AtomicBool::new(false),
            sharing_ratio_percent: AtomicU32::new(config.stop_sharing_ratio_percent),
            sharing_minutes: AtomicU32::new(config.stop_sharing_after_minutes),
            tally: Mutex::new(DayTally {
                day: days::today(),
                ..DayTally::default()
            }),
            speeds: Mutex::new(VecDeque::with_capacity(SPEED_HISTORY_SECONDS)),
            fastest: Mutex::new(Fastest::default()),
        });
        // What earlier runs learned.
        let learned = |name| shared.with_list(|list| list.learned(name)).flatten();
        *lock(&shared.fastest) = Fastest {
            download: learned(recommend::FASTEST_DOWNLOAD),
            upload: learned(recommend::FASTEST_UPLOAD),
        };
        // Once a second, let the queue move on (a download that finished frees its place).
        let weak = Arc::downgrade(&shared);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(1));
            let mut last = Instant::now();
            let mut ticks: u64 = 0;
            loop {
                ticker.tick().await;
                let Some(shared) = weak.upgrade() else {
                    break;
                };
                if shared.stopped.load(Ordering::SeqCst) {
                    break;
                }
                let now = Instant::now();
                shared.account(now.saturating_duration_since(last));
                last = now;
                ticks += 1;
                // The counts are saved every 15 seconds (and at once when a download's sharing
                // state changes, and when the engine shuts down).
                if ticks.is_multiple_of(15) {
                    shared.save_counts();
                }
                shared.rebalance();
            }
        });
        let engine = Engine {
            allow_http_links: AtomicBool::new(config.allow_http_links),
            shared,
            download_dir,
            next_id: AtomicU64::new(1),
        };
        if config.restore_saved {
            engine.restore_saved();
        }
        Ok(engine)
    }

    /// Changes the speed limits for all downloads together, in KiB/s (0: no limit). Applies
    /// straight away. (Check them first with `profile::check_speed_limit`.)
    pub fn set_speed_limits(&self, download_kib: u32, upload_kib: u32) {
        let limits = &self.shared.session.ratelimits;
        limits.set_download_bps(bytes_per_second(download_kib));
        limits.set_upload_bps(bytes_per_second(upload_kib));
    }

    /// The speed limits in force, in KiB/s (0: no limit): (download, upload).
    pub fn speed_limits(&self) -> (u32, u32) {
        let limits = &self.shared.session.ratelimits;
        let kib = |bps: Option<NonZeroU32>| bps.map_or(0, |bps| bps.get() / 1024);
        (kib(limits.get_download_bps()), kib(limits.get_upload_bps()))
    }

    /// Changes the sharing limits: a ratio in percent (100 = 1.0) and minutes of sharing; 0 means no
    /// such limit. Applies within a second, also to downloads a limit stopped earlier (they share
    /// again if the new limits allow it).
    pub fn set_sharing_limits(&self, ratio_percent: u32, minutes: u32) {
        self.shared
            .sharing_ratio_percent
            .store(ratio_percent, Ordering::SeqCst);
        self.shared.sharing_minutes.store(minutes, Ordering::SeqCst);
    }

    /// The fastest download and upload speeds seen (10-second averages, bytes per second), over all
    /// runs (M15).
    pub fn fastest_seen(&self) -> Fastest {
        *lock(&self.shared.fastest)
    }

    /// The total download and upload speeds of the last ten minutes, one a second, oldest first.
    pub fn speed_history(&self) -> Vec<(u64, u64)> {
        lock(&self.shared.speeds).iter().copied().collect()
    }

    /// Removed downloads, newest first (at most `limit`).
    pub fn history(&self, limit: usize) -> Vec<HistoryEntry> {
        self.shared
            .with_list(|list| list.history(limit))
            .unwrap_or_default()
    }

    /// What was downloaded and uploaded on each of the last `days` days (today included), oldest
    /// first, with today's unsaved part added. Days with nothing are left out.
    pub fn daily_totals(&self, days: i64) -> Vec<DayTotal> {
        let today = days::today();
        let mut totals = self
            .shared
            .with_list(|list| list.daily_totals(today - days + 1))
            .unwrap_or_default();
        let tally = *lock(&self.shared.tally);
        if tally.downloaded > 0 || tally.uploaded > 0 {
            match totals.iter_mut().find(|total| total.day == tally.day) {
                Some(total) => {
                    total.downloaded_bytes += tally.downloaded;
                    total.uploaded_bytes += tally.uploaded;
                }
                None => totals.push(DayTotal {
                    day: tally.day,
                    downloaded_bytes: tally.downloaded,
                    uploaded_bytes: tally.uploaded,
                }),
            }
        }
        totals
    }

    /// The port other people can connect to right now, if BitDinghy accepts connections.
    pub fn listen_port(&self) -> Option<u16> {
        self.shared
            .session
            .listen_addr()
            .map(|address| address.port())
    }

    /// Changes how many downloads run at once (0: no limit). Applies straight away.
    pub fn set_max_active_downloads(&self, limit: usize) {
        self.shared.max_active.store(limit, Ordering::SeqCst);
        self.shared.rebalance();
    }

    /// Moves a download in the list, which is also the queue's order, and remembers the order.
    pub fn move_download(&self, id: DownloadId, to: Move) -> Result<()> {
        let order = {
            let mut entries = lock(&self.shared.entries);
            let from = entries
                .iter()
                .position(|entry| entry.id == id)
                .ok_or_else(|| {
                    Error::Control("That download isn't in the list any more.".to_string())
                })?;
            let last = entries.len() - 1;
            let place = match to {
                Move::Up => from.saturating_sub(1),
                Move::Down => (from + 1).min(last),
                Move::Top => 0,
                Move::Bottom => last,
            };
            let entry = entries.remove(from);
            entries.insert(place, entry);
            entries
                .iter()
                .map(|entry| entry.info_hash.clone())
                .collect::<Vec<_>>()
        };
        self.shared.with_list(|list| list.set_order(&order));
        self.shared.rebalance();
        Ok(())
    }

    /// The folder new downloads are saved in (always an absolute path).
    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }

    /// The download folder a particular download belongs to. A remembered download keeps the
    /// folder it was started in, even if the setting has changed since.
    pub fn download_dir_of(&self, id: DownloadId) -> Option<PathBuf> {
        lock(&self.shared.entries)
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.download_dir.clone())
    }

    /// Messages the person should see, such as a problem with the saved list.
    pub fn notices(&self) -> Vec<String> {
        lock(&self.shared.notices).clone()
    }

    /// Whether downloads are being remembered between runs.
    pub fn is_remembering(&self) -> bool {
        lock(&self.shared.saved)
            .as_ref()
            .is_some_and(SavedList::is_saving)
    }

    /// Starts downloading from a magnet link.
    ///
    /// Returns as soon as the link has been checked. The rest (finding people, getting the file
    /// list, the safety check) happens in the background, and shows up in [`Engine::snapshot`].
    /// `extra_peers` are addresses to try directly (handy on your own network, and in tests).
    ///
    /// If the torrent is already in the saved list (for example `dinghy get` run a second time),
    /// it carries on in the folder it was started in, from the file list saved last time.
    pub fn add_magnet(&self, input: &str, extra_peers: Vec<SocketAddr>) -> Result<DownloadId> {
        let parsed = magnet::parse(input)?;
        self.add_new(
            &parsed.info_hash,
            parsed.display_name.as_deref(),
            &parsed.clean_link,
            Source::Magnet(parsed.clean_link.clone()),
            AddOptions::with_peers(extra_peers),
        )
    }

    /// Adds a magnet link or a `.torrent` file at once (see `input.rs`). A `.torrent` file is
    /// read and rebuilt from checked parts first (`torrent_file.rs`), so a bad file fails here.
    /// Web links need fetching: use [`Engine::add_any`].
    pub fn add(&self, input: &str, extra_peers: Vec<SocketAddr>) -> Result<DownloadId> {
        self.add_with(input, AddOptions::with_peers(extra_peers))
    }

    /// [`Engine::add`] with more choices (see [`AddOptions`]).
    pub fn add_with(&self, input: &str, options: AddOptions) -> Result<DownloadId> {
        match input::classify(input)? {
            Input::Magnet(parsed) => self.add_new(
                &parsed.info_hash,
                parsed.display_name.as_deref(),
                &parsed.clean_link,
                Source::Magnet(parsed.clean_link.clone()),
                options,
            ),
            Input::TorrentFile(path) => {
                self.add_checked_torrent(torrent_file::read(&path)?, options)
            }
            // Fetching takes time, so web links go through `add_any`.
            Input::WebLink(_) => Err(Error::WebLink(
                "A web link has to be fetched first (use `add_any`).".to_string(),
            )),
        }
    }

    /// Adds anything: a magnet link, a `.torrent` file, or a web link to one, which is fetched
    /// first (carefully, see `web_link.rs`). Fetching can take a few seconds, so this is `async`;
    /// the window runs it in the background.
    pub async fn add_any(&self, input: &str, extra_peers: Vec<SocketAddr>) -> Result<DownloadId> {
        self.add_any_with(input, AddOptions::with_peers(extra_peers))
            .await
    }

    /// [`Engine::add_any`] with more choices (see [`AddOptions`]).
    pub async fn add_any_with(&self, input: &str, options: AddOptions) -> Result<DownloadId> {
        match input::classify(input)? {
            Input::WebLink(link) => {
                let rules = web_link::Rules::new(self.allow_http_links.load(Ordering::SeqCst));
                let torrent = web_link::fetch(&link, rules).await?;
                self.add_checked_torrent(torrent, options)
            }
            _ => self.add_with(input, options),
        }
    }

    /// Fetches a web link to a `.torrent` file and adds it. Plain http links only if allowed.
    pub async fn add_from_web(
        &self,
        link: &str,
        extra_peers: Vec<SocketAddr>,
    ) -> Result<DownloadId> {
        let rules = web_link::Rules::new(self.allow_http_links.load(Ordering::SeqCst));
        let torrent = web_link::fetch(link, rules).await?;
        self.add_torrent(torrent, extra_peers)
    }

    /// Changes the "allow plain http links" setting for this run (the window's Settings).
    pub fn set_allow_http_links(&self, allow: bool) {
        self.allow_http_links.store(allow, Ordering::SeqCst);
    }

    /// Adds a `.torrent` file that has already been checked and rebuilt (`torrent_file.rs`).
    pub fn add_torrent(
        &self,
        torrent: CheckedTorrent,
        extra_peers: Vec<SocketAddr>,
    ) -> Result<DownloadId> {
        self.add_checked_torrent(torrent, AddOptions::with_peers(extra_peers))
    }

    fn add_checked_torrent(
        &self,
        torrent: CheckedTorrent,
        options: AddOptions,
    ) -> Result<DownloadId> {
        self.add_new(
            &torrent.info_hash,
            torrent.display_name.as_deref(),
            &torrent.magnet,
            Source::Torrent {
                bytes: torrent.bytes,
                folder: None,
            },
            options,
        )
    }

    /// The shared part of adding: the same torrent twice is one download; one that is already in
    /// the saved list carries on in its own folder from its saved file list; anything else is
    /// remembered and started from `first_source`.
    fn add_new(
        &self,
        info_hash: &str,
        name: Option<&str>,
        clean_link: &str,
        first_source: Source,
        options: AddOptions,
    ) -> Result<DownloadId> {
        let mut entries = lock(&self.shared.entries);
        if let Some(existing) = entries.iter().find(|e| e.info_hash == info_hash).cloned() {
            drop(entries);
            // Adding the same torrent twice just shows the same download (asking to use the files
            // already there tries a download refused for that reason again).
            if options.use_existing_files
                && existing.name_clash.load(Ordering::SeqCst)
                && matches!(*lock(&existing.state), EntryState::Failed(_))
            {
                self.use_existing_files(existing.id)?;
            }
            return Ok(existing.id);
        }
        // Kept so that "Try again" can start from this file list without asking anyone for it.
        let known_torrent = match &first_source {
            Source::Torrent { bytes, .. } => Some(bytes.clone()),
            Source::Magnet(_) => None,
        };

        let remembered = self.shared.with_list(|list| list.get(info_hash)).flatten();
        let skipped = remembered
            .as_ref()
            .map(|saved| saved.skipped_files.clone())
            .unwrap_or_default();
        let counts = remembered
            .as_ref()
            .map(Counts::from_saved)
            .unwrap_or_default();
        // Where it starts from is worked out when its turn in the queue comes (`start_again`): its
        // saved file list, or `first_source` (a `.torrent` is kept as `known_torrent`).
        let (label, download_dir) = match remembered {
            Some(saved) => {
                // Asking for a download again means "carry on", even if it was paused.
                if saved.paused {
                    self.shared
                        .with_list(|list| list.set_paused(info_hash, false));
                }
                (saved.name.clone(), saved.download_dir.clone())
            }
            None => {
                let label = name
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("Torrent {}", &info_hash[..8]));
                self.shared.with_list(|list| {
                    list.add(&SavedDownload::new(
                        info_hash,
                        &label,
                        clean_link,
                        &self.download_dir,
                    ))
                });
                (label, self.download_dir.clone())
            }
        };

        let entry = self.new_entry(NewEntry {
            info_hash: info_hash.to_string(),
            label,
            download_dir,
            magnet: clean_link.to_string(),
            paused: false,
            peers: options.peers,
            known_torrent,
            skipped,
            use_existing: options.use_existing_files,
            counts,
        });
        *lock(&entry.state) = EntryState::Waiting;
        let id = entry.id;
        entries.push(entry);
        drop(entries);

        self.shared.rebalance();
        Ok(id)
    }

    /// A picture of every download right now. Cheap to call, so redraw from it a few times a
    /// second.
    pub fn snapshot(&self) -> Vec<DownloadSnapshot> {
        let entries = lock(&self.shared.entries).clone();
        entries.iter().map(|entry| snapshot_of(entry)).collect()
    }

    /// Stops sharing, closes connections and closes the saved list (so the next BitDinghy can open
    /// it). Files already on disk are left as they are, so the downloads continue next time.
    pub async fn shutdown(&self) {
        self.shared.stopped.store(true, Ordering::SeqCst);
        // What was shared since the last count, then everything unsaved, before the list closes.
        self.shared.account(Duration::ZERO);
        self.shared.save_counts();
        self.shared.session.stop().await;
        lock(&self.shared.saved).take();
    }

    /// Brings back every download in the saved list, through the same safety steps as a new one.
    fn restore_saved(&self) {
        let saved = self.shared.with_list(SavedList::all).unwrap_or_default();
        for download in saved {
            // The list is our own file, but it is checked like anything else from outside.
            let magnet = magnet::parse(&download.magnet)
                .map(|parsed| parsed.clean_link)
                .unwrap_or_default();
            let (info_hash, name) = (download.info_hash.clone(), download.name.clone());
            let (download_dir, paused) = (download.download_dir.clone(), download.paused);
            let skipped = download.skipped_files.clone();
            let counts = Counts::from_saved(&download);
            let source = match Source::saved(download) {
                Some(source) => source,
                None if !magnet.is_empty() => Source::Magnet(magnet.clone()),
                None => {
                    self.shared.notice(format!(
                        "The saved download \"{name}\" has a broken magnet link and was skipped."
                    ));
                    continue;
                }
            };
            let entry = self.new_entry(NewEntry {
                info_hash,
                label: name,
                download_dir,
                magnet,
                paused,
                peers: Vec::new(),
                known_torrent: None,
                skipped,
                use_existing: false,
                counts,
            });
            lock(&self.shared.entries).push(entry.clone());
            match source {
                // Paused, and its file list was never fetched: contact nobody until resumed.
                Source::Magnet(_) if paused => *lock(&entry.state) = EntryState::Stopped,
                // A known file list is added paused: its files are checked so the progress shows,
                // but nobody is contacted (and it takes no place in the queue).
                _ if paused => self.shared.spawn_start(entry, source),
                // The others start when the queue says so.
                _ => *lock(&entry.state) = EntryState::Waiting,
            }
        }
        self.shared.rebalance();
    }

    fn new_entry(&self, start: NewEntry) -> Arc<Entry> {
        Arc::new(Entry {
            id: DownloadId(self.next_id.fetch_add(1, Ordering::Relaxed)),
            info_hash: start.info_hash,
            label: start.label,
            download_dir: start.download_dir,
            magnet: start.magnet,
            wants_pause: AtomicBool::new(start.paused),
            peers: start.peers,
            known_torrent: start.known_torrent,
            removed: AtomicBool::new(false),
            held: AtomicBool::new(false),
            files: Mutex::new(None),
            file_list: Mutex::new(None),
            skipped: Mutex::new(start.skipped),
            use_existing: AtomicBool::new(start.use_existing),
            name_clash: AtomicBool::new(false),
            check_requested: AtomicBool::new(false),
            check_only: AtomicBool::new(false),
            note: Mutex::new(None),
            warning: Mutex::new(None),
            counts: Mutex::new(start.counts),
            state: Mutex::new(EntryState::GettingInfo(Instant::now())),
            speed: Mutex::new(Smoother::new()),
        })
    }

    /// Removes a download: it stops, leaves the window and the saved list, and is forgotten.
    ///
    /// With `delete_files`, the files it downloaded are deleted too: exactly the files in its own
    /// checked file list, and any folders they leave empty (see `owned_files.rs`). Nothing else is
    /// ever deleted. Without it, the files stay where they are.
    pub async fn remove(&self, id: DownloadId, delete_files: bool) -> Result<Deleted> {
        let entry = self.entry(id)?;
        // Work out what would be deleted BEFORE changing anything: if its file list can't be
        // read, nothing is removed at all.
        let known = self.known_file_list(&entry);
        let owned = if delete_files {
            self.owned_paths(&entry)?
        } else {
            None
        };
        entry.removed.store(true, Ordering::SeqCst);
        lock(&self.shared.entries).retain(|other| other.id != id);
        // Its latest counts first, so the history has them.
        self.shared.save_entry_counts(&entry);
        self.shared.with_list(|list| list.remove(&entry.info_hash));
        // Its place in the queue is free for the next one.
        self.shared.rebalance();

        // Stop it in the engine first: on Windows a file can't be deleted while it is open.
        let handle = match &*lock(&entry.state) {
            EntryState::Active(handle) => Some(handle.clone()),
            _ => None,
        };
        if let Some(handle) = handle {
            self.shared
                .session
                .delete(handle.id().into(), false)
                .await
                .map_err(|e| Error::Control(format!("Couldn't stop \"{}\": {e:#}", entry.label)))?;
        }
        let Some((paths, output_folder)) = owned else {
            // Keeping the files: the leftovers of unticked files become visible again, so nothing
            // BitDinghy no longer looks after stays hidden. (If its file list never arrived,
            // nothing was written.)
            if let Some((torrent, output_folder)) = known {
                let download_dir = entry.download_dir.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    let everything = BTreeSet::new();
                    if let Ok(marks) = owned_files::leftover_marks(
                        &torrent,
                        &download_dir,
                        &output_folder,
                        &everything,
                    ) {
                        owned_files::apply_hidden(&marks);
                    }
                })
                .await;
            }
            return Ok(Deleted::default());
        };
        let download_dir = entry.download_dir.clone();
        // Deleting can wait a moment for Windows to let go of a file, so it runs off to the side.
        tokio::task::spawn_blocking(move || {
            owned_files::delete(&paths, &download_dir, &output_folder)
        })
        .await
        .map_err(|e| Error::Control(format!("Deleting the files stopped unexpectedly: {e}")))
    }

    /// The files "Remove, delete the files too" would delete right now: those in the download's
    /// own file list that exist. Nothing is changed. (`dinghy remove --delete-files` shows these
    /// before asking for "yes".)
    pub fn files_to_delete(&self, id: DownloadId) -> Result<Vec<PathBuf>> {
        let entry = self.entry(id)?;
        Ok(self
            .owned_paths(&entry)?
            .map(|(paths, _)| paths.into_iter().filter(|path| path.exists()).collect())
            .unwrap_or_default())
    }

    /// Every file a download owns, and its folder: from its file list as the engine knows it, or
    /// else as it was saved. `None` when its file list never arrived (nothing was written).
    fn owned_paths(&self, entry: &Entry) -> Result<Option<(Vec<PathBuf>, PathBuf)>> {
        let Some((torrent, output_folder)) = self.known_file_list(entry) else {
            return Ok(None);
        };
        let paths = owned_files::files_of(&torrent, &entry.download_dir, &output_folder)?;
        Ok(Some((paths, output_folder)))
    }

    /// A download's files with tick boxes: which are chosen, and how far each has got (while it
    /// runs). Fails while its file list isn't known yet.
    pub fn files(&self, id: DownloadId) -> Result<Vec<FileChoice>> {
        let entry = self.entry(id)?;
        let files = self.file_list_of(&entry)?.ok_or_else(files_not_known)?;
        let progress = match &*lock(&entry.state) {
            EntryState::Active(handle) => Some(handle.stats().file_progress),
            _ => None,
        };
        let skipped = lock(&entry.skipped).clone();
        Ok(file_choice::choices(&files, &skipped, progress.as_deref()))
    }

    /// Chooses which files to download, by their numbers in [`Engine::files`] (starting at 1).
    /// The others are no longer downloaded. Works while the download runs or is paused, and is
    /// remembered. At least one file must stay chosen.
    pub async fn choose_files(&self, id: DownloadId, chosen: &BTreeSet<usize>) -> Result<()> {
        let entry = self.entry(id)?;
        let files = self.file_list_of(&entry)?.ok_or_else(files_not_known)?;
        let skipped = file_choice::skipped_for(&files, chosen)?;

        let handle = match &*lock(&entry.state) {
            EntryState::Active(handle) => Some(handle.clone()),
            // Between finding its file list and starting, the choice could be missed: say so.
            EntryState::GettingInfo(_) => return Err(still_starting(&entry.label)),
            // Not running: the choice is used when it starts.
            EntryState::Stopped | EntryState::Failed(_) | EntryState::Waiting => None,
        };
        if let Some(handle) = handle {
            // The engine can't change the choice while it checks the files already on disk.
            if matches!(handle.stats().state, TorrentStatsState::Initializing { .. }) {
                return Err(still_starting(&entry.label));
            }
            let wanted: HashSet<usize> = files
                .iter()
                .map(|file| file.index)
                .filter(|index| !skipped.contains(index))
                .collect();
            self.shared
                .session
                .update_only_files(&handle, &wanted)
                .await
                .map_err(|e| {
                    Error::FileChoice(format!(
                        "Couldn't change the files of \"{}\": {e:#}",
                        entry.label
                    ))
                })?;
        }
        self.shared
            .with_list(|list| list.set_skipped_files(&entry.info_hash, &skipped));
        *lock(&entry.skipped) = skipped;
        // Hide what is left of newly unticked files; show newly ticked ones again.
        let _ = tokio::task::spawn_blocking(move || mark_leftovers(&entry)).await;
        Ok(())
    }

    /// A download's file list for choosing files: from the running download, or else from its
    /// saved file list. `None` while it isn't known yet.
    fn file_list_of(&self, entry: &Entry) -> Result<Option<Arc<Vec<TorrentFile>>>> {
        if let Some(files) = lock(&entry.file_list).clone() {
            return Ok(Some(files));
        }
        let torrent = lock(&entry.files)
            .as_ref()
            .map(|(torrent, _)| torrent.clone())
            .or_else(|| {
                self.shared
                    .with_list(|list| list.get(&entry.info_hash))
                    .flatten()
                    .and_then(|saved| saved.torrent)
            });
        let Some(torrent) = torrent else {
            return Ok(None);
        };
        let files = Arc::new(file_choice::files_of_torrent(&torrent)?);
        *lock(&entry.file_list) = Some(files.clone());
        Ok(Some(files))
    }

    /// A download's checked file list (a `.torrent` file) and its folder: as the engine knows it,
    /// or else as it was saved. `None` when its file list never arrived (nothing was written).
    fn known_file_list(&self, entry: &Entry) -> Option<(Vec<u8>, PathBuf)> {
        lock(&entry.files).clone().or_else(|| {
            self.shared
                .with_list(|list| list.get(&entry.info_hash))
                .flatten()
                .and_then(|saved| Some((saved.torrent?, saved.output_folder?)))
        })
    }

    fn entry(&self, id: DownloadId) -> Result<Arc<Entry>> {
        lock(&self.shared.entries)
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or_else(|| Error::Control("That download isn't in the list any more.".to_string()))
    }

    /// Pauses a download: it stops downloading and sharing, and stays paused after a restart. A
    /// download still fetching its file list starts paused once the list arrives.
    pub async fn pause(&self, id: DownloadId) -> Result<()> {
        let entry = self.entry(id)?;
        entry.wants_pause.store(true, Ordering::SeqCst);
        self.shared
            .with_list(|list| list.set_paused(&entry.info_hash, true));
        let handle = {
            let mut state = lock(&entry.state);
            match &*state {
                EntryState::Active(handle) => Some(handle.clone()),
                // Pausing a failed download means "leave it alone for now"; a queued one that
                // never started just stays put.
                EntryState::Failed(_) | EntryState::Waiting => {
                    *state = EntryState::Stopped;
                    None
                }
                EntryState::GettingInfo(_) | EntryState::Stopped => None,
            }
        };
        entry.held.store(false, Ordering::SeqCst);
        if let Some(handle) = handle
            && !handle.is_paused()
            && let Err(error) = self.shared.session.pause(&handle).await
            && !handle.is_paused()
        {
            return Err(Error::Control(format!(
                "Couldn't pause \"{}\": {error:#}",
                entry.label
            )));
        }
        // Its place in the queue is free for the next one.
        self.shared.rebalance();
        Ok(())
    }

    /// Resumes a paused download. On a download that failed, it tries again from the start (the
    /// file list goes through the guard again, like a new download).
    pub async fn resume(&self, id: DownloadId) -> Result<()> {
        let entry = self.entry(id)?;
        entry.wants_pause.store(false, Ordering::SeqCst);
        self.shared
            .with_list(|list| list.set_paused(&entry.info_hash, false));

        // Stopped by a sharing limit: the person wants it to share again, limits or not.
        {
            let mut counts = lock(&entry.counts);
            if counts.sharing == Sharing::StoppedByLimit {
                counts.sharing = Sharing::KeepSharing;
                counts.unsaved = true;
                drop(counts);
                self.shared.save_entry_counts(&entry);
            }
        }
        // A download that isn't running joins the queue; the queue then starts it, or lets a
        // paused one carry on, if there is a place for it (otherwise it shows as queued).
        {
            let mut state = lock(&entry.state);
            if matches!(*state, EntryState::Stopped | EntryState::Failed(_)) {
                entry.name_clash.store(false, Ordering::SeqCst);
                *state = EntryState::Waiting;
            }
        }
        self.shared.rebalance();
        Ok(())
    }

    /// For a download refused because a file with the same name was already there: use that file
    /// (and any others with its files' names) after all. The files are checked first and
    /// completed; from then on they count as this download's files.
    pub fn use_existing_files(&self, id: DownloadId) -> Result<()> {
        let entry = self.entry(id)?;
        {
            let mut state = lock(&entry.state);
            if !matches!(*state, EntryState::Failed(_) | EntryState::Stopped) {
                return Err(Error::Control(format!(
                    "\"{}\" is already running with its own files.",
                    entry.label
                )));
            }
            *state = EntryState::Waiting;
        }
        entry.use_existing.store(true, Ordering::SeqCst);
        entry.name_clash.store(false, Ordering::SeqCst);
        entry.wants_pause.store(false, Ordering::SeqCst);
        self.shared
            .with_list(|list| list.set_paused(&entry.info_hash, false));
        self.shared.rebalance();
        Ok(())
    }

    /// Checks a download's files on disk again, piece by piece, against the torrent's
    /// fingerprints. Anything missing or damaged is downloaded again (when it runs). The result
    /// shows up as the download's `note` when the check is done. A paused download stays paused.
    pub async fn check(&self, id: DownloadId) -> Result<()> {
        let entry = self.entry(id)?;
        let Some((bytes, folder)) = self.known_file_list(&entry) else {
            return Err(Error::Control(format!(
                "\"{}\" can't be checked yet: BitDinghy doesn't know its files yet.",
                entry.label
            )));
        };
        let handle = {
            let mut state = lock(&entry.state);
            let handle = match &*state {
                EntryState::Active(handle) => {
                    if matches!(handle.stats().state, TorrentStatsState::Initializing { .. })
                        && !handle.is_paused()
                    {
                        return Err(Error::Control(format!(
                            "\"{}\" is being checked already.",
                            entry.label
                        )));
                    }
                    Some(handle.clone())
                }
                EntryState::GettingInfo(_) => return Err(still_starting(&entry.label)),
                EntryState::Stopped | EntryState::Failed(_) | EntryState::Waiting => None,
            };
            *state = EntryState::GettingInfo(Instant::now());
            handle
        };
        *lock(&entry.note) = None;
        entry.check_requested.store(true, Ordering::SeqCst);
        if let Some(handle) = handle {
            // Out of librqbit (its files stay), so that adding it again checks every piece.
            if let Err(error) = self.shared.session.delete(handle.id().into(), false).await {
                entry.check_requested.store(false, Ordering::SeqCst);
                *lock(&entry.state) = EntryState::Active(handle);
                return Err(Error::Control(format!(
                    "Couldn't check \"{}\": {error:#}",
                    entry.label
                )));
            }
        }
        self.shared.spawn_start(
            entry,
            Source::Torrent {
                bytes,
                folder: Some(folder),
            },
        );
        Ok(())
    }

    /// For `dinghy check` with the window closed: checks a SAVED download's files without
    /// downloading anything (it is added paused, just for this engine; the saved list is not
    /// changed). Its result shows up as the download's `note`.
    pub fn check_saved(&self, info_hash: &str) -> Result<DownloadId> {
        let saved = self
            .shared
            .with_list(|list| list.get(info_hash))
            .flatten()
            .ok_or_else(|| {
                Error::Control("That download isn't in the saved list any more.".to_string())
            })?;
        let (label, download_dir) = (saved.name.clone(), saved.download_dir.clone());
        let skipped = saved.skipped_files.clone();
        let magnet = saved.magnet.clone();
        let counts = Counts::from_saved(&saved);
        let Some(source) = Source::saved(saved) else {
            return Err(Error::Control(format!(
                "\"{label}\" can't be checked yet: BitDinghy doesn't know its files yet."
            )));
        };
        let entry = self.new_entry(NewEntry {
            info_hash: info_hash.to_string(),
            label,
            download_dir,
            magnet,
            paused: true,
            peers: Vec::new(),
            known_torrent: None,
            skipped,
            use_existing: false,
            counts,
        });
        entry.check_requested.store(true, Ordering::SeqCst);
        entry.check_only.store(true, Ordering::SeqCst);
        let id = entry.id;
        lock(&self.shared.entries).push(entry.clone());
        self.shared.spawn_start(entry, source);
        Ok(id)
    }
}

/// Refuses a download with a file the download folder's drive can't hold (FAT32: under 4 GB),
/// and, for a new download, warns when the drive has less free room than the chosen files need.
fn check_drive(
    download_dir: &Path,
    chosen: &[TorrentFile],
    continuing: bool,
) -> Result<Option<String>> {
    drive_problem(
        drive::drive_of(download_dir),
        drive::free_space(download_dir),
        chosen,
        continuing,
    )
}

/// The decision of [`check_drive`], given what Windows said about the drive.
fn drive_problem(
    drive: Option<drive::Drive>,
    free_space: Option<u64>,
    chosen: &[TorrentFile],
    continuing: bool,
) -> Result<Option<String>> {
    if let Some(drive) = &drive
        && drive.has_4gb_limit()
        && let Some(big) = chosen
            .iter()
            .find(|file| file.size > drive::FAT32_MAX_FILE_BYTES)
    {
        return Err(Error::DriveTooSmallForFile {
            drive: drive.root.clone(),
            format: drive.format.clone(),
            file: big.path.display().to_string(),
            size: format_bytes(big.size),
        });
    }
    let needed: u64 = chosen.iter().map(|file| file.size).sum();
    match free_space {
        Some(free) if !continuing && free < needed => Ok(Some(format!(
            "The drive{} has {} free, but this download needs {}. It stops when the drive is \
             full: free some space, or choose a download folder on another drive in Settings.",
            drive.map(|d| format!(" {}", d.root)).unwrap_or_default(),
            format_bytes(free),
            format_bytes(needed)
        ))),
        _ => Ok(None),
    }
}

/// Whether a finished download has shared enough: its uploads reached `ratio_percent` of its size
/// (100 = as much as its size), or it shared for `minutes`. 0 turns a limit off.
fn sharing_limit_reached(counts: &Counts, size: u64, ratio_percent: u64, minutes: u64) -> bool {
    let by_ratio = ratio_percent > 0
        && counts.uploaded_bytes.saturating_mul(100) >= ratio_percent.saturating_mul(size);
    let by_time = minutes > 0 && counts.shared_millis >= minutes.saturating_mul(60_000);
    by_ratio || by_time
}

/// The current time in seconds since 1970.
fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A limit in KiB/s as librqbit wants it: bytes per second, `None` for no limit.
fn bytes_per_second(kib: u32) -> Option<NonZeroU32> {
    NonZeroU32::new(kib.saturating_mul(1024))
}

/// What a finished check found, in plain words. `check_only`: checked by `dinghy check` with the
/// window closed (it is paused only for the check).
fn check_note(handle: &ManagedTorrent, check_only: bool) -> String {
    let stats = handle.stats();
    let missing = stats.total_bytes.saturating_sub(stats.progress_bytes);
    if missing == 0 {
        "Checked: every chosen file is complete and correct.".to_string()
    } else if check_only {
        format!(
            "Checked: {} of the chosen files are missing or damaged. They are downloaded again \
              the next time BitDinghy runs this download.",
            format_bytes(missing)
        )
    } else if handle.is_paused() {
        format!(
            "Checked: {} of the chosen files are missing or damaged. They are downloaded again \
             when you resume it.",
            format_bytes(missing)
        )
    } else {
        format!(
            "Checked: {} of the chosen files were missing or damaged. BitDinghy downloads them \
             again.",
            format_bytes(missing)
        )
    }
}

/// Hides the leftovers of a download's unticked files and shows its ticked ones (see
/// `owned_files.rs`). Best effort: a file that can't be hidden is harmless. Blocking (file system).
fn mark_leftovers(entry: &Entry) {
    let Some((torrent, output_folder)) = lock(&entry.files).clone() else {
        return;
    };
    let skipped = lock(&entry.skipped).clone();
    if let Ok(marks) =
        owned_files::leftover_marks(&torrent, &entry.download_dir, &output_folder, &skipped)
    {
        owned_files::apply_hidden(&marks);
    }
}

fn files_not_known() -> Error {
    Error::FileChoice(
        "BitDinghy doesn't know this download's files yet. They can be chosen once it has found \
         its file list."
            .to_string(),
    )
}

fn still_starting(name: &str) -> Error {
    Error::FileChoice(format!(
        "\"{name}\" is still starting (BitDinghy is checking its files). Try again in a moment."
    ))
}

/// Steps 1 to 3 from the module notes.
async fn start_download(
    shared: &Shared,
    entry: &Entry,
    source: Source,
) -> Result<Arc<ManagedTorrent>> {
    let initial_peers = (!entry.peers.is_empty()).then(|| entry.peers.clone());
    // The engine reports problems as general-purpose errors; `{:#}` prints the whole cause chain,
    // and `plain::explain` puts the common ones in plain words first.
    let engine_error = |e| Error::Add(plain::explain(&format!("{e:#}")));

    // Step 1: just the file list. Nothing is created on disk.
    let (add, saved_folder) = match source {
        Source::Magnet(link) => (AddTorrent::from_url(link), None),
        Source::Torrent { bytes, folder } => (AddTorrent::from_bytes(bytes), folder),
    };
    let answer = shared
        .session
        .add_torrent(
            add,
            Some(AddTorrentOptions {
                list_only: true,
                initial_peers: initial_peers.clone(),
                ..Default::default()
            }),
        )
        .await
        .map_err(engine_error)?;
    let AddTorrentResponse::ListOnly(listing) = answer else {
        return Err(Error::Add(
            "the engine gave an unexpected answer".to_string(),
        ));
    };

    // Step 2: the guard. A torrent refused here is not remembered either.
    let vetted = vet_listing(
        &shared.session_dir,
        &entry.download_dir,
        saved_folder.as_deref(),
        &listing,
    );
    let output_folder = match vetted {
        Ok(folder) => folder,
        Err(error) => {
            if matches!(error, Error::UnsafeFileName(_)) {
                shared.with_list(|list| list.forget(&entry.info_hash));
            }
            return Err(error);
        }
    };

    // A NEW download must not take over files that are already there (they aren't ours). Only a
    // download continuing in the very folder it was started in (and checked in) skips this: the
    // files there are its own. A refused one is kept in the list, so the person can move the
    // other file away and press "Try again".
    let paths = owned_files::paths_in(&listing.info, &output_folder)?;
    // Never save through a link (a symbolic link or junction) to another place.
    owned_files::refuse_links(&entry.download_dir, &paths)?;
    let continuing = saved_folder.is_some();
    if !continuing && !entry.use_existing.load(Ordering::SeqCst) {
        owned_files::refuse_existing_files(&paths)?;
    }
    *lock(&entry.files) = Some((listing.torrent_bytes.to_vec(), output_folder.clone()));
    // The files the person chose (a saved choice is checked against the real list first).
    let files = file_choice::files_in(&listing.info);
    let (only_files, chosen) = {
        let mut skipped = lock(&entry.skipped);
        *skipped = file_choice::clean_skipped(&files, &skipped);
        let chosen: Vec<TorrentFile> = files
            .iter()
            .filter(|file| !skipped.contains(&file.index))
            .cloned()
            .collect();
        (file_choice::only_files(&files, &skipped), chosen)
    };
    // Say what the drive can't do now, not halfway through (M12).
    *lock(&entry.warning) = check_drive(&entry.download_dir, &chosen, continuing)?;
    *lock(&entry.file_list) = Some(Arc::new(files));
    if entry.removed.load(Ordering::SeqCst) {
        return Err(Error::Control("removed".to_string()));
    }
    let folder_text = output_folder
        .to_str()
        .ok_or_else(|| {
            Error::Add(format!(
                "the folder {} can't be used",
                output_folder.display()
            ))
        })?
        .to_string();

    // Remember the checked file list, so the next start doesn't have to ask anyone for it.
    let name = listing
        .info
        .name()
        .and_then(|name| magnet::clean_display_name(&name))
        .unwrap_or_else(|| entry.label.clone());
    shared.with_list(|list| {
        list.record_file_list(
            &entry.info_hash,
            &name,
            &listing.torrent_bytes,
            &output_folder,
        )
    });

    // Step 3: start for real from the file list we already hold. `overwrite` lets an interrupted
    // download carry on from the files already on disk (each piece is re-checked first).
    shared
        .session
        .add_torrent(
            AddTorrent::from_bytes(listing.torrent_bytes),
            Some(AddTorrentOptions {
                overwrite: true,
                output_folder: Some(folder_text),
                paused: entry.wants_pause.load(Ordering::SeqCst)
                    || lock(&entry.counts).sharing == Sharing::StoppedByLimit,
                initial_peers,
                only_files,
                ..Default::default()
            }),
        )
        .await
        .map_err(engine_error)?
        .into_handle()
        .ok_or_else(|| Error::Add("the engine did not start the download".to_string()))
}

/// Refuses a torrent whose files could end up outside its download folder, and returns the exact
/// folder its files will be saved in.
///
/// A NEW download: librqbit places a torrent relative to the folder the session started with
/// (`session_dir`): directly in it for a single file, or in a subfolder named after the torrent.
/// We keep that shape but move it into this download's own folder (a remembered download stays
/// in the download folder it was started in, even if the setting changed).
///
/// A CONTINUING download (`saved_folder`): always the folder it was started in. librqbit's choice
/// is ignored, because it can differ from last time; the security review found that a torrent
/// could use that to move into a folder full of someone else's files.
fn vet_listing(
    session_dir: &Path,
    download_dir: &Path,
    saved_folder: Option<&Path>,
    listing: &ListOnlyResponse,
) -> Result<PathBuf> {
    // Every name must be safe on this operating system (this also refuses several files with no
    // folder name).
    safety::check_torrent(&listing.info, Platform::current())?;

    let output_folder = match saved_folder {
        Some(folder) => folder.to_path_buf(),
        None => {
            // A second, independent check: the folder the engine chose must be inside its own
            // folder and made only of plain names.
            let relative = listing
                .output_folder
                .strip_prefix(session_dir)
                .map_err(|_| safety::UnsafePath::outside_download_folder(&listing.output_folder))?;
            if relative.as_os_str().is_empty() {
                download_dir.to_path_buf() // A single file goes straight into the download folder.
            } else {
                safety::join_inside(download_dir, relative)?
            }
        }
    };
    // Either way, the folder must have the only shape BitDinghy creates, and every file path must
    // be a plain relative path under it.
    safety::check_download_folder(download_dir, &output_folder)?;
    for file in listing.info.iter_file_details() {
        safety::join_inside(&output_folder, &file.filename.to_pathbuf())?;
    }
    Ok(output_folder)
}

fn snapshot_of(entry: &Entry) -> DownloadSnapshot {
    let (files_total, files_chosen) = match &*lock(&entry.file_list) {
        Some(files) => {
            let skipped = lock(&entry.skipped);
            let chosen = files
                .iter()
                .filter(|file| !skipped.contains(&file.index))
                .count();
            (files.len(), chosen)
        }
        None => (0, 0),
    };
    let counts = lock(&entry.counts).clone();
    let mut snapshot = DownloadSnapshot {
        id: entry.id,
        info_hash: entry.info_hash.clone(),
        name: entry.label.clone(),
        status: DownloadStatus::GettingInfo,
        progress_bytes: 0,
        total_bytes: 0,
        download_bytes_per_second: 0,
        upload_bytes_per_second: 0,
        eta: None,
        peers_connected: 0,
        peers_known: 0,
        uploaded_bytes: counts.uploaded_bytes,
        shared_seconds: counts.shared_millis / 1000,
        sharing_stopped: counts.sharing == Sharing::StoppedByLimit,
        waiting_for: None,
        save_folder: None,
        error: None,
        files_total,
        files_chosen,
        note: lock(&entry.note).clone(),
        can_use_existing_files: false,
        warning: lock(&entry.warning).clone(),
    };

    let state = lock(&entry.state);
    match &*state {
        // Taken out of librqbit for a check, and on its way back in.
        EntryState::GettingInfo(_) if entry.check_requested.load(Ordering::SeqCst) => {
            snapshot.status = DownloadStatus::Checking;
        }
        EntryState::GettingInfo(since) => {
            if entry.wants_pause.load(Ordering::SeqCst) {
                snapshot.status = DownloadStatus::Paused;
            } else {
                snapshot.waiting_for = Some(since.elapsed());
            }
        }
        EntryState::Stopped => {
            snapshot.status = DownloadStatus::Paused;
        }
        EntryState::Waiting => {
            snapshot.status = DownloadStatus::Queued;
        }
        EntryState::Failed(message) => {
            snapshot.status = DownloadStatus::Failed;
            snapshot.error = Some(message.clone());
            snapshot.can_use_existing_files = entry.name_clash.load(Ordering::SeqCst);
        }
        EntryState::Active(handle) => {
            let stats = handle.stats();
            if let Some(name) = handle.name().and_then(|n| magnet::clean_display_name(&n)) {
                snapshot.name = name;
            }
            snapshot.progress_bytes = stats.progress_bytes;
            snapshot.total_bytes = stats.total_bytes;

            snapshot.save_folder = Some(handle.output_folder().to_path_buf());
            if let Some(live) = &stats.live {
                let raw_speed = live.download_speed.as_bytes() as f64;
                snapshot.download_bytes_per_second =
                    lock(&entry.speed).update(raw_speed, Instant::now()) as u64;
                snapshot.upload_bytes_per_second = live.upload_speed.as_bytes();
                snapshot.peers_connected = live.snapshot.peer_stats.live;
                snapshot.peers_known = live.snapshot.peer_stats.seen;
            }
            snapshot.status = match stats.state {
                // Paused during a check, librqbit stops checking and waits: that is "paused".
                // (A "check files" of a paused download does check, though.)
                TorrentStatsState::Initializing { paused }
                    if paused && !entry.check_requested.load(Ordering::SeqCst) =>
                {
                    DownloadStatus::Paused
                }
                TorrentStatsState::Initializing { .. } => DownloadStatus::Checking,
                TorrentStatsState::Live if stats.finished => DownloadStatus::Finished,
                TorrentStatsState::Live => DownloadStatus::Downloading,
                // Paused by the queue, not by the person.
                TorrentStatsState::Paused
                    if entry.held.load(Ordering::SeqCst)
                        && !entry.wants_pause.load(Ordering::SeqCst) =>
                {
                    DownloadStatus::Queued
                }
                // Finished, and stopped sharing because of a limit (not paused by the person).
                TorrentStatsState::Paused
                    if stats.finished
                        && snapshot.sharing_stopped
                        && !entry.wants_pause.load(Ordering::SeqCst) =>
                {
                    DownloadStatus::Finished
                }
                TorrentStatsState::Paused => DownloadStatus::Paused,
                TorrentStatsState::Error => DownloadStatus::Failed,
            };
            if snapshot.status == DownloadStatus::Failed {
                snapshot.error = Some(match stats.error {
                    Some(technical) => plain::explain(&technical),
                    None => "The download stopped with an error.".to_string(),
                });
            }
            // Time left = what's still missing divided by the current speed.
            let remaining = stats.total_bytes.saturating_sub(stats.progress_bytes);
            if snapshot.status == DownloadStatus::Downloading
                && snapshot.download_bytes_per_second > 0
            {
                snapshot.eta = Some(Duration::from_secs(
                    remaining / snapshot.download_bytes_per_second,
                ));
            }
        }
    }
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fat32_drive_refuses_a_file_of_4gb_and_a_full_drive_warns() {
        let files = |sizes: &[u64]| -> Vec<TorrentFile> {
            sizes
                .iter()
                .enumerate()
                .map(|(index, size)| TorrentFile {
                    index,
                    path: PathBuf::from(format!("file{index}.iso")),
                    size: *size,
                })
                .collect()
        };
        let drive = |format: &str| {
            Some(drive::Drive {
                root: "E:\\".to_string(),
                format: format.to_string(),
            })
        };
        let gib = 1024 * 1024 * 1024;
        // A 5 GiB file can't go on FAT32...
        let error = drive_problem(drive("FAT32"), None, &files(&[100, 5 * gib]), false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("uses FAT32"), "{error}");
        assert!(error.contains("file1.iso"), "{error}");
        assert!(error.contains("5.0 GiB"), "{error}");
        // ...but a 3 GiB one can, and NTFS takes anything.
        assert!(drive_problem(drive("FAT32"), None, &files(&[3 * gib]), false).is_ok());
        assert!(drive_problem(drive("NTFS"), None, &files(&[50 * gib]), false).is_ok());

        // Not enough room for a new download: a warning, not a refusal.
        let warning = drive_problem(drive("NTFS"), Some(gib), &files(&[2 * gib]), false)
            .unwrap()
            .unwrap();
        assert!(warning.contains("has 1.0 GiB free"), "{warning}");
        assert!(warning.contains("needs 2.0 GiB"), "{warning}");
        // Enough room, or a download carrying on (part of it is already there): nothing to say.
        assert_eq!(
            drive_problem(drive("NTFS"), Some(3 * gib), &files(&[2 * gib]), false).unwrap(),
            None
        );
        assert_eq!(
            drive_problem(drive("NTFS"), Some(gib), &files(&[2 * gib]), true).unwrap(),
            None
        );
    }

    #[test]
    fn a_sharing_limit_is_reached_by_ratio_or_by_time() {
        let counts = |uploaded_bytes, minutes: u64| Counts {
            uploaded_bytes,
            shared_millis: minutes * 60_000,
            ..Counts::default()
        };
        // No limits: never.
        assert!(!sharing_limit_reached(&counts(u64::MAX, 9_999), 100, 0, 0));
        // Ratio 1.5 of a 1000-byte download: reached at 1500 bytes.
        assert!(!sharing_limit_reached(&counts(1499, 0), 1000, 150, 0));
        assert!(sharing_limit_reached(&counts(1500, 0), 1000, 150, 0));
        // 90 minutes.
        assert!(!sharing_limit_reached(&counts(0, 89), 1000, 0, 90));
        assert!(sharing_limit_reached(&counts(0, 90), 1000, 0, 90));
        // Either one is enough.
        assert!(sharing_limit_reached(&counts(0, 90), 1000, 150, 90));
    }

    #[test]
    fn the_first_reading_is_taken_as_is() {
        let mut smoother = Smoother::new();
        assert_eq!(smoother.update(1000.0, Instant::now()), 1000.0);
    }

    #[test]
    fn a_sudden_dip_only_nudges_the_value() {
        let mut smoother = Smoother::new();
        let start = Instant::now();
        smoother.update(90_000_000.0, start);
        // One second later the raw reading collapses to almost nothing...
        let shown = smoother.update(60_000.0, start + Duration::from_secs(1));
        // ...but the number we show only falls part of the way.
        assert!(shown > 60_000_000.0, "{shown}");
        assert!(shown < 90_000_000.0, "{shown}");
    }

    #[test]
    fn smoothing_depends_on_time_not_on_how_often_it_is_called() {
        let start = Instant::now();
        let mut rarely = Smoother::new();
        rarely.update(100.0, start);
        let after_one_call = rarely.update(0.0, start + Duration::from_secs(2));

        let mut often = Smoother::new();
        often.update(100.0, start);
        let mut after_many_calls = 0.0;
        for step in 1..=20 {
            after_many_calls = often.update(0.0, start + Duration::from_millis(100 * step));
        }
        assert!(
            (after_one_call - after_many_calls).abs() < 0.5,
            "{after_one_call} vs {after_many_calls}"
        );
    }

    #[test]
    fn a_steady_stop_settles_at_exactly_zero() {
        let mut smoother = Smoother::new();
        let start = Instant::now();
        smoother.update(5_000.0, start);
        let shown = smoother.update(0.0, start + Duration::from_secs(120));
        assert_eq!(shown, 0.0);
    }
}
