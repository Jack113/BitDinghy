//! The `dinghy` command-line program. A thin front-end: it parses what you typed and hands the
//! real work to `dinghy-core`. More commands (`pause`, `resume`...) arrive feature by feature.

use std::io::{IsTerminal, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand, ValueEnum};
use dinghy_core::display::{format_bytes, format_eta, format_speed, fraction, waiting_text};
use dinghy_core::{
    DownloadSnapshot, DownloadStatus, Engine, Profile, SavedDownload, SavedList, magnet,
    owned_files, remote,
};

/// BitDinghy on the command line.
#[derive(Parser)]
#[command(name = "dinghy", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Download a magnet link, showing progress, and stop when it is finished
    Get(GetArgs),
    /// Show the saved download list (it works while the window is open, too)
    List,
    /// Pause a saved download, so it stays paused until you resume it
    Pause(NumberArg),
    /// Resume a paused download (it carries on when BitDinghy next runs it)
    Resume(NumberArg),
    /// Remove a download from the saved list (its files stay, unless you add --delete-files)
    Remove(RemoveArgs),
    /// Show where BitDinghy keeps its files and what it remembers, or change a setting
    Settings(SettingsArgs),
}

#[derive(Args)]
struct NumberArg {
    /// The download's number, as shown by `dinghy list`
    number: usize,
}

#[derive(Args)]
struct RemoveArgs {
    /// The download's number, as shown by `dinghy list`
    number: usize,

    /// Also delete the files it downloaded (only those). You are asked to confirm
    #[arg(long)]
    delete_files: bool,

    /// Don't ask before deleting files (for scripts)
    #[arg(long, requires = "delete_files")]
    yes: bool,
}

#[derive(Args)]
struct SettingsArgs {
    #[command(subcommand)]
    action: Option<SettingsAction>,
}

#[derive(Subcommand)]
enum SettingsAction {
    /// Change a setting (it is remembered for next time, in the window too)
    Set {
        /// Which setting to change
        key: SettingKey,
        /// The new value, or the word "default" to go back to the standard one
        value: String,
    },
}

/// The settings that can be changed. More arrive with later features.
#[derive(Clone, Copy, ValueEnum)]
enum SettingKey {
    /// The folder downloads are saved in
    DownloadDir,
}

#[derive(Args)]
struct GetArgs {
    /// The magnet link. Put it in quotes, because it contains & characters
    link: String,

    /// Folder to save into, just this once [default: your remembered download folder]
    #[arg(short, long, value_name = "FOLDER")]
    dir: Option<PathBuf>,

    /// Also try this person directly (IP:PORT). Handy on your own network
    #[arg(long = "peer", value_name = "IP:PORT")]
    peers: Vec<SocketAddr>,

    /// Don't look for people through the DHT (a shared phone book of peers)
    #[arg(long)]
    no_dht: bool,

    /// Don't accept connections from other people (you can still download)
    #[arg(long)]
    no_listen: bool,

    /// Show the engine's detailed log messages (useful when something goes wrong)
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    match Cli::parse().command {
        Some(Command::Get(args)) => get(args).await,
        Some(Command::List) => list().await,
        Some(Command::Pause(which)) => set_paused(which.number, true),
        Some(Command::Resume(which)) => set_paused(which.number, false),
        Some(Command::Remove(args)) => remove(args),
        Some(Command::Settings(args)) => settings(args),
        None => {
            println!("{}", dinghy_core::about());
            println!("Try: dinghy get \"<magnet link>\"     (or dinghy --help)");
            ExitCode::SUCCESS
        }
    }
}

/// Prints anything the user should know about their saved settings (for example a damaged file).
fn print_notices(profile: &Profile) {
    for notice in profile.notices() {
        eprintln!("Note: {notice}");
    }
}

/// `dinghy settings`: show where BitDinghy's files are and what it remembers.
fn show_settings(profile: &Profile) {
    println!("{} settings", dinghy_core::about());
    match profile.data_dir() {
        Some(dir) => println!(
            "Data folder:   {}  ({})",
            dir.path().display(),
            dir.kind().describe()
        ),
        None => println!("Data folder:   none available, so nothing can be remembered"),
    }
    if let Some(path) = profile.settings_path() {
        println!("Settings file: {}", path.display());
    }
    let remembered = profile.settings().download_dir.is_some();
    println!(
        "download-dir:  {}{}",
        profile.settings().download_dir_or_default().display(),
        if remembered {
            ""
        } else {
            "  (standard folder)"
        }
    );
}

/// `dinghy settings` and `dinghy settings set <key> <value>`.
fn settings(args: SettingsArgs) -> ExitCode {
    let mut profile = Profile::load();
    print_notices(&profile);

    let Some(SettingsAction::Set { key, value }) = args.action else {
        show_settings(&profile);
        return ExitCode::SUCCESS;
    };

    let result = match key {
        SettingKey::DownloadDir if value.eq_ignore_ascii_case("default") => {
            profile.set_download_dir(None)
        }
        SettingKey::DownloadDir => profile.set_download_dir(Some(Path::new(&value))),
    };
    match result {
        Ok(()) => {
            println!("Saved. It will be used from now on, in the window too.");
            show_settings(&profile);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// `dinghy list`: the open window's list, live; or with the window closed, the saved list.
async fn list() -> ExitCode {
    let profile = Profile::load();
    print_notices(&profile);
    match window_client(&profile).await {
        Err(code) => return code,
        Ok(Some(mut window)) => return list_from_window(&mut window).await,
        Ok(None) => {}
    }
    let Some(path) = profile.saved_list_path() else {
        println!("BitDinghy has no data folder right now, so it doesn't remember any downloads.");
        return ExitCode::SUCCESS;
    };
    let saved = match SavedList::read(&path) {
        Ok(saved) => saved,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    if saved.is_empty() {
        println!("No saved downloads yet. Add one with: dinghy get \"<magnet link>\"");
        return ExitCode::SUCCESS;
    }
    println!(
        "{} saved download{}:",
        saved.len(),
        if saved.len() == 1 { "" } else { "s" }
    );
    for (index, download) in saved.iter().enumerate() {
        println!(
            "{:>3}. {}  [{}]",
            index + 1,
            shorten(&download.name, 60),
            saved_state(download)
        );
        let folder = download
            .output_folder
            .as_deref()
            .unwrap_or(&download.download_dir);
        println!("     {}", folder.display());
    }
    println!("Progress shows in the BitDinghy window, or while `dinghy get` runs.");
    ExitCode::SUCCESS
}

/// Opens the saved list to change it (only while BitDinghy isn't running), and finds download
/// number `number`. On a problem, it has already been explained and `Err` holds the exit code.
fn open_for_change(number: usize, verb: &str) -> Result<(SavedList, SavedDownload), ExitCode> {
    let profile = Profile::load();
    print_notices(&profile);
    let Some(path) = profile.saved_list_path() else {
        eprintln!("BitDinghy has no data folder right now, so there is no saved list to change.");
        return Err(ExitCode::FAILURE);
    };
    let opened = match SavedList::open(&path) {
        Ok(opened) => opened,
        Err(dinghy_core::Error::AlreadyRunning) => {
            eprintln!(
                "BitDinghy is running (in its window or in another terminal), so {verb} it \
                 there. (A later version lets the command line pass this on.)"
            );
            return Err(ExitCode::FAILURE);
        }
        Err(error) => {
            eprintln!("{error}");
            return Err(ExitCode::FAILURE);
        }
    };
    if let Some(problem) = &opened.problem {
        eprintln!("Note: {}", problem.message());
    }
    let saved = opened.list.all().map_err(|error| {
        eprintln!("{error}");
        ExitCode::FAILURE
    })?;
    let Some(download) = number.checked_sub(1).and_then(|index| saved.get(index)) else {
        eprintln!("There is no download number {number}. `dinghy list` shows the numbers.");
        return Err(ExitCode::FAILURE);
    };
    Ok((opened.list, download.clone()))
}

/// `dinghy pause <n>` and `dinghy resume <n>`: changes the saved list while BitDinghy isn't running.
fn set_paused(number: usize, paused: bool) -> ExitCode {
    let verb = if paused { "pause" } else { "resume" };
    let (list, download) = match open_for_change(number, verb) {
        Ok(found) => found,
        Err(code) => return code,
    };

    if download.paused == paused {
        println!(
            "{} is already {}.",
            download.name,
            if paused { "paused" } else { "active" }
        );
        return ExitCode::SUCCESS;
    }
    if let Err(error) = list.set_paused(&download.info_hash, paused) {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }
    if paused {
        println!(
            "Paused: {}. It stays paused, in the window too, until you resume it.",
            download.name
        );
    } else {
        println!(
            "Resumed: {}. It carries on the next time BitDinghy opens (or right away with `dinghy get`).",
            download.name
        );
    }
    ExitCode::SUCCESS
}

/// `dinghy remove <n> [--delete-files [--yes]]`: forgets a download, and if asked deletes the files
/// it downloaded (only those: see `owned_files.rs` in dinghy-core).
fn remove(args: RemoveArgs) -> ExitCode {
    let (list, download) = match open_for_change(args.number, "remove") {
        Ok(found) => found,
        Err(code) => return code,
    };
    let folder = download
        .output_folder
        .clone()
        .unwrap_or_else(|| download.download_dir.clone());

    // Work out exactly which files would go BEFORE anything is changed.
    let to_delete = if args.delete_files {
        match (&download.torrent, &download.output_folder) {
            (Some(torrent), Some(output_folder)) => {
                match owned_files::files_of(torrent, &download.download_dir, output_folder) {
                    Ok(paths) => paths.into_iter().filter(|path| path.exists()).collect(),
                    Err(error) => {
                        eprintln!("{error}");
                        return ExitCode::FAILURE;
                    }
                }
            }
            // Its file list never arrived, so it never wrote anything.
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };

    if !to_delete.is_empty() && !args.yes && !confirm_deleting(&download.name, &to_delete) {
        println!("Nothing was removed or deleted.");
        return ExitCode::FAILURE;
    }

    let deleted = owned_files::delete(&to_delete, &download.download_dir, &folder);
    if let Err(error) = list.remove(&download.info_hash) {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }
    println!("Removed: {}.", download.name);
    if !args.delete_files {
        println!("Its files are still in {}.", folder.display());
    } else if deleted.files > 0 {
        println!(
            "Deleted {} file{} it downloaded.",
            deleted.files,
            if deleted.files == 1 { "" } else { "s" }
        );
    }
    if deleted.problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        eprintln!("These files couldn't be deleted; you can delete them yourself:");
        for problem in &deleted.problems {
            eprintln!("  {problem}");
        }
        ExitCode::FAILURE
    }
}

/// Shows what would be deleted and asks for "yes". Without a person at the keyboard (a script),
/// it refuses and points to `--yes`.
fn confirm_deleting(name: &str, paths: &[PathBuf]) -> bool {
    if !std::io::stdin().is_terminal() {
        eprintln!(
            "Deleting files needs a confirmation. Run this in a terminal, or add --yes if you are \
             sure."
        );
        return false;
    }
    println!(
        "This deletes {} file{} that BitDinghy downloaded for \"{name}\":",
        paths.len(),
        if paths.len() == 1 { "" } else { "s" }
    );
    for path in paths.iter().take(5) {
        println!("  {}", path.display());
    }
    if paths.len() > 5 {
        println!("  ...and {} more", paths.len() - 5);
    }
    print!("Type yes to delete them: ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).is_ok() && answer.trim().eq_ignore_ascii_case("yes")
}

/// A saved download's state in a word or two (without the engine running, progress isn't known).
fn saved_state(download: &SavedDownload) -> &'static str {
    if download.paused {
        "paused"
    } else if download.torrent.is_none() {
        "looking for its file list"
    } else {
        "active"
    }
}

/// `dinghy get`: download one magnet link in this terminal.
async fn get(args: GetArgs) -> ExitCode {
    if args.verbose {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "warn,librqbit=info,dinghy_core=debug".into());
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .init();
    }

    // Check the link BEFORE starting anything, so a typo fails instantly and quietly.
    if let Err(error) = magnet::parse(&args.link) {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }

    let profile = Profile::load();
    print_notices(&profile);
    // The window is open? Then it owns the engine: hand the link over and follow it from here.
    match window_client(&profile).await {
        Err(code) => return code,
        Ok(Some(window)) => return get_through_window(window, args).await,
        Ok(None) => {}
    }

    // Remembered settings first; `--dir` overrides the folder for this run only.
    let mut config = profile.engine_config();
    if let Some(dir) = &args.dir {
        config.download_dir = dir.clone();
    }
    config.use_dht = !args.no_dht;
    config.accept_incoming = !args.no_listen;
    // Only this one download: the others in the saved list wait for the window.
    config.restore_saved = false;
    let engine = match Engine::start(config).await {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    for notice in engine.notices() {
        eprintln!("Note: {notice}");
    }

    let id = match engine.add_magnet(&args.link, args.peers) {
        Ok(id) => id,
        Err(error) => {
            eprintln!("{error}");
            engine.shutdown().await;
            return ExitCode::FAILURE;
        }
    };
    // A download already in the saved list carries on in the folder it was started in.
    let folder = engine
        .download_dir_of(id)
        .unwrap_or_else(|| engine.download_dir().to_path_buf());
    if args.dir.is_some() && folder != engine.download_dir() {
        println!(
            "This download is already in your list, so it carries on in the folder it was started in."
        );
    }
    println!("Saving to {}", folder.display());

    let mut printer = ProgressPrinter::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(500));
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    let exit = loop {
        tokio::select! {
            _ = ticker.tick() => {
                let Some(snapshot) = engine.snapshot().into_iter().find(|s| s.id == id) else {
                    continue;
                };
                if let Some(code) = show_progress(&mut printer, &snapshot, &folder) {
                    break code;
                }
            }
            _ = &mut ctrl_c => {
                printer.finish();
                println!("Stopped. Files already saved stay in place; run the same command again to continue.");
                break ExitCode::from(130);
            }
        }
    };

    engine.shutdown().await;
    exit
}

/// Draws one progress update. When the download has finished or failed, says so and returns the
/// exit code to stop with.
fn show_progress(
    printer: &mut ProgressPrinter,
    snapshot: &DownloadSnapshot,
    folder: &Path,
) -> Option<ExitCode> {
    printer.show(snapshot);
    match snapshot.status {
        DownloadStatus::Finished => {
            printer.finish();
            println!(
                "Done: {} ({}) is in {}",
                snapshot.name,
                format_bytes(snapshot.total_bytes),
                snapshot.save_folder.as_deref().unwrap_or(folder).display()
            );
            Some(ExitCode::SUCCESS)
        }
        DownloadStatus::Failed => {
            printer.finish();
            eprintln!(
                "{}",
                snapshot.error.as_deref().unwrap_or("The download failed.")
            );
            Some(ExitCode::FAILURE)
        }
        _ => None,
    }
}

/// Connects to the open window, if there is one. `Ok(None)`: no window is open (or there is no
/// data folder), so the command line works on its own. A real problem has already been explained
/// when `Err` comes back.
async fn window_client(profile: &Profile) -> Result<Option<remote::Client>, ExitCode> {
    let Some(data_dir) = profile.data_dir() else {
        return Ok(None);
    };
    remote::Client::connect(&remote::Address::for_data_dir(data_dir.path()))
        .await
        .map_err(|error| {
            eprintln!("{error}");
            ExitCode::FAILURE
        })
}

/// `dinghy get` while the window is open: the link is handed to the window, and this terminal
/// follows its progress. Ctrl+C only stops following; the window carries on.
async fn get_through_window(mut window: remote::Client, args: GetArgs) -> ExitCode {
    if args.dir.is_some() {
        eprintln!(
            "BitDinghy's window is open, so the download goes into the window's download folder. \
             Leave out --dir, or close the window first."
        );
        return ExitCode::FAILURE;
    }
    if args.no_dht || args.no_listen {
        println!(
            "Note: --no-dht and --no-listen don't apply; the open window uses its own settings."
        );
    }
    let (info_hash, folder) = match window
        .ask(remote::Command::Add {
            link: args.link,
            peers: args.peers,
        })
        .await
    {
        Ok(remote::Reply::Added { info_hash, folder }) => (info_hash, folder),
        Ok(_) => {
            eprintln!("BitDinghy's window gave an unexpected answer.");
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    println!("BitDinghy's window is open, so the download was added there.");
    println!("Saving to {}", folder.display());

    let mut printer = ProgressPrinter::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(500));
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let downloads = match window.ask(remote::Command::List).await {
                    Ok(remote::Reply::Downloads { downloads, .. }) => downloads,
                    Ok(_) => continue,
                    Err(_) => {
                        printer.finish();
                        eprintln!(
                            "BitDinghy's window was closed. The download carries on the next time \
                             it opens."
                        );
                        return ExitCode::FAILURE;
                    }
                };
                let Some(snapshot) = downloads.iter().find(|d| d.info_hash == info_hash) else {
                    printer.finish();
                    println!("The download was removed in the window.");
                    return ExitCode::SUCCESS;
                };
                if let Some(code) = show_progress(&mut printer, snapshot, &folder) {
                    return code;
                }
            }
            _ = &mut ctrl_c => {
                printer.finish();
                println!("Stopped following. The download carries on in BitDinghy's window.");
                return ExitCode::from(130);
            }
        }
    }
}

/// `dinghy list` while the window is open: the window's own list, live.
async fn list_from_window(window: &mut remote::Client) -> ExitCode {
    let downloads = match window.ask(remote::Command::List).await {
        Ok(remote::Reply::Downloads { downloads, .. }) => downloads,
        Ok(_) => {
            eprintln!("BitDinghy's window gave an unexpected answer.");
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    if downloads.is_empty() {
        println!("BitDinghy's window is open and has no downloads.");
        return ExitCode::SUCCESS;
    }
    println!(
        "{} download{} in BitDinghy's open window:",
        downloads.len(),
        if downloads.len() == 1 { "" } else { "s" }
    );
    for (index, download) in downloads.iter().enumerate() {
        println!(
            "{:>3}. {}  [{}]",
            index + 1,
            shorten(&download.name, 60),
            live_state(download)
        );
        if let Some(folder) = &download.save_folder {
            println!("     {}", folder.display());
        }
    }
    ExitCode::SUCCESS
}

/// A download's state, as the open window sees it right now.
fn live_state(download: &DownloadSnapshot) -> String {
    let percent = fraction(download.progress_bytes, download.total_bytes) * 100.0;
    match download.status {
        DownloadStatus::GettingInfo => "looking for people sharing it".to_string(),
        DownloadStatus::Checking => "checking files already on disk".to_string(),
        DownloadStatus::Downloading => format!(
            "{percent:.1}% of {}, {}, {} left",
            format_bytes(download.total_bytes),
            format_speed(download.download_bytes_per_second),
            format_eta(download.eta)
        ),
        DownloadStatus::Paused if download.total_bytes == 0 => "paused".to_string(),
        DownloadStatus::Paused => format!("paused at {percent:.1}%"),
        DownloadStatus::Finished => format!(
            "finished, sharing ({} uploaded)",
            format_bytes(download.uploaded_bytes)
        ),
        DownloadStatus::Failed => format!(
            "failed: {}",
            download.error.as_deref().unwrap_or("unknown problem")
        ),
    }
}

/// Shortens a long name so a progress line fits on one terminal line. Plain "..." is used
/// instead of the single "…" character, which older Windows terminals can garble.
fn shorten(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_string();
    }
    let kept: String = name.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{kept}...")
}

/// The one line of text that describes a download right now.
fn progress_line(s: &DownloadSnapshot) -> String {
    match s.status {
        DownloadStatus::GettingInfo => waiting_text(s.waiting_for.unwrap_or_default()),
        DownloadStatus::Checking => {
            format!(
                "{}: checking files already on disk...",
                shorten(&s.name, 40)
            )
        }
        _ => format!(
            "{}  {:5.1}%  {} / {}  {}  ETA {}  peers {}",
            shorten(&s.name, 28),
            fraction(s.progress_bytes, s.total_bytes) * 100.0,
            format_bytes(s.progress_bytes),
            format_bytes(s.total_bytes),
            format_speed(s.download_bytes_per_second),
            format_eta(s.eta),
            s.peers_connected,
        ),
    }
}

/// Draws progress. In a terminal it rewrites one line; when output is piped to a file it prints a
/// fresh line every few seconds instead, so logs stay readable.
struct ProgressPrinter {
    is_terminal: bool,
    last_width: usize,
    last_print: Option<Instant>,
}

impl ProgressPrinter {
    fn new() -> Self {
        Self {
            is_terminal: std::io::stdout().is_terminal(),
            last_width: 0,
            last_print: None,
        }
    }

    fn show(&mut self, snapshot: &DownloadSnapshot) {
        let line = progress_line(snapshot);
        if self.is_terminal {
            let width = line.chars().count();
            // Pad with spaces so a shorter line fully covers a longer one from before.
            let padding = self.last_width.saturating_sub(width);
            print!("\r{line}{}", " ".repeat(padding));
            let _ = std::io::stdout().flush();
            self.last_width = width;
        } else if self
            .last_print
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(5))
        {
            println!("{line}");
            self.last_print = Some(Instant::now());
        }
    }

    /// Moves to a fresh line so the next message doesn't land on top of the progress line.
    fn finish(&mut self) {
        if self.is_terminal && self.last_width > 0 {
            println!();
            self.last_width = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dinghy_core::DownloadId;

    fn snapshot(status: DownloadStatus) -> DownloadSnapshot {
        DownloadSnapshot {
            id: DownloadId(1),
            info_hash: "c9e15763f722f23e98a29decdfae341b98d53056".to_string(),
            name: "ubuntu-24.04-desktop-amd64.iso".to_string(),
            status,
            progress_bytes: 512 * 1024 * 1024,
            total_bytes: 2048 * 1024 * 1024,
            download_bytes_per_second: 8 * 1024 * 1024,
            upload_bytes_per_second: 0,
            eta: Some(Duration::from_secs(192)),
            peers_connected: 34,
            peers_known: 120,
            uploaded_bytes: 0,
            waiting_for: Some(Duration::from_secs(7)),
            save_folder: None,
            error: None,
        }
    }

    #[test]
    fn a_download_line_shows_progress_speed_eta_and_peers() {
        let line = progress_line(&snapshot(DownloadStatus::Downloading));
        assert!(line.contains("25.0%"), "{line}");
        assert!(line.contains("512.0 MiB / 2.0 GiB"), "{line}");
        assert!(line.contains("8.0 MiB/s"), "{line}");
        assert!(line.contains("ETA 3m 12s"), "{line}");
        assert!(line.contains("peers 34"), "{line}");
    }

    #[test]
    fn the_waiting_line_uses_the_shared_wording() {
        let line = progress_line(&snapshot(DownloadStatus::GettingInfo));
        assert!(
            line.contains("Looking for people sharing this torrent"),
            "{line}"
        );
    }

    #[test]
    fn a_saved_download_is_described_in_plain_words() {
        let mut saved = SavedDownload::new(
            "c9e15763f722f23e98a29decdfae341b98d53056",
            "Ubuntu",
            "magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056",
            Path::new("downloads"),
        );
        assert_eq!(saved_state(&saved), "looking for its file list");
        saved.torrent = Some(b"d4:infodee".to_vec());
        assert_eq!(saved_state(&saved), "active");
        saved.paused = true;
        assert_eq!(saved_state(&saved), "paused");
    }

    #[test]
    fn long_names_are_shortened() {
        assert_eq!(shorten("short", 28), "short");
        let long = shorten(&"x".repeat(100), 28);
        assert_eq!(long.chars().count(), 28);
        assert!(long.ends_with("..."));
    }
}
