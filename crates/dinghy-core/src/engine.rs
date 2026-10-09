//! The download engine: BitDinghy's wrapper around librqbit.
//!
//! This is the ONLY file that talks to the BitTorrent engine. Everything else in the project
//! (the window, the command line) sees only the plain types defined here, so upgrading or
//! replacing the engine later means changing this file and nothing else.
//!
//! How a download starts (see `add_checked`):
//! 1. Ask the engine for the torrent's file list ONLY. This creates nothing on disk.
//! 2. Run the file-name guard (`safety`) over that list. Anything unsafe stops here.
//! 3. Only then start the real download, reusing the file list we already have.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use librqbit::dht::DhtPersistenceConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, DhtSessionConfig, ListOnlyResponse,
    ListenerOptions, ManagedTorrent, Session, SessionOptions, TorrentStatsState,
};

use crate::error::{Error, Result};
use crate::magnet::{self, ParsedMagnet};
use crate::safety::{self, Platform};

/// Identifies one download for as long as the program runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DownloadId(pub u64);

/// How the engine should start. `EngineConfig::new` gives the normal everyday settings.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Where downloads are saved. Nothing is ever written outside this folder.
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
}

impl EngineConfig {
    pub fn new(download_dir: impl Into<PathBuf>) -> Self {
        Self {
            download_dir: download_dir.into(),
            use_dht: true,
            accept_incoming: true,
            local_discovery: true,
            dht_state_file: None,
        }
    }
}

/// Where a download is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

/// A picture of one download at one moment. The window and the command line redraw from these.
#[derive(Debug, Clone)]
pub struct DownloadSnapshot {
    pub id: DownloadId,
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
    /// Bytes shared with others so far.
    pub uploaded_bytes: u64,
    /// How long we have been waiting for the file list (only while `GettingInfo`).
    pub waiting_for: Option<Duration>,
    /// The folder the files are saved in, once known.
    pub save_folder: Option<PathBuf>,
    /// A plain-language problem message when `status` is `Failed`.
    pub error: Option<String>,
}

enum EntryState {
    GettingInfo,
    Active(Arc<ManagedTorrent>),
    Failed(String),
}

struct Entry {
    id: DownloadId,
    info_hash: String,
    label: String,
    added: Instant,
    state: Mutex<EntryState>,
    /// Smooths the download speed shown to the user (see [`Smoother`]).
    speed: Mutex<Smoother>,
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

/// The download engine. Create one with [`Engine::start`] and keep it for the whole run.
pub struct Engine {
    session: Arc<Session>,
    runtime: tokio::runtime::Handle,
    download_dir: PathBuf,
    entries: Mutex<Vec<Arc<Entry>>>,
    next_id: AtomicU64,
}

impl Engine {
    /// Starts the engine. Must be called from inside a tokio runtime.
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

        let options = SessionOptions {
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
            persistence: None,
            fastresume: false,
            listen: config.accept_incoming.then(ListenerOptions::default),
            disable_local_service_discovery: !config.local_discovery,
            client_name_and_version: Some(format!("{} {}", crate::APP_NAME, crate::VERSION)),
            ..Default::default()
        };

        let session = Session::new_with_opts(download_dir.clone(), options)
            .await
            .map_err(|e| Error::EngineStart(format!("{e:#}")))?;

        Ok(Engine {
            session,
            runtime: tokio::runtime::Handle::current(),
            download_dir,
            entries: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
        })
    }

    /// The folder downloads are saved in (always an absolute path).
    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }

    /// Starts downloading from a magnet link.
    ///
    /// Returns as soon as the link has been checked. The rest (finding people, getting the file
    /// list, the safety check) happens in the background, and shows up in [`Engine::snapshot`].
    /// `extra_peers` are addresses to try directly (handy on your own network, and in tests).
    pub fn add_magnet(&self, input: &str, extra_peers: Vec<SocketAddr>) -> Result<DownloadId> {
        let parsed = magnet::parse(input)?;

        let mut entries = lock(&self.entries);
        if let Some(existing) = entries.iter().find(|e| e.info_hash == parsed.info_hash) {
            return Ok(existing.id); // Pasting the same link twice just shows the same download.
        }

        let id = DownloadId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let entry = Arc::new(Entry {
            id,
            info_hash: parsed.info_hash.clone(),
            label: parsed
                .display_name
                .clone()
                .unwrap_or_else(|| format!("Torrent {}", &parsed.info_hash[..8])),
            added: Instant::now(),
            state: Mutex::new(EntryState::GettingInfo),
            speed: Mutex::new(Smoother::new()),
        });
        entries.push(entry.clone());
        drop(entries);

        let session = self.session.clone();
        let download_dir = self.download_dir.clone();
        self.runtime.spawn(async move {
            let outcome = add_checked(&session, &download_dir, &parsed, extra_peers).await;
            *lock(&entry.state) = match outcome {
                Ok(handle) => EntryState::Active(handle),
                Err(error) => EntryState::Failed(error.to_string()),
            };
        });
        Ok(id)
    }

    /// A picture of every download right now. Cheap to call, so redraw from it a few times a
    /// second.
    pub fn snapshot(&self) -> Vec<DownloadSnapshot> {
        let entries = lock(&self.entries).clone();
        entries.iter().map(|entry| snapshot_of(entry)).collect()
    }

    /// Stops sharing and closes connections. Files already on disk are left as they are, so
    /// running the same download again continues where it stopped.
    pub async fn shutdown(&self) {
        self.session.stop().await;
    }
}

/// Steps 1 to 3 from the module notes.
async fn add_checked(
    session: &Arc<Session>,
    download_dir: &Path,
    magnet: &ParsedMagnet,
    peers: Vec<SocketAddr>,
) -> Result<Arc<ManagedTorrent>> {
    let initial_peers = (!peers.is_empty()).then_some(peers);
    // The engine reports problems as general-purpose errors; `{:#}` prints the whole cause chain.
    let engine_error = |e| Error::Add(format!("{e:#}"));

    // Step 1: just the file list. Nothing is created on disk.
    let answer = session
        .add_torrent(
            AddTorrent::from_url(magnet.clean_link.clone()),
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

    // Step 2: the guard.
    vet_listing(download_dir, &listing)?;

    // Step 3: start for real from the file list we already hold. `overwrite` lets an interrupted
    // download carry on from the files already on disk (each piece is re-checked first).
    session
        .add_torrent(
            AddTorrent::from_bytes(listing.torrent_bytes),
            Some(AddTorrentOptions {
                overwrite: true,
                initial_peers,
                ..Default::default()
            }),
        )
        .await
        .map_err(engine_error)?
        .into_handle()
        .ok_or_else(|| Error::Add("the engine did not start the download".to_string()))
}

/// Refuses a torrent whose files could end up outside the download folder.
fn vet_listing(download_dir: &Path, listing: &ListOnlyResponse) -> Result<()> {
    // Every name must be safe on this operating system.
    safety::check_torrent(&listing.info, Platform::current())?;

    // And, as a second, independent check: the folder the engine chose must be inside the
    // download folder, and every file path must be a plain relative path under it.
    if !listing.output_folder.starts_with(download_dir) {
        return Err(safety::UnsafePath::outside_download_folder(&listing.output_folder).into());
    }
    for file in listing.info.iter_file_details() {
        safety::join_inside(&listing.output_folder, &file.filename.to_pathbuf())?;
    }
    Ok(())
}

fn snapshot_of(entry: &Entry) -> DownloadSnapshot {
    let mut snapshot = DownloadSnapshot {
        id: entry.id,
        name: entry.label.clone(),
        status: DownloadStatus::GettingInfo,
        progress_bytes: 0,
        total_bytes: 0,
        download_bytes_per_second: 0,
        upload_bytes_per_second: 0,
        eta: None,
        peers_connected: 0,
        peers_known: 0,
        uploaded_bytes: 0,
        waiting_for: None,
        save_folder: None,
        error: None,
    };

    let state = lock(&entry.state);
    match &*state {
        EntryState::GettingInfo => {
            snapshot.waiting_for = Some(entry.added.elapsed());
        }
        EntryState::Failed(message) => {
            snapshot.status = DownloadStatus::Failed;
            snapshot.error = Some(message.clone());
        }
        EntryState::Active(handle) => {
            let stats = handle.stats();
            if let Some(name) = handle.name().and_then(|n| magnet::clean_display_name(&n)) {
                snapshot.name = name;
            }
            snapshot.progress_bytes = stats.progress_bytes;
            snapshot.total_bytes = stats.total_bytes;
            snapshot.uploaded_bytes = stats.uploaded_bytes;
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
                TorrentStatsState::Initializing { .. } => DownloadStatus::Checking,
                TorrentStatsState::Live if stats.finished => DownloadStatus::Finished,
                TorrentStatsState::Live => DownloadStatus::Downloading,
                TorrentStatsState::Paused => DownloadStatus::Paused,
                TorrentStatsState::Error => DownloadStatus::Failed,
            };
            if snapshot.status == DownloadStatus::Failed {
                snapshot.error = Some(
                    stats
                        .error
                        .unwrap_or_else(|| "the download stopped with an error".to_string()),
                );
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
