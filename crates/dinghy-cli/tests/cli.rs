//! Runs the real `dinghy` program, the same way a person would, and checks what it prints.
//!
//! Every test points the program's data folder at a scratch folder (through the
//! `BITDINGHY_DATA_DIR` setting), so tests never read or change anyone's real settings.

use std::net::Ipv4Addr;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;

use librqbit::spawn_utils::BlockingSpawner;
use librqbit::{
    AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerMode, ListenerOptions, Session,
    SessionOptions, create_torrent,
};

/// Cargo builds the binary first and tells us where it is via `CARGO_BIN_EXE_<name>`.
fn dinghy(data_dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dinghy"));
    command.env("BITDINGHY_DATA_DIR", data_dir);
    command
}

fn run(mut command: Command) -> Output {
    command.output().expect("failed to run dinghy")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn version_flag_prints_the_version() {
    let data = tempfile::tempdir().unwrap();
    let output = run({
        let mut c = dinghy(data.path());
        c.arg("--version");
        c
    });
    assert!(output.status.success());
    assert!(
        text(&output.stdout).contains(env!("CARGO_PKG_VERSION")),
        "got: {}",
        text(&output.stdout)
    );
}

#[test]
fn running_with_no_arguments_shows_the_app_name_and_a_hint() {
    let data = tempfile::tempdir().unwrap();
    let output = run(dinghy(data.path()));
    assert!(output.status.success());
    let stdout = text(&output.stdout);
    assert!(stdout.contains("BitDinghy"), "got: {stdout}");
    assert!(stdout.contains("dinghy get"), "got: {stdout}");
}

#[test]
fn get_with_something_that_is_not_a_magnet_link_explains_and_fails() {
    let data = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap().path().join("downloads");
    let output = run({
        let mut c = dinghy(data.path());
        c.args(["get", "hello", "--dir"]).arg(&downloads);
        c
    });
    assert!(!output.status.success());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("isn't a magnet link"), "got: {stderr}");
    // A typo must not create the download folder or start anything.
    assert!(!downloads.exists());
}

#[test]
fn get_with_no_link_shows_usage() {
    let data = tempfile::tempdir().unwrap();
    let output = run({
        let mut c = dinghy(data.path());
        c.arg("get");
        c
    });
    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("Usage"));
}

#[test]
fn settings_shows_the_data_folder_and_the_standard_download_folder() {
    let data = tempfile::tempdir().unwrap();
    let output = run({
        let mut c = dinghy(data.path());
        c.arg("settings");
        c
    });
    assert!(output.status.success());
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains(&data.path().display().to_string()),
        "got: {stdout}"
    );
    assert!(stdout.contains("BITDINGHY_DATA_DIR"), "got: {stdout}");
    assert!(stdout.contains("(standard folder)"), "got: {stdout}");
    assert!(stdout.contains("BitDinghy"), "got: {stdout}");
}

#[test]
fn a_changed_download_folder_is_remembered_by_the_next_run() {
    let data = tempfile::tempdir().unwrap();
    let chosen = tempfile::tempdir().unwrap().path().join("my downloads");

    // Change it...
    let output = run({
        let mut c = dinghy(data.path());
        c.args(["settings", "set", "download-dir"]).arg(&chosen);
        c
    });
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(text(&output.stdout).contains("Saved"));
    assert!(chosen.is_dir(), "the folder is created");

    // ...and a brand-new run of the program still knows.
    let output = run({
        let mut c = dinghy(data.path());
        c.arg("settings");
        c
    });
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains(&chosen.display().to_string()),
        "got: {stdout}"
    );
    assert!(!stdout.contains("(standard folder)"), "got: {stdout}");

    // The settings file is plain text that a person can read.
    let file = std::fs::read_to_string(data.path().join("settings.toml")).unwrap();
    assert!(file.contains("version = 1"), "{file}");
    assert!(file.contains("download_dir"), "{file}");

    // "default" goes back to the standard folder.
    let output = run({
        let mut c = dinghy(data.path());
        c.args(["settings", "set", "download-dir", "default"]);
        c
    });
    assert!(output.status.success());
    assert!(text(&output.stdout).contains("(standard folder)"));
}

#[test]
fn a_download_folder_that_cannot_be_used_is_refused_and_nothing_is_saved() {
    let data = tempfile::tempdir().unwrap();
    let a_file = data.path().join("this-is-a-file");
    std::fs::write(&a_file, b"x").unwrap();
    let output = run({
        let mut c = dinghy(data.path());
        c.args(["settings", "set", "download-dir"]).arg(&a_file);
        c
    });
    assert!(!output.status.success());
    assert!(
        text(&output.stderr).contains("a file, not a folder"),
        "got: {}",
        text(&output.stderr)
    );
    assert!(!data.path().join("settings.toml").exists());
}

#[test]
fn an_unknown_setting_name_is_explained() {
    let data = tempfile::tempdir().unwrap();
    let output = run({
        let mut c = dinghy(data.path());
        c.args(["settings", "set", "nonsense", "x"]);
        c
    });
    assert!(!output.status.success());
    assert!(
        text(&output.stderr).contains("download-dir"),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn a_damaged_settings_file_is_kept_and_the_program_carries_on() {
    let data = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join("settings.toml"), "[[ not toml").unwrap();
    let output = run({
        let mut c = dinghy(data.path());
        c.arg("settings");
        c
    });
    assert!(output.status.success());
    assert!(
        text(&output.stderr).contains("couldn't read your saved settings"),
        "got: {}",
        text(&output.stderr)
    );
    let kept: Vec<_> = std::fs::read_dir(data.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("settings.toml.broken-"))
        .collect();
    assert_eq!(kept.len(), 1, "the damaged file is kept for recovery");
}

/// Shares two small files from this computer, like another person would.
struct Seeder {
    _session: Arc<Session>,
    address: std::net::SocketAddr,
    magnet: String,
    files: Vec<(&'static str, Vec<u8>)>,
}

async fn start_seeder(work: &Path) -> Seeder {
    let source = work.join("source").join("sample");
    std::fs::create_dir_all(&source).unwrap();
    let files: Vec<(&'static str, Vec<u8>)> = vec![
        ("one.bin", (0..90_000u32).map(|n| (n % 251) as u8).collect()),
        ("two.txt", b"hello from the seeder".to_vec()),
    ];
    for (name, bytes) in &files {
        std::fs::write(source.join(name), bytes).unwrap();
    }

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

    Seeder {
        address: session.listen_addr().unwrap(),
        magnet: format!("magnet:?xt=urn:btih:{}", created.info_hash().as_string()),
        files,
        _session: session,
    }
}

/// Runs `dinghy` without blocking the async runtime the seeder lives on.
async fn run_blocking(command: Command) -> Output {
    tokio::task::spawn_blocking(move || run(command))
        .await
        .unwrap()
}

fn assert_downloaded(seeder: &Seeder, folder: &Path) {
    for (name, bytes) in &seeder.files {
        let saved = std::fs::read(folder.join("sample").join(name)).unwrap();
        assert_eq!(&saved, bytes, "{name}");
    }
}

/// The whole thing for real: a pretend person on this computer shares files, and the actual
/// `dinghy get` program downloads them, prints progress, saves every byte and exits with success.
/// It does so twice: once into a folder given with `--dir`, and once into the REMEMBERED download
/// folder, set by a separate earlier run of the program. No internet is used.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn get_downloads_from_a_peer_into_the_chosen_and_the_remembered_folder() {
    let work = tempfile::tempdir().unwrap();
    let data = work.path().join("data");
    let seeder = start_seeder(work.path()).await;
    let offline_args = |c: &mut Command| {
        c.args(["--no-dht", "--no-listen", "--peer"])
            .arg(seeder.address.to_string());
    };

    // 1. With --dir.
    let with_dir = work.path().join("via-dir-flag");
    let mut command = dinghy(&data);
    command.args(["get", &seeder.magnet]);
    offline_args(&mut command);
    command.arg("--dir").arg(&with_dir);
    let output = run_blocking(command).await;
    let (stdout, stderr) = (text(&output.stdout), text(&output.stderr));
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("Done: sample"), "stdout: {stdout}");
    assert_downloaded(&seeder, &with_dir);

    // 2. With the remembered folder (a different run of the program saved it).
    let remembered = work.path().join("via-saved-setting");
    let mut command = dinghy(&data);
    command
        .args(["settings", "set", "download-dir"])
        .arg(&remembered);
    assert!(run_blocking(command).await.status.success());

    let mut command = dinghy(&data);
    command.args(["get", &seeder.magnet]);
    offline_args(&mut command);
    let output = run_blocking(command).await;
    let (stdout, stderr) = (text(&output.stdout), text(&output.stderr));
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert_downloaded(&seeder, &remembered);
}
