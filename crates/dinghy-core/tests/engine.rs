//! End-to-end tests of the download engine. They need NO internet: a pretend "seeder" runs inside
//! the test on this computer, and the engine downloads from it over the loopback connection.
//!
//! Two kinds of test live here:
//! * a real download of a small multi-file torrent, checked byte for byte;
//! * hostile torrents (dangerous file names) that must be refused with NOTHING written to disk.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dinghy_core::{DownloadSnapshot, DownloadStatus, Engine, EngineConfig};
use librqbit::spawn_utils::BlockingSpawner;
use librqbit::storage::StorageFactoryExt;
use librqbit::storage::examples::inmemory::InMemoryExampleStorageFactory;
use librqbit::{
    AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerMode, ListenerOptions, Session,
    SessionOptions, create_torrent,
};

const TIMEOUT: Duration = Duration::from_secs(60);

/// A small, repeatable stream of bytes (so the test files aren't all zeros).
fn pseudo_random_bytes(len: usize, seed: u32) -> Vec<u8> {
    let mut x: u32 = 0x9E37_79B9 ^ seed;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 8) as u8
        })
        .collect()
}

/// A pretend person sharing a torrent from this computer.
struct Seeder {
    // Kept alive so the seeder keeps running until the test ends.
    _session: Arc<Session>,
    address: SocketAddr,
    info_hash: String,
}

impl Seeder {
    fn magnet(&self) -> String {
        format!("magnet:?xt=urn:btih:{}", self.info_hash)
    }
}

async fn start_session(scratch: &Path, in_memory: bool) -> Arc<Session> {
    Session::new_with_opts(
        scratch.to_path_buf(),
        SessionOptions {
            dht: None,
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpOnly,
                listen_addr: (Ipv4Addr::LOCALHOST, 0).into(),
                ..Default::default()
            }),
            disable_local_service_discovery: true,
            default_storage_factory: in_memory
                .then(|| InMemoryExampleStorageFactory::default().boxed()),
            ..Default::default()
        },
    )
    .await
    .expect("seeder session should start")
}

/// Shares the real files in `source_dir`, and waits until it holds all of them.
async fn real_seeder(scratch: &Path, source_dir: &Path) -> Seeder {
    let created = create_torrent(
        source_dir,
        CreateTorrentOptions {
            piece_length: Some(32 * 1024),
            ..Default::default()
        },
        &BlockingSpawner::new(1),
    )
    .await
    .expect("torrent should be created");

    let session = start_session(scratch, false).await;
    let handle = session
        .add_torrent(
            AddTorrent::from_bytes(created.as_bytes().unwrap().to_vec()),
            Some(AddTorrentOptions {
                overwrite: true,
                output_folder: Some(source_dir.to_string_lossy().into_owned()),
                ..Default::default()
            }),
        )
        .await
        .expect("seeder should add the torrent")
        .into_handle()
        .expect("a handle");
    tokio::time::timeout(TIMEOUT, handle.wait_until_completed())
        .await
        .expect("seeder timed out checking its files")
        .expect("seeder failed");

    Seeder {
        address: session.listen_addr().expect("seeder is listening"),
        info_hash: created.info_hash().as_string(),
        _session: session,
    }
}

/// Serves ONLY a file list (no file data, nothing on disk), for a hostile torrent.
async fn hostile_seeder(scratch: &Path, first_path_part: &str) -> Seeder {
    // A tiny one-file torrent whose file path starts with `first_path_part`. Written by hand in
    // the BitTorrent file format ("bencode"): d = dictionary, l = list, i5e = the number 5.
    let bytes = format!(
        "d4:infod5:filesld6:lengthi5e4:pathl{}:{}1:xeee4:name4:evil12:piece lengthi16384e6:pieces20:AAAAAAAAAAAAAAAAAAAAee",
        first_path_part.len(),
        first_path_part
    )
    .into_bytes();

    let session = start_session(scratch, true).await;
    let handle = session
        .add_torrent(
            AddTorrent::from_bytes(bytes),
            Some(AddTorrentOptions {
                overwrite: true,
                ..Default::default()
            }),
        )
        .await
        .expect("the seeder's own engine accepts this torrent; only BitDinghy's guard refuses it")
        .into_handle()
        .expect("a handle");
    tokio::time::timeout(TIMEOUT, handle.wait_until_initialized())
        .await
        .expect("hostile seeder timed out")
        .expect("hostile seeder failed to start");

    Seeder {
        address: session.listen_addr().expect("seeder is listening"),
        info_hash: handle.info_hash().as_string(),
        _session: session,
    }
}

/// An engine that never touches the internet: no DHT, no local-network search, no listening.
async fn offline_engine(download_dir: &Path) -> Engine {
    let mut config = EngineConfig::new(download_dir);
    config.use_dht = false;
    config.accept_incoming = false;
    config.local_discovery = false;
    Engine::start(config).await.expect("engine should start")
}

/// Waits until the first download reaches one of `wanted`, then returns its snapshot.
async fn wait_for_status(engine: &Engine, wanted: &[DownloadStatus]) -> DownloadSnapshot {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(snapshot) = engine.snapshot().into_iter().next()
            && wanted.contains(&snapshot.status)
        {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "timed out; last snapshot: {:?}",
            engine.snapshot()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn entries_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn downloads_a_torrent_and_every_byte_matches() {
    let work = tempfile::tempdir().unwrap();
    let source = work.path().join("source").join("testdata");
    std::fs::create_dir_all(source.join("sub")).unwrap();
    let files = [
        ("a.bin", pseudo_random_bytes(100_000, 1)),
        ("sub/b.bin", pseudo_random_bytes(70_001, 2)),
        ("c.txt", b"a very small file".to_vec()),
    ];
    for (name, bytes) in &files {
        std::fs::write(source.join(name), bytes).unwrap();
    }

    let seeder = real_seeder(&work.path().join("seeder-scratch"), &source).await;

    let download_dir = work.path().join("downloads");
    let engine = offline_engine(&download_dir).await;
    engine
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .expect("the magnet link is valid");

    let done = wait_for_status(&engine, &[DownloadStatus::Finished, DownloadStatus::Failed]).await;
    assert_eq!(done.status, DownloadStatus::Finished, "{:?}", done.error);
    assert_eq!(done.name, "testdata");
    assert_eq!(done.progress_bytes, done.total_bytes);
    assert_eq!(done.total_bytes, 100_000 + 70_001 + 17);

    // Every file arrived, byte for byte, inside the download folder.
    let saved = done.save_folder.expect("save folder is known");
    assert!(saved.starts_with(&download_dir));
    for (name, bytes) in &files {
        assert_eq!(&std::fs::read(saved.join(name)).unwrap(), bytes, "{name}");
    }
    // ...and nothing else was created next to it.
    assert_eq!(entries_in(&download_dir).len(), 1);

    engine.shutdown().await;
}

/// The window is not an async program: it starts the engine inside a runtime it owns, then calls
/// `add_magnet` and `snapshot` from its ordinary UI thread, with no async context at all. This
/// test does exactly that, so a mistake like "needs to be called inside a runtime" shows up here
/// and not as a crash when someone opens the window.
#[test]
fn works_when_driven_from_a_plain_thread_like_the_window_does() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(3)
        .enable_all()
        .build()
        .unwrap();

    let work = tempfile::tempdir().unwrap();
    let source = work.path().join("source").join("plain");
    std::fs::create_dir_all(&source).unwrap();
    let bytes = pseudo_random_bytes(50_000, 7);
    std::fs::write(source.join("data.bin"), &bytes).unwrap();

    let seeder = runtime.block_on(real_seeder(&work.path().join("seeder-scratch"), &source));
    let download_dir = work.path().join("downloads");
    let engine = runtime.block_on(offline_engine(&download_dir));

    // From here on, this thread is NOT inside the runtime, like a window's UI thread.
    engine
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    let done = loop {
        let snapshot = engine.snapshot().into_iter().next().unwrap();
        if matches!(
            snapshot.status,
            DownloadStatus::Finished | DownloadStatus::Failed
        ) {
            break snapshot;
        }
        assert!(Instant::now() < deadline, "timed out: {snapshot:?}");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(done.status, DownloadStatus::Finished, "{:?}", done.error);
    let saved = done.save_folder.unwrap();
    assert_eq!(std::fs::read(saved.join("data.bin")).unwrap(), bytes);

    runtime.block_on(engine.shutdown());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn adding_the_same_link_twice_shows_one_download() {
    let work = tempfile::tempdir().unwrap();
    let engine = offline_engine(&work.path().join("downloads")).await;
    let link = "magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056";
    let first = engine.add_magnet(link, vec![]).unwrap();
    let second = engine.add_magnet(link, vec![]).unwrap();
    assert_eq!(first, second);
    assert_eq!(engine.snapshot().len(), 1);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_bad_link_is_refused_straight_away() {
    let work = tempfile::tempdir().unwrap();
    let engine = offline_engine(&work.path().join("downloads")).await;
    for bad in ["", "hello", "https://example.org/a.torrent", "magnet:?dn=x"] {
        assert!(engine.add_magnet(bad, vec![]).is_err(), "{bad:?}");
    }
    assert!(engine.snapshot().is_empty());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hostile_torrents_are_refused_and_nothing_is_written() {
    // Names that are dangerous (or impossible to save) on every system...
    let mut hostile_names = vec!["bad\u{7}name"];
    // ...and ones that are dangerous on Windows. They are only created in memory here, so it is
    // safe to try them on any computer, but they are only *refused* where they matter.
    if cfg!(windows) {
        hostile_names.extend(["C:evil", "NUL", "CON.txt", "trailing.", "stream:name"]);
    }

    for name in hostile_names {
        let work = tempfile::tempdir().unwrap();
        let seeder = hostile_seeder(&work.path().join("seeder-scratch"), name).await;

        let download_dir = work.path().join("downloads");
        let engine = offline_engine(&download_dir).await;
        engine
            .add_magnet(&seeder.magnet(), vec![seeder.address])
            .unwrap();

        let result = wait_for_status(
            &engine,
            &[DownloadStatus::Failed, DownloadStatus::Downloading],
        )
        .await;
        assert_eq!(
            result.status,
            DownloadStatus::Failed,
            "{name:?} should have been refused"
        );
        let message = result.error.unwrap();
        assert!(
            message.contains("isn't safe to save"),
            "unexpected message for {name:?}: {message}"
        );
        assert!(
            entries_in(&download_dir).is_empty(),
            "{name:?}: files were created in {download_dir:?}"
        );

        engine.shutdown().await;
    }
}
