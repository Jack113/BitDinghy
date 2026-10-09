//! Runs the real `dinghy` program, the same way a person would, and checks what it prints.

use std::net::Ipv4Addr;
use std::process::Command;

use librqbit::spawn_utils::BlockingSpawner;
use librqbit::{
    AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerMode, ListenerOptions, Session,
    SessionOptions, create_torrent,
};

/// Cargo builds the binary first and tells us where it is via `CARGO_BIN_EXE_<name>`.
fn dinghy() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dinghy"))
}

#[test]
fn version_flag_prints_the_version() {
    let output = dinghy()
        .arg("--version")
        .output()
        .expect("failed to run dinghy");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "got: {stdout}");
}

#[test]
fn running_with_no_arguments_shows_the_app_name_and_a_hint() {
    let output = dinghy().output().expect("failed to run dinghy");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BitDinghy"), "got: {stdout}");
    assert!(stdout.contains("dinghy get"), "got: {stdout}");
}

#[test]
fn get_with_something_that_is_not_a_magnet_link_explains_and_fails() {
    let folder = tempfile::tempdir().unwrap();
    let output = dinghy()
        .args(["get", "hello", "--dir"])
        .arg(folder.path().join("downloads"))
        .output()
        .expect("failed to run dinghy");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("isn't a magnet link"), "got: {stderr}");
    // A typo must not create the download folder or start anything.
    assert!(!folder.path().join("downloads").exists());
}

#[test]
fn get_with_no_link_shows_usage() {
    let output = dinghy().arg("get").output().expect("failed to run dinghy");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage"), "got: {stderr}");
}

/// The whole thing for real: a pretend person on this computer shares three files, and the actual
/// `dinghy get` program downloads them, prints progress, saves every byte and exits with success.
/// No internet is used.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn get_downloads_from_a_peer_and_exits_successfully() {
    let work = tempfile::tempdir().unwrap();

    // Three small files to share.
    let source = work.path().join("source").join("sample");
    std::fs::create_dir_all(&source).unwrap();
    let files: [(&str, Vec<u8>); 2] = [
        ("one.bin", (0..90_000u32).map(|n| (n % 251) as u8).collect()),
        ("two.txt", b"hello from the seeder".to_vec()),
    ];
    for (name, bytes) in &files {
        std::fs::write(source.join(name), bytes).unwrap();
    }

    // The pretend seeder.
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
    let seeder = Session::new_with_opts(
        work.path().join("seeder-scratch"),
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
    let handle = seeder
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
    let seeder_address = seeder.listen_addr().unwrap();

    // Run the real program against it.
    let download_dir = work.path().join("downloads");
    let magnet = format!("magnet:?xt=urn:btih:{}", created.info_hash().as_string());
    let mut command = dinghy();
    command
        .args(["get", &magnet, "--no-dht", "--no-listen", "--peer"])
        .arg(seeder_address.to_string())
        .arg("--dir")
        .arg(&download_dir);
    let output = tokio::task::spawn_blocking(move || command.output())
        .await
        .unwrap()
        .expect("failed to run dinghy");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("Done: sample"), "stdout: {stdout}");
    for (name, bytes) in &files {
        let saved = std::fs::read(download_dir.join("sample").join(name)).unwrap();
        assert_eq!(&saved, bytes, "{name}");
    }

    seeder.stop().await;
}
