//! The channel between the command line and the window, over the REAL local channel (a named
//! pipe on Windows, a socket file on Linux). No internet: a pretend seeder runs on this computer.

use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dinghy_core::remote::{Address, Client, Command, Reply, start_server};
use dinghy_core::{DownloadStatus, Engine, EngineConfig};
use librqbit::spawn_utils::BlockingSpawner;
use librqbit::{
    AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerMode, ListenerOptions, Session,
    SessionOptions, create_torrent,
};

async fn offline_engine(download_dir: &Path) -> Arc<Engine> {
    let mut config = EngineConfig::new(download_dir);
    config.use_dht = false;
    config.accept_incoming = false;
    config.local_discovery = false;
    Arc::new(Engine::start(config).await.unwrap())
}

/// A pretend person sharing one small file from this computer: (session, address, magnet).
async fn seeder(work: &Path) -> (Arc<Session>, std::net::SocketAddr, String) {
    let source = work.join("source").join("shared");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("data.bin"), vec![7u8; 50_000]).unwrap();
    let created = create_torrent(
        &source,
        CreateTorrentOptions {
            piece_length: Some(32 * 1024),
            ..Default::default()
        },
        &BlockingSpawner::new(1),
    )
    .await
    .unwrap();
    let session = Session::new_with_opts(
        work.join("seeder-scratch"),
        SessionOptions {
            dht: None,
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpOnly,
                listen_addr: (Ipv4Addr::LOCALHOST, 0).into(),
                ..Default::default()
            }),
            disable_local_service_discovery: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let handle = session
        .add_torrent(
            AddTorrent::from_bytes(created.as_bytes().unwrap().to_vec()),
            Some(AddTorrentOptions {
                overwrite: true,
                output_folder: Some(source.to_string_lossy().into_owned()),
                ..Default::default()
            }),
        )
        .await
        .unwrap()
        .into_handle()
        .unwrap();
    handle.wait_until_completed().await.unwrap();
    let address = session.listen_addr().unwrap();
    let magnet = format!("magnet:?xt=urn:btih:{}", created.info_hash().as_string());
    (session, address, magnet)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_command_line_adds_a_link_to_the_window_and_follows_it() {
    let work = tempfile::tempdir().unwrap();
    let (_seeder, seeder_address, magnet) = seeder(work.path()).await;
    let window = offline_engine(&work.path().join("downloads")).await;
    let address = Address::for_data_dir(&work.path().join("data"));
    start_server(window.clone(), &address).unwrap();

    let mut client = Client::connect(&address)
        .await
        .unwrap()
        .expect("the window is listening");
    let Reply::Added { info_hash, folder } = client
        .ask(Command::Add {
            link: magnet,
            peers: vec![seeder_address],
        })
        .await
        .unwrap()
    else {
        panic!("expected Added");
    };
    assert_eq!(folder, window.download_dir());

    // Follow it through the channel, like `dinghy get` does.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let Reply::Downloads { downloads, .. } = client.ask(Command::List).await.unwrap() else {
            panic!("expected Downloads");
        };
        let ours = downloads
            .iter()
            .find(|download| download.info_hash == info_hash)
            .expect("the added download is in the window's list");
        if ours.status == DownloadStatus::Finished {
            break;
        }
        assert!(Instant::now() < deadline, "timed out: {ours:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // The window really has it: its own picture says so.
    assert_eq!(window.snapshot()[0].status, DownloadStatus::Finished);
    window.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn with_no_window_open_there_is_simply_no_one_to_talk_to() {
    let work = tempfile::tempdir().unwrap();
    let address = Address::for_data_dir(&work.path().join("nobody-here"));
    assert!(Client::connect(&address).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_second_window_cannot_take_over_the_channel() {
    let work = tempfile::tempdir().unwrap();
    let address = Address::for_data_dir(&work.path().join("data"));
    let first = offline_engine(&work.path().join("a")).await;
    start_server(first.clone(), &address).unwrap();
    let second = offline_engine(&work.path().join("b")).await;
    let refused = start_server(second.clone(), &address).unwrap_err();
    assert!(refused.to_string().contains("already in use"), "{refused}");

    // The first window still answers.
    let mut client = Client::connect(&address).await.unwrap().unwrap();
    assert!(matches!(
        client.ask(Command::List).await.unwrap(),
        Reply::Downloads { .. }
    ));
    first.shutdown().await;
    second.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn several_command_lines_at_once_are_all_answered() {
    let work = tempfile::tempdir().unwrap();
    let window = offline_engine(&work.path().join("downloads")).await;
    let address = Address::for_data_dir(&work.path().join("data"));
    start_server(window.clone(), &address).unwrap();

    let mut clients = Vec::new();
    for _ in 0..5 {
        clients.push(Client::connect(&address).await.unwrap().unwrap());
    }
    for client in &mut clients {
        assert!(matches!(
            client.ask(Command::List).await.unwrap(),
            Reply::Downloads { .. }
        ));
    }
    // A problem is reported as an error, and the conversation goes on.
    let error = clients[0]
        .ask(Command::Add {
            link: "hello".to_string(),
            peers: vec![],
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("isn't a magnet link"), "{error}");
    assert!(clients[0].ask(Command::List).await.is_ok());
    window.shutdown().await;
}
