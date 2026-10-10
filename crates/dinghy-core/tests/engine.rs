//! End-to-end tests of the download engine. They need NO internet: a pretend "seeder" runs inside
//! the test on this computer, and the engine downloads from it over the loopback connection.
//!
//! Two kinds of test live here:
//! * a real download of a small multi-file torrent, checked byte for byte;
//! * hostile torrents (dangerous file names) that must be refused with NOTHING written to disk.

use std::collections::BTreeSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dinghy_core::owned_files::is_hidden;
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
    /// The `.torrent` file the seeder shares.
    torrent: Vec<u8>,
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
        torrent: created.as_bytes().unwrap().to_vec(),
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
    memory_seeder(scratch, bytes).await
}

/// Serves the file list in `bytes` (a hand-written `.torrent`) from memory, writing nothing.
async fn memory_seeder(scratch: &Path, bytes: Vec<u8>) -> Seeder {
    let session = start_session(scratch, true).await;
    let handle = session
        .add_torrent(
            AddTorrent::from_bytes(bytes.clone()),
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
        torrent: bytes,
        _session: session,
    }
}

/// Settings for an engine that never touches the internet: no DHT, no local-network search, no
/// listening, and no saved list.
fn offline_config(download_dir: &Path) -> EngineConfig {
    let mut config = EngineConfig::new(download_dir);
    config.use_dht = false;
    config.accept_incoming = false;
    config.local_discovery = false;
    config
}

async fn offline_engine(download_dir: &Path) -> Engine {
    Engine::start(offline_config(download_dir))
        .await
        .expect("engine should start")
}

/// An offline engine that remembers its downloads in `list`. `restore` brings back what is
/// already in the list (like the window does).
async fn remembering_engine(download_dir: &Path, list: &Path, restore: bool) -> Engine {
    let mut config = offline_config(download_dir);
    config.saved_list_file = Some(list.to_path_buf());
    config.restore_saved = restore;
    Engine::start(config).await.expect("engine should start")
}

/// A small one-folder torrent with real files, shared by a seeder on this computer.
async fn small_shared_torrent(work: &Path) -> (Seeder, Vec<(&'static str, Vec<u8>)>) {
    let source = work.join("source").join("testdata");
    std::fs::create_dir_all(&source).unwrap();
    let files = vec![
        ("a.bin", pseudo_random_bytes(60_000, 11)),
        ("b.bin", pseudo_random_bytes(40_000, 12)),
    ];
    for (name, bytes) in &files {
        std::fs::write(source.join(name), bytes).unwrap();
    }
    let seeder = real_seeder(&work.join("seeder-scratch"), &source).await;
    (seeder, files)
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

/// Waits until the first download satisfies `wanted`, then returns its snapshot.
async fn wait_until(
    engine: &Engine,
    what: &str,
    wanted: impl Fn(&DownloadSnapshot) -> bool,
) -> DownloadSnapshot {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(snapshot) = engine.snapshot().into_iter().next()
            && wanted(&snapshot)
        {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; last snapshot: {:?}",
            engine.snapshot()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Really paused: the torrent has started (its folder is known) and the engine has paused it.
fn really_paused(snapshot: &DownloadSnapshot) -> bool {
    snapshot.status == DownloadStatus::Paused && snapshot.save_folder.is_some()
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

// ---------------------------------------------------------------------------------------------
// The saved list (M3): downloads are remembered and come back after a restart.
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_finished_download_is_remembered_and_comes_back_after_a_restart() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, files) = small_shared_torrent(work.path()).await;
    let download_dir = work.path().join("downloads");
    let list = work.path().join("data").join("downloads.db");
    std::fs::create_dir_all(list.parent().unwrap()).unwrap();

    let first_run = remembering_engine(&download_dir, &list, true).await;
    first_run
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    let done = wait_for_status(
        &first_run,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!(done.status, DownloadStatus::Finished, "{:?}", done.error);
    first_run.shutdown().await;
    drop(first_run); // "BitDinghy closes"

    // The list holds the checked file list, so the next start needs nobody to ask.
    let saved = dinghy_core::SavedList::read(&list).unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "testdata");
    assert!(saved[0].torrent.is_some());
    assert_eq!(
        saved[0].output_folder.as_deref(),
        done.save_folder.as_deref()
    );

    // "BitDinghy opens again", with no seeder to ask: everything comes from the list and the disk.
    let second_run = remembering_engine(&download_dir, &list, true).await;
    let back = wait_for_status(
        &second_run,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!(back.status, DownloadStatus::Finished, "{:?}", back.error);
    assert_eq!(back.name, "testdata");
    assert_eq!(back.save_folder, done.save_folder);
    let folder = back.save_folder.unwrap();
    for (name, bytes) in &files {
        assert_eq!(&std::fs::read(folder.join(name)).unwrap(), bytes, "{name}");
    }
    second_run.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_remembered_download_stays_in_its_own_folder_when_the_setting_changes() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, _files) = small_shared_torrent(work.path()).await;
    let old_folder = work.path().join("old-downloads");
    let new_folder = work.path().join("new-downloads");
    let list = work.path().join("downloads.db");

    let first_run = remembering_engine(&old_folder, &list, true).await;
    first_run
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    let done = wait_for_status(
        &first_run,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!(done.status, DownloadStatus::Finished, "{:?}", done.error);
    first_run.shutdown().await;
    drop(first_run);

    // The person picked a new download folder in the meantime.
    let second_run = remembering_engine(&new_folder, &list, true).await;
    let back = wait_for_status(
        &second_run,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!(back.status, DownloadStatus::Finished, "{:?}", back.error);
    assert!(back.save_folder.unwrap().starts_with(&old_folder));
    assert_eq!(second_run.download_dir_of(back.id), Some(old_folder));
    assert!(
        entries_in(&new_folder).is_empty(),
        "nothing should be created in the new folder"
    );
    second_run.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn running_dinghy_get_again_carries_on_from_the_saved_file_list() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, _files) = small_shared_torrent(work.path()).await;
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");

    let first_run = remembering_engine(&download_dir, &list, false).await;
    first_run
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    wait_for_status(&first_run, &[DownloadStatus::Finished]).await;
    first_run.shutdown().await;
    drop(first_run);

    // Same link again, and this time no seeder address: the saved file list is enough.
    let second_run = remembering_engine(&download_dir, &list, false).await;
    assert!(
        second_run.snapshot().is_empty(),
        "`get` doesn't restore others"
    );
    second_run.add_magnet(&seeder.magnet(), vec![]).unwrap();
    let back = wait_for_status(
        &second_run,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!(back.status, DownloadStatus::Finished, "{:?}", back.error);
    assert_eq!(dinghy_core::SavedList::read(&list).unwrap().len(), 1);
    second_run.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_download_still_waiting_for_its_file_list_is_remembered_too() {
    let work = tempfile::tempdir().unwrap();
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");
    let link = "magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056&dn=Waiting%20Example";

    let first_run = remembering_engine(&download_dir, &list, true).await;
    first_run.add_magnet(link, vec![]).unwrap();
    first_run.shutdown().await;
    drop(first_run);

    let saved = dinghy_core::SavedList::read(&list).unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "Waiting Example");
    assert!(saved[0].torrent.is_none());

    let second_run = remembering_engine(&download_dir, &list, true).await;
    let shown = second_run.snapshot();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].name, "Waiting Example");
    second_run.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_refused_torrent_is_not_remembered() {
    let work = tempfile::tempdir().unwrap();
    let seeder = hostile_seeder(&work.path().join("seeder-scratch"), "bad\u{7}name").await;
    let list = work.path().join("downloads.db");
    let engine = remembering_engine(&work.path().join("downloads"), &list, true).await;
    engine
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    let result = wait_for_status(
        &engine,
        &[DownloadStatus::Failed, DownloadStatus::Downloading],
    )
    .await;
    assert_eq!(result.status, DownloadStatus::Failed);
    assert!(dinghy_core::SavedList::read(&list).unwrap().is_empty());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_one_engine_can_use_a_saved_list_at_a_time() {
    let work = tempfile::tempdir().unwrap();
    let list = work.path().join("downloads.db");
    let window = remembering_engine(&work.path().join("a"), &list, true).await;

    let mut second = offline_config(&work.path().join("b"));
    second.saved_list_file = Some(list.clone());
    let refused = Engine::start(second.clone()).await;
    assert!(matches!(refused, Err(dinghy_core::Error::AlreadyRunning)));

    // Closing the first one (even before it is dropped) lets the next one in.
    window.shutdown().await;
    let next = Engine::start(second).await.expect("the list is free again");
    next.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Pause and resume (M3 step 2).
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_paused_download_stays_paused_after_a_restart_and_resumes() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, _files) = small_shared_torrent(work.path()).await;
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");

    let first_run = remembering_engine(&download_dir, &list, true).await;
    let id = first_run
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    wait_for_status(&first_run, &[DownloadStatus::Finished]).await;
    first_run.pause(id).await.unwrap();
    wait_until(&first_run, "paused", really_paused).await;
    assert!(dinghy_core::SavedList::read(&list).unwrap()[0].paused);
    first_run.shutdown().await;
    drop(first_run);

    // After a restart it comes back paused (its files are checked first, then it stays paused).
    let second_run = remembering_engine(&download_dir, &list, true).await;
    let back = wait_until(&second_run, "paused after restart", really_paused).await;
    second_run.resume(back.id).await.unwrap();
    let done = wait_for_status(
        &second_run,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!(done.status, DownloadStatus::Finished, "{:?}", done.error);
    assert!(!dinghy_core::SavedList::read(&list).unwrap()[0].paused);
    second_run.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_download_can_be_paused_before_its_file_list_arrives() {
    let work = tempfile::tempdir().unwrap();
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");
    // Nobody can be asked for this one (this engine is offline), so its file list never arrives:
    // the attempt fails at once or keeps waiting. Either way, pausing makes it simply "paused".
    let link = "magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056&dn=Nobody%20Has%20It";

    let engine = remembering_engine(&download_dir, &list, true).await;
    let id = engine.add_magnet(link, vec![]).unwrap();
    engine.pause(id).await.unwrap();
    assert_eq!(engine.snapshot()[0].status, DownloadStatus::Paused);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        engine.snapshot()[0].status,
        DownloadStatus::Paused,
        "a failed attempt must not hide the pause"
    );

    engine.resume(id).await.unwrap();
    let after_resume = engine.snapshot()[0].status;
    assert!(
        matches!(
            after_resume,
            DownloadStatus::GettingInfo | DownloadStatus::Failed
        ),
        "{after_resume:?}"
    );
    engine.pause(id).await.unwrap();
    engine.shutdown().await;
    drop(engine);

    let saved = dinghy_core::SavedList::read(&list).unwrap();
    assert!(saved[0].paused);
    // After a restart it is paused straight away, and nobody is contacted for it.
    let restarted = remembering_engine(&download_dir, &list, true).await;
    assert_eq!(restarted.snapshot()[0].status, DownloadStatus::Paused);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(restarted.snapshot()[0].status, DownloadStatus::Paused);
    restarted.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pausing_right_after_adding_starts_the_download_paused() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, _files) = small_shared_torrent(work.path()).await;
    let engine = offline_engine(&work.path().join("downloads")).await;
    let id = engine
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    // The file list is still on its way: the pause must not get lost.
    engine.pause(id).await.unwrap();
    wait_until(&engine, "paused", really_paused).await;
    // And it stays paused.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(engine.snapshot()[0].status, DownloadStatus::Paused);
    engine.resume(id).await.unwrap();
    let done = wait_for_status(&engine, &[DownloadStatus::Finished, DownloadStatus::Failed]).await;
    assert_eq!(done.status, DownloadStatus::Finished, "{:?}", done.error);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resume_tries_a_failed_download_again() {
    let work = tempfile::tempdir().unwrap();
    let seeder = hostile_seeder(&work.path().join("seeder-scratch"), "bad\u{7}name").await;
    let engine = offline_engine(&work.path().join("downloads")).await;
    let id = engine
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    wait_for_status(&engine, &[DownloadStatus::Failed]).await;

    engine.resume(id).await.unwrap();
    // It starts again from the beginning...
    assert!(engine.snapshot()[0].waiting_for.is_some());
    // ...and the guard refuses it again, as it should.
    let again = wait_for_status(&engine, &[DownloadStatus::Failed]).await;
    assert!(again.error.unwrap().contains("isn't safe to save"));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn asking_for_a_paused_download_again_resumes_it() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, _files) = small_shared_torrent(work.path()).await;
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");

    let first_run = remembering_engine(&download_dir, &list, true).await;
    let id = first_run
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    wait_for_status(&first_run, &[DownloadStatus::Finished]).await;
    first_run.pause(id).await.unwrap();
    first_run.shutdown().await;
    drop(first_run);

    // Like `dinghy get` with the same link: it means "carry on".
    let second_run = remembering_engine(&download_dir, &list, false).await;
    second_run.add_magnet(&seeder.magnet(), vec![]).unwrap();
    let done = wait_for_status(
        &second_run,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!(done.status, DownloadStatus::Finished, "{:?}", done.error);
    assert!(!dinghy_core::SavedList::read(&list).unwrap()[0].paused);
    second_run.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Remove, and never overwriting someone's file (M3 step 3).
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remove_forgets_the_download_and_keeps_its_files() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, files) = small_shared_torrent(work.path()).await;
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");

    let engine = remembering_engine(&download_dir, &list, true).await;
    let id = engine
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    let done = wait_for_status(&engine, &[DownloadStatus::Finished]).await;
    let deleted = engine.remove(id, false).await.unwrap();
    assert_eq!(deleted.files, 0);
    assert!(engine.snapshot().is_empty());
    assert!(dinghy_core::SavedList::read(&list).unwrap().is_empty());
    let folder = done.save_folder.unwrap();
    for (name, bytes) in &files {
        assert_eq!(&std::fs::read(folder.join(name)).unwrap(), bytes, "{name}");
    }
    engine.shutdown().await;
    drop(engine);

    // Gone after a restart too.
    let restarted = remembering_engine(&download_dir, &list, true).await;
    assert!(restarted.snapshot().is_empty());
    restarted.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remove_with_delete_files_deletes_exactly_its_own_files() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, files) = small_shared_torrent(work.path()).await;
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");

    let first_run = remembering_engine(&download_dir, &list, true).await;
    first_run
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    wait_for_status(&first_run, &[DownloadStatus::Finished]).await;
    first_run.shutdown().await;
    drop(first_run);

    // Someone else's file right next to the download.
    let neighbour = download_dir.join("my own notes.txt");
    std::fs::write(&neighbour, b"not BitDinghy's").unwrap();

    // Removing after a restart: the file list comes from the saved list.
    let engine = remembering_engine(&download_dir, &list, true).await;
    let back = wait_for_status(&engine, &[DownloadStatus::Finished]).await;
    let folder = back.save_folder.unwrap();
    let deleted = engine.remove(back.id, true).await.unwrap();
    assert_eq!(deleted.files, files.len(), "{:?}", deleted.problems);
    assert!(deleted.problems.is_empty(), "{:?}", deleted.problems);
    assert!(!folder.exists(), "the torrent's own folder is gone");
    assert!(download_dir.is_dir(), "the download folder stays");
    assert_eq!(std::fs::read(&neighbour).unwrap(), b"not BitDinghy's");
    assert!(dinghy_core::SavedList::read(&list).unwrap().is_empty());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_download_never_overwrites_an_existing_file() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, files) = small_shared_torrent(work.path()).await;
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");

    // A file of the person's own, exactly where the torrent wants to write one.
    let theirs = download_dir.join("testdata").join("a.bin");
    std::fs::create_dir_all(theirs.parent().unwrap()).unwrap();
    std::fs::write(&theirs, b"precious").unwrap();

    let engine = remembering_engine(&download_dir, &list, true).await;
    let id = engine
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    let refused =
        wait_for_status(&engine, &[DownloadStatus::Failed, DownloadStatus::Finished]).await;
    assert_eq!(refused.status, DownloadStatus::Failed);
    let message = refused.error.unwrap();
    assert!(message.contains("same name"), "{message}");
    assert!(message.contains("a.bin"), "{message}");
    assert_eq!(std::fs::read(&theirs).unwrap(), b"precious");
    assert!(!download_dir.join("testdata").join("b.bin").exists());
    // It stays in the list, so the person can fix it and try again.
    assert_eq!(dinghy_core::SavedList::read(&list).unwrap().len(), 1);

    std::fs::rename(&theirs, work.path().join("moved-away.bin")).unwrap();
    engine.resume(id).await.unwrap();
    let done = wait_for_status(&engine, &[DownloadStatus::Finished, DownloadStatus::Failed]).await;
    assert_eq!(done.status, DownloadStatus::Finished, "{:?}", done.error);
    assert_eq!(std::fs::read(&theirs).unwrap(), files[0].1);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn removing_a_download_that_is_still_starting_leaves_nothing_behind() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, _files) = small_shared_torrent(work.path()).await;
    let list = work.path().join("downloads.db");
    let engine = remembering_engine(&work.path().join("downloads"), &list, true).await;
    let id = engine
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    engine.remove(id, false).await.unwrap();
    tokio::time::sleep(Duration::from_millis(1000)).await;
    assert!(engine.snapshot().is_empty());
    assert!(dinghy_core::SavedList::read(&list).unwrap().is_empty());
    // Adding it again works normally.
    engine
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    let done = wait_for_status(&engine, &[DownloadStatus::Finished, DownloadStatus::Failed]).await;
    assert_eq!(done.status, DownloadStatus::Finished, "{:?}", done.error);
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Security review fixes (M3): a download never moves into another folder on a restart.
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn several_files_without_a_folder_name_are_refused() {
    // The attack from the review: no `name`, so the engine would name the folder after the magnet
    // link's `dn=` the first time and after the biggest file the next time.
    let work = tempfile::tempdir().unwrap();
    let nameless = b"d4:infod5:filesld6:lengthi5e4:pathl11:Release.bineed6:lengthi1e4:pathl10:family.jpgeee12:piece lengthi16384e6:pieces20:AAAAAAAAAAAAAAAAAAAAee".to_vec();
    let seeder = memory_seeder(&work.path().join("seeder-scratch"), nameless).await;

    let download_dir = work.path().join("downloads");
    let victim = download_dir.join("Release").join("family.jpg");
    std::fs::create_dir_all(victim.parent().unwrap()).unwrap();
    std::fs::write(&victim, b"the real photo").unwrap();
    let list = work.path().join("downloads.db");

    let engine = remembering_engine(&download_dir, &list, true).await;
    engine
        .add_magnet(
            &format!("{}&dn=Anything", seeder.magnet()),
            vec![seeder.address],
        )
        .unwrap();
    let refused = wait_for_status(
        &engine,
        &[DownloadStatus::Failed, DownloadStatus::Downloading],
    )
    .await;
    assert_eq!(refused.status, DownloadStatus::Failed);
    let message = refused.error.unwrap();
    assert!(message.contains("no name for their folder"), "{message}");
    assert_eq!(std::fs::read(&victim).unwrap(), b"the real photo");
    assert!(!download_dir.join("Anything").exists());
    assert!(dinghy_core::SavedList::read(&list).unwrap().is_empty());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_continuing_download_always_uses_its_saved_folder() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, files) = small_shared_torrent(work.path()).await;
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");

    let first_run = remembering_engine(&download_dir, &list, true).await;
    first_run
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    wait_for_status(&first_run, &[DownloadStatus::Finished]).await;
    first_run.shutdown().await;
    drop(first_run);

    // Its saved folder is now somewhere the engine itself would never choose for this torrent
    // (the engine would pick `downloads/testdata`).
    let kept_here = download_dir.join("kept-here");
    std::fs::rename(download_dir.join("testdata"), &kept_here).unwrap();
    {
        let opened = dinghy_core::SavedList::open(&list).unwrap();
        let saved = opened.list.all().unwrap().remove(0);
        opened
            .list
            .record_file_list(
                &saved.info_hash,
                &saved.name,
                saved.torrent.as_deref().unwrap(),
                &kept_here,
            )
            .unwrap();
    }

    let second_run = remembering_engine(&download_dir, &list, true).await;
    let back = wait_for_status(
        &second_run,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!(back.status, DownloadStatus::Finished, "{:?}", back.error);
    assert_eq!(back.save_folder.as_deref(), Some(kept_here.as_path()));
    assert!(
        !download_dir.join("testdata").exists(),
        "it must not move to the engine's own choice of folder"
    );
    for (name, bytes) in &files {
        assert_eq!(
            &std::fs::read(kept_here.join(name)).unwrap(),
            bytes,
            "{name}"
        );
    }
    assert_eq!(
        dinghy_core::SavedList::read(&list).unwrap()[0]
            .output_folder
            .as_deref(),
        Some(kept_here.as_path())
    );
    second_run.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_saved_folder_with_the_wrong_shape_is_refused() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, _files) = small_shared_torrent(work.path()).await;
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");

    let first_run = remembering_engine(&download_dir, &list, true).await;
    first_run
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    wait_for_status(&first_run, &[DownloadStatus::Finished]).await;
    first_run.shutdown().await;
    drop(first_run);

    // A damaged (or tampered) list pointing above the download folder.
    {
        let opened = dinghy_core::SavedList::open(&list).unwrap();
        let saved = opened.list.all().unwrap().remove(0);
        opened
            .list
            .record_file_list(
                &saved.info_hash,
                &saved.name,
                saved.torrent.as_deref().unwrap(),
                &download_dir.join(".."),
            )
            .unwrap();
    }
    let second_run = remembering_engine(&download_dir, &list, true).await;
    let refused = wait_for_status(
        &second_run,
        &[DownloadStatus::Failed, DownloadStatus::Finished],
    )
    .await;
    assert_eq!(refused.status, DownloadStatus::Failed);
    assert!(
        !work.path().join("a.bin").exists(),
        "nothing written above the folder"
    );
    second_run.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// .torrent files (M5).
// ---------------------------------------------------------------------------------------------

/// The same torrent as `bytes`, with these trackers added around its file list.
fn with_trackers(bytes: &[u8], trackers: &[&str]) -> Vec<u8> {
    let parsed = librqbit::torrent_from_bytes(bytes).unwrap();
    let text = |value: &str| format!("{}:{value}", value.len());
    let mut out = format!(
        "d{}{}{}l",
        text("announce"),
        text(trackers[0]),
        text("announce-list")
    );
    for tracker in trackers {
        out += &format!("l{}e", text(tracker));
    }
    out += &format!("e{}", text("info"));
    let mut out = out.into_bytes();
    out.extend_from_slice(parsed.info.raw_bytes.0);
    out.push(b'e');
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_torrent_file_downloads_and_its_local_trackers_are_dropped() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, files) = small_shared_torrent(work.path()).await;
    // A tracker on the home network: it must never be contacted, and never be remembered.
    let torrent_file = work.path().join("testdata.torrent");
    std::fs::write(
        &torrent_file,
        with_trackers(&seeder.torrent, &["http://192.168.1.10:6969/announce"]),
    )
    .unwrap();
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");

    let engine = remembering_engine(&download_dir, &list, true).await;
    engine
        .add(&torrent_file.display().to_string(), vec![seeder.address])
        .unwrap();
    let done = wait_for_status(&engine, &[DownloadStatus::Finished, DownloadStatus::Failed]).await;
    assert_eq!(done.status, DownloadStatus::Finished, "{:?}", done.error);
    assert_eq!(done.info_hash, seeder.info_hash);
    let folder = done.save_folder.unwrap();
    for (name, bytes) in &files {
        assert_eq!(&std::fs::read(folder.join(name)).unwrap(), bytes, "{name}");
    }

    // What was remembered is the rebuilt file: same torrent, no home-network tracker.
    let saved = dinghy_core::SavedList::read(&list).unwrap();
    let remembered = saved[0].torrent.as_deref().unwrap();
    let parsed = librqbit::torrent_from_bytes(remembered).unwrap();
    assert_eq!(parsed.info_hash.as_string(), seeder.info_hash);
    assert_eq!(parsed.iter_announce().count(), 0);
    assert!(!saved[0].magnet.contains("192.168"));
    engine.shutdown().await;
    drop(engine);

    // And it comes back after a restart, like any other download.
    let restarted = remembering_engine(&download_dir, &list, true).await;
    let back = wait_for_status(
        &restarted,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!(back.status, DownloadStatus::Finished, "{:?}", back.error);
    restarted.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_torrent_file_and_its_magnet_link_are_one_download() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, _files) = small_shared_torrent(work.path()).await;
    let torrent_file = work.path().join("testdata.torrent");
    std::fs::write(&torrent_file, &seeder.torrent).unwrap();
    let engine = offline_engine(&work.path().join("downloads")).await;
    let first = engine
        .add(&torrent_file.display().to_string(), vec![seeder.address])
        .unwrap();
    let second = engine.add(&seeder.magnet(), vec![seeder.address]).unwrap();
    assert_eq!(first, second);
    assert_eq!(engine.snapshot().len(), 1);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hostile_torrent_file_is_refused_and_nothing_is_written() {
    let work = tempfile::tempdir().unwrap();
    let torrent_file = work.path().join("evil.torrent");
    // A file name that is impossible to save safely on any system (a control character).
    std::fs::write(
        &torrent_file,
        // (In this format `l` ... `e` is a list: the path list, the file's dictionary and the
        // list of files each need their own closing `e`.)
        "d4:infod5:filesld6:lengthi5e4:pathl8:bad\u{7}nameeee4:name4:evil12:piece lengthi16384e6:pieces20:AAAAAAAAAAAAAAAAAAAAee",
    )
    .unwrap();
    let download_dir = work.path().join("downloads");
    let engine = offline_engine(&download_dir).await;
    engine
        .add(&torrent_file.display().to_string(), vec![])
        .unwrap();
    let refused = wait_for_status(
        &engine,
        &[DownloadStatus::Failed, DownloadStatus::Downloading],
    )
    .await;
    assert_eq!(refused.status, DownloadStatus::Failed);
    let message = refused.error.unwrap();
    assert!(message.contains("isn't safe to save"), "{message}");
    assert!(entries_in(&download_dir).is_empty());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_bad_torrent_file_is_refused_at_once() {
    let work = tempfile::tempdir().unwrap();
    let engine = offline_engine(&work.path().join("downloads")).await;
    let not_a_torrent = work.path().join("notes.torrent");
    std::fs::write(&not_a_torrent, b"just some text").unwrap();
    let error = engine
        .add(&not_a_torrent.display().to_string(), vec![])
        .unwrap_err();
    assert!(
        error.to_string().contains("isn't a valid .torrent file"),
        "{error}"
    );
    let error = engine
        .add(
            &work.path().join("missing.torrent").display().to_string(),
            vec![],
        )
        .unwrap_err();
    assert!(error.to_string().contains("There is no file"), "{error}");
    assert!(engine.snapshot().is_empty());
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Choosing files (M6).
// ---------------------------------------------------------------------------------------------

/// Three files over ten pieces of 32 KiB. The middle one is big, so leaving it out really means
/// most of it is never downloaded (the pieces it shares with its neighbours are, in part).
async fn three_file_torrent(work: &Path) -> (Seeder, Vec<(&'static str, Vec<u8>)>) {
    let source = work.join("source").join("three");
    std::fs::create_dir_all(&source).unwrap();
    let files = vec![
        ("a.bin", pseudo_random_bytes(60_000, 21)),
        ("b.bin", pseudo_random_bytes(200_000, 22)),
        ("c.bin", pseudo_random_bytes(40_000, 23)),
    ];
    for (name, bytes) in &files {
        std::fs::write(source.join(name), bytes).unwrap();
    }
    let seeder = real_seeder(&work.join("seeder-scratch"), &source).await;
    (seeder, files)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_the_chosen_files_are_downloaded_and_the_choice_is_remembered() {
    let work = tempfile::tempdir().unwrap();
    let (seeder, files) = three_file_torrent(work.path()).await;
    let download_dir = work.path().join("downloads");
    let list = work.path().join("downloads.db");

    // Paused straight away, so the choice is made before anything is downloaded.
    let first_run = remembering_engine(&download_dir, &list, true).await;
    let id = first_run
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    first_run.pause(id).await.unwrap();
    let paused = wait_until(&first_run, "paused with its file list", really_paused).await;
    assert_eq!((paused.files_total, paused.files_chosen), (3, 3));
    let shown = first_run.files(id).unwrap();
    let names: Vec<_> = shown.iter().map(|f| f.path.display().to_string()).collect();
    assert_eq!(names, ["a.bin", "b.bin", "c.bin"]);
    assert!(shown.iter().all(|f| f.chosen));

    // Refused: nothing chosen, and a number that isn't in the list.
    let none = first_run.choose_files(id, &BTreeSet::new()).await;
    assert!(none.unwrap_err().to_string().contains("at least one file"));
    let wrong = first_run.choose_files(id, &BTreeSet::from([1, 4])).await;
    assert!(wrong.unwrap_err().to_string().contains("no file number 4"));

    // Files 1 and 3 only.
    first_run
        .choose_files(id, &BTreeSet::from([1, 3]))
        .await
        .unwrap();
    assert_eq!(
        dinghy_core::SavedList::read(&list).unwrap()[0].skipped_files,
        BTreeSet::from([1]),
        "the torrent's own index of b.bin is remembered"
    );
    // The engine created b.bin anyway; it is hidden now (Windows has a "hidden" mark).
    let folder = paused.save_folder.clone().unwrap();
    assert!(folder.join("b.bin").exists());
    assert_eq!(is_hidden(&folder.join("b.bin")), cfg!(windows));
    assert!(!is_hidden(&folder.join("a.bin")));
    first_run.resume(id).await.unwrap();
    let done = wait_for_status(
        &first_run,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!(done.status, DownloadStatus::Finished, "{:?}", done.error);
    assert_eq!((done.files_total, done.files_chosen), (3, 2));
    assert!(
        done.total_bytes < 300_000,
        "only the chosen files' pieces count: {}",
        done.total_bytes
    );
    let folder = done.save_folder.unwrap();
    assert_eq!(std::fs::read(folder.join("a.bin")).unwrap(), files[0].1);
    assert_eq!(std::fs::read(folder.join("c.bin")).unwrap(), files[2].1);
    assert_ne!(
        std::fs::read(folder.join("b.bin")).unwrap_or_default(),
        files[1].1,
        "b.bin was not downloaded"
    );
    let shown = first_run.files(id).unwrap();
    assert_eq!(shown[0].done_bytes, Some(60_000));
    assert!(!shown[1].chosen);
    assert_eq!(shown[2].done_bytes, Some(40_000));
    first_run.shutdown().await;
    drop(first_run);

    // After a restart the choice is still there, and the download is still finished. Its start
    // hides the leftovers again (here b.bin is made visible by hand first, like a choice changed
    // with `dinghy files` while the window was closed).
    dinghy_core::owned_files::apply_hidden(&[(folder.join("b.bin"), false)]);
    let second_run = remembering_engine(&download_dir, &list, true).await;
    let back = wait_for_status(
        &second_run,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!(back.status, DownloadStatus::Finished, "{:?}", back.error);
    assert_eq!((back.files_total, back.files_chosen), (3, 2));
    let deadline = Instant::now() + TIMEOUT;
    while is_hidden(&folder.join("b.bin")) != cfg!(windows) {
        assert!(Instant::now() < deadline, "b.bin was not hidden again");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    second_run.shutdown().await;
    drop(second_run);

    // Like `dinghy get` run again (it knows where the seeder is): it carries on with the saved
    // choice. Ticking b.bin again while it runs downloads that too, straight away.
    let third_run = remembering_engine(&download_dir, &list, false).await;
    let id = third_run
        .add_magnet(&seeder.magnet(), vec![seeder.address])
        .unwrap();
    let again = wait_for_status(
        &third_run,
        &[DownloadStatus::Finished, DownloadStatus::Failed],
    )
    .await;
    assert_eq!((again.files_total, again.files_chosen), (3, 2));
    third_run
        .choose_files(id, &BTreeSet::from([1, 2, 3]))
        .await
        .unwrap();
    let all = wait_until(&third_run, "every file downloaded", |s| {
        s.status == DownloadStatus::Finished && s.files_chosen == 3 && s.total_bytes == 300_000
    })
    .await;
    for (name, bytes) in &files {
        assert_eq!(
            &std::fs::read(all.save_folder.as_ref().unwrap().join(name)).unwrap(),
            bytes,
            "{name}"
        );
    }
    assert!(
        dinghy_core::SavedList::read(&list).unwrap()[0]
            .skipped_files
            .is_empty()
    );
    assert!(!is_hidden(&folder.join("b.bin")), "ticked again: visible");

    // Unticked again, then removed keeping the files: nothing stays hidden.
    third_run
        .choose_files(id, &BTreeSet::from([1, 3]))
        .await
        .unwrap();
    assert_eq!(is_hidden(&folder.join("b.bin")), cfg!(windows));
    third_run.remove(id, false).await.unwrap();
    assert!(!is_hidden(&folder.join("b.bin")));
    assert_eq!(
        std::fs::read(folder.join("b.bin")).unwrap(),
        files[1].1,
        "kept"
    );
    third_run.shutdown().await;
}
