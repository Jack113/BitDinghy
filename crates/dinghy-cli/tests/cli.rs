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
    /// The torrent's name (also its folder's name).
    name: &'static str,
    files: Vec<(&'static str, Vec<u8>)>,
}

/// A seeder sharing a torrent called `name`. Different names give different torrents.
async fn start_seeder(work: &Path, name: &'static str) -> Seeder {
    let source = work.join("source").join(name);
    std::fs::create_dir_all(&source).unwrap();
    let files: Vec<(&'static str, Vec<u8>)> = vec![
        ("one.bin", (0..90_000u32).map(|n| (n % 251) as u8).collect()),
        (
            "two.txt",
            format!("hello from the seeder of {name}").into_bytes(),
        ),
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
        work.join(format!("seeder-scratch-{name}")),
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
        name,
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
        let saved = std::fs::read(folder.join(seeder.name).join(name)).unwrap();
        assert_eq!(&saved, bytes, "{name}");
    }
}

/// `dinghy get <seeder's link>` with no internet: only the seeder on this computer is asked.
fn get_from(data: &Path, seeder: &Seeder) -> Command {
    let mut command = dinghy(data);
    command
        .args(["get", &seeder.magnet, "--no-dht", "--no-listen", "--peer"])
        .arg(seeder.address.to_string());
    command
}

fn assert_success(output: &Output) -> String {
    let (stdout, stderr) = (text(&output.stdout), text(&output.stderr));
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    stdout
}

/// The whole thing for real: pretend people on this computer share files, and the actual
/// `dinghy get` program downloads them, prints progress, saves every byte and exits with success.
/// It downloads into a folder given with `--dir`, then into the REMEMBERED download folder (set by
/// a separate earlier run of the program), and both end up in the saved list. No internet is used.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn get_downloads_from_a_peer_into_the_chosen_and_the_remembered_folder() {
    let work = tempfile::tempdir().unwrap();
    let data = work.path().join("data");
    let first = start_seeder(work.path(), "sample").await;
    let second = start_seeder(work.path(), "another").await;

    // 1. With --dir.
    let with_dir = work.path().join("via-dir-flag");
    let mut command = get_from(&data, &first);
    command.arg("--dir").arg(&with_dir);
    let stdout = assert_success(&run_blocking(command).await);
    assert!(stdout.contains("Done: sample"), "stdout: {stdout}");
    assert_downloaded(&first, &with_dir);

    // 2. With the remembered folder (a different run of the program saved it).
    let remembered = work.path().join("via-saved-setting");
    let mut command = dinghy(&data);
    command
        .args(["settings", "set", "download-dir"])
        .arg(&remembered);
    assert!(run_blocking(command).await.status.success());

    let stdout = assert_success(&run_blocking(get_from(&data, &second)).await);
    assert!(stdout.contains("Done: another"), "stdout: {stdout}");
    assert_downloaded(&second, &remembered);

    // 3. The first link again, even with a different --dir: it is already in the list, so it
    //    carries on (here: is already finished) in the folder it was started in.
    let mut command = get_from(&data, &first);
    command.arg("--dir").arg(work.path().join("somewhere-else"));
    let stdout = assert_success(&run_blocking(command).await);
    assert!(stdout.contains("already in your list"), "stdout: {stdout}");
    assert!(
        stdout.contains(&with_dir.display().to_string()),
        "stdout: {stdout}"
    );
    assert!(!work.path().join("somewhere-else").join("sample").exists());

    // 4. `dinghy list` shows both, in the order they were added, with their folders.
    let mut command = dinghy(&data);
    command.arg("list");
    let stdout = assert_success(&run_blocking(command).await);
    assert!(stdout.contains("2 saved downloads"), "stdout: {stdout}");
    let sample_line = stdout.find("1. sample").expect(&stdout);
    let another_line = stdout.find("2. another").expect(&stdout);
    assert!(sample_line < another_line, "stdout: {stdout}");
    assert!(
        stdout.contains(&with_dir.join("sample").display().to_string()),
        "stdout: {stdout}"
    );
}

#[test]
fn list_with_nothing_saved_says_so() {
    let data = tempfile::tempdir().unwrap();
    let output = run({
        let mut c = dinghy(data.path());
        c.arg("list");
        c
    });
    let stdout = assert_success(&output);
    assert!(stdout.contains("No saved downloads yet"), "{stdout}");
}

#[test]
fn get_is_refused_while_another_bitdinghy_is_running() {
    let data = tempfile::tempdir().unwrap();
    // Pretend the window is open: it holds the saved list.
    let window = dinghy_core::SavedList::open(&data.path().join("downloads.db")).unwrap();

    let output = run({
        let mut c = dinghy(data.path());
        c.args([
            "get",
            "magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056",
            "--no-dht",
            "--no-listen",
        ]);
        c
    });
    assert!(!output.status.success());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("already running"), "{stderr}");

    // Reading the list still works while the window has it.
    let output = run({
        let mut c = dinghy(data.path());
        c.arg("list");
        c
    });
    assert_success(&output);
    drop(window);
}

#[test]
fn a_damaged_saved_list_is_reported_not_guessed() {
    let data = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join("downloads.db"), b"not a database").unwrap();
    let output = run({
        let mut c = dinghy(data.path());
        c.arg("list");
        c
    });
    assert!(!output.status.success());
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("couldn't read the download list"),
        "{stderr}"
    );
    // `list` only reads: the file is left exactly as it was.
    assert_eq!(
        std::fs::read(data.path().join("downloads.db")).unwrap(),
        b"not a database"
    );
}

/// Puts one download in the saved list, the way an earlier run of BitDinghy would have.
fn save_one_download(data: &Path, name: &str) {
    let opened = dinghy_core::SavedList::open(&data.join("downloads.db")).unwrap();
    opened
        .list
        .add(&dinghy_core::SavedDownload::new(
            "c9e15763f722f23e98a29decdfae341b98d53056",
            name,
            "magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056",
            &data.join("downloads"),
        ))
        .unwrap();
}

fn dinghy_output(data: &Path, args: &[&str]) -> Output {
    run({
        let mut c = dinghy(data);
        c.args(args);
        c
    })
}

#[test]
fn pause_and_resume_change_the_saved_list() {
    let data = tempfile::tempdir().unwrap();
    save_one_download(data.path(), "Example");

    let stdout = assert_success(&dinghy_output(data.path(), &["pause", "1"]));
    assert!(stdout.contains("Paused: Example"), "{stdout}");
    let stdout = assert_success(&dinghy_output(data.path(), &["list"]));
    assert!(stdout.contains("[paused]"), "{stdout}");

    // Pausing twice is fine, and says so.
    let stdout = assert_success(&dinghy_output(data.path(), &["pause", "1"]));
    assert!(stdout.contains("already paused"), "{stdout}");

    let stdout = assert_success(&dinghy_output(data.path(), &["resume", "1"]));
    assert!(stdout.contains("Resumed: Example"), "{stdout}");
    let stdout = assert_success(&dinghy_output(data.path(), &["list"]));
    assert!(!stdout.contains("[paused]"), "{stdout}");
}

#[test]
fn a_number_that_is_not_in_the_list_is_explained() {
    let data = tempfile::tempdir().unwrap();
    save_one_download(data.path(), "Example");
    for number in ["0", "2"] {
        let output = dinghy_output(data.path(), &["pause", number]);
        assert!(!output.status.success());
        let stderr = text(&output.stderr);
        assert!(stderr.contains("dinghy list"), "{stderr}");
    }
    // Not a number at all: the usage help says what is expected.
    let output = dinghy_output(data.path(), &["resume", "first"]);
    assert!(!output.status.success());
}

#[test]
fn pause_is_refused_while_the_window_is_open() {
    let data = tempfile::tempdir().unwrap();
    save_one_download(data.path(), "Example");
    let window = dinghy_core::SavedList::open(&data.path().join("downloads.db")).unwrap();

    let output = dinghy_output(data.path(), &["pause", "1"]);
    assert!(!output.status.success());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("pause it there"), "{stderr}");
    let output = dinghy_output(data.path(), &["remove", "1"]);
    assert!(!output.status.success());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("remove it there"), "{stderr}");
    drop(window);

    // Nothing changed.
    let stdout = assert_success(&dinghy_output(data.path(), &["list"]));
    assert!(!stdout.contains("[paused]"), "{stdout}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn remove_keeps_the_files_and_a_new_download_never_overwrites_them() {
    let work = tempfile::tempdir().unwrap();
    let data = work.path().join("data");
    let downloads = work.path().join("downloads");
    let seeder = start_seeder(work.path(), "sample").await;

    let mut command = get_from(&data, &seeder);
    command.arg("--dir").arg(&downloads);
    assert_success(&run_blocking(command).await);

    let stdout = assert_success(
        &run_blocking({
            let mut c = dinghy(&data);
            c.args(["remove", "1"]);
            c
        })
        .await,
    );
    assert!(stdout.contains("Removed: sample"), "{stdout}");
    assert!(stdout.contains("still in"), "{stdout}");
    assert_downloaded(&seeder, &downloads);
    let stdout = assert_success(
        &run_blocking({
            let mut c = dinghy(&data);
            c.arg("list");
            c
        })
        .await,
    );
    assert!(stdout.contains("No saved downloads"), "{stdout}");

    // The files are no longer BitDinghy's, so the same torrent added again must not take them over.
    let mut command = get_from(&data, &seeder);
    command.arg("--dir").arg(&downloads);
    let output = run_blocking(command).await;
    assert!(!output.status.success());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("same name"), "{stderr}");
    assert_downloaded(&seeder, &downloads);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn remove_with_delete_files_asks_first_and_deletes_only_its_own_files() {
    let work = tempfile::tempdir().unwrap();
    let data = work.path().join("data");
    let downloads = work.path().join("downloads");
    let seeder = start_seeder(work.path(), "sample").await;

    let mut command = get_from(&data, &seeder);
    command.arg("--dir").arg(&downloads);
    assert_success(&run_blocking(command).await);
    let neighbour = downloads.join("my own file.txt");
    std::fs::write(&neighbour, b"mine").unwrap();

    // No person at the keyboard (tests run without one): it won't delete without --yes.
    let output = run_blocking({
        let mut c = dinghy(&data);
        c.args(["remove", "1", "--delete-files"]);
        c
    })
    .await;
    assert!(!output.status.success());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("--yes"), "{stderr}");
    assert_downloaded(&seeder, &downloads);

    let stdout = assert_success(
        &run_blocking({
            let mut c = dinghy(&data);
            c.args(["remove", "1", "--delete-files", "--yes"]);
            c
        })
        .await,
    );
    assert!(stdout.contains("Deleted 2 files"), "{stdout}");
    assert!(
        !downloads.join("sample").exists(),
        "the torrent's folder is gone"
    );
    assert_eq!(std::fs::read(&neighbour).unwrap(), b"mine");
}

#[test]
fn yes_on_its_own_is_refused() {
    let data = tempfile::tempdir().unwrap();
    let output = dinghy_output(data.path(), &["remove", "1", "--yes"]);
    assert!(!output.status.success());
    assert!(
        text(&output.stderr).contains("--delete-files"),
        "{}",
        text(&output.stderr)
    );
}

// ---------------------------------------------------------------------------------------------
// M4: the command line talks to the open window.
// ---------------------------------------------------------------------------------------------

/// A pretend window inside the test: an engine that holds the saved list in `data` (like the real
/// window) and listens on the channel the real `dinghy` will look for.
async fn open_window(data: &Path, downloads: &Path) -> Arc<dinghy_core::Engine> {
    let data_dir = dinghy_core::DataDir::custom(data.to_path_buf()).unwrap();
    let mut config = dinghy_core::EngineConfig::new(downloads);
    config.use_dht = false;
    config.accept_incoming = false;
    config.local_discovery = false;
    config.saved_list_file = Some(data_dir.file("downloads.db"));
    let engine = Arc::new(dinghy_core::Engine::start(config).await.unwrap());
    let address = dinghy_core::remote::Address::for_data_dir(data_dir.path());
    dinghy_core::remote::start_server(engine.clone(), &address).unwrap();
    engine
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn with_the_window_open_get_hands_the_link_over_and_list_shows_it_live() {
    let work = tempfile::tempdir().unwrap();
    let data = work.path().join("data");
    let downloads = work.path().join("window-downloads");
    let seeder = start_seeder(work.path(), "sample").await;
    let window = open_window(&data, &downloads).await;

    // `list` asks the window, which has nothing yet.
    let stdout = assert_success(
        &run_blocking({
            let mut c = dinghy(&data);
            c.arg("list");
            c
        })
        .await,
    );
    assert!(
        stdout.contains("window is open and has no downloads"),
        "{stdout}"
    );

    // `get` hands the link to the window and follows it to the end.
    let stdout = assert_success(&run_blocking(get_from(&data, &seeder)).await);
    assert!(stdout.contains("download was added there"), "{stdout}");
    assert!(
        stdout.contains("--no-dht and --no-listen don't apply"),
        "{stdout}"
    );
    assert!(stdout.contains("Done: sample"), "{stdout}");
    // The WINDOW did the downloading, into its own folder.
    assert_downloaded(&seeder, &downloads);
    assert_eq!(window.snapshot().len(), 1);

    // `list` now shows the window's live state.
    let stdout = assert_success(
        &run_blocking({
            let mut c = dinghy(&data);
            c.arg("list");
            c
        })
        .await,
    );
    assert!(
        stdout.contains("1 download in BitDinghy's open window"),
        "{stdout}"
    );
    assert!(stdout.contains("1. sample  [finished, sharing"), "{stdout}");

    window.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn with_the_window_open_get_refuses_a_different_folder() {
    let work = tempfile::tempdir().unwrap();
    let data = work.path().join("data");
    let window = open_window(&data, &work.path().join("window-downloads")).await;
    let elsewhere = work.path().join("elsewhere");

    let output = run_blocking({
        let mut c = dinghy(&data);
        c.args([
            "get",
            "magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056",
            "--dir",
        ])
        .arg(&elsewhere);
        c
    })
    .await;
    assert!(!output.status.success());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("Leave out --dir"), "{stderr}");
    assert!(!elsewhere.exists());
    assert!(window.snapshot().is_empty(), "nothing was added");
    window.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn a_bad_link_is_refused_before_the_window_is_even_asked() {
    let work = tempfile::tempdir().unwrap();
    let data = work.path().join("data");
    let window = open_window(&data, &work.path().join("window-downloads")).await;
    let output = run_blocking({
        let mut c = dinghy(&data);
        c.args(["get", "hello"]);
        c
    })
    .await;
    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("isn't a magnet link"));
    assert!(window.snapshot().is_empty());
    window.shutdown().await;
}
