//! The `dinghy` command-line program. A thin front-end: it parses what you typed and hands the
//! real work to `dinghy-core`. More commands (`add`, `list`, `pause`...) arrive feature by feature.

use std::io::{IsTerminal, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand, ValueEnum};
use dinghy_core::display::{format_bytes, format_eta, format_speed, fraction, waiting_text};
use dinghy_core::{DownloadSnapshot, DownloadStatus, Engine, Profile, magnet};

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
    /// Show where BitDinghy keeps its files and what it remembers, or change a setting
    Settings(SettingsArgs),
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

    // Remembered settings first; `--dir` overrides the folder for this run only.
    let profile = Profile::load();
    print_notices(&profile);
    let mut config = profile.engine_config();
    if let Some(dir) = args.dir {
        config.download_dir = dir;
    }
    config.use_dht = !args.no_dht;
    config.accept_incoming = !args.no_listen;
    let engine = match Engine::start(config).await {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    println!("Saving to {}", engine.download_dir().display());

    let id = match engine.add_magnet(&args.link, args.peers) {
        Ok(id) => id,
        Err(error) => {
            eprintln!("{error}");
            engine.shutdown().await;
            return ExitCode::FAILURE;
        }
    };

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
                printer.show(&snapshot);
                match snapshot.status {
                    DownloadStatus::Finished => {
                        printer.finish();
                        println!(
                            "Done: {} ({}) is in {}",
                            snapshot.name,
                            format_bytes(snapshot.total_bytes),
                            snapshot.save_folder.as_deref().unwrap_or(engine.download_dir()).display()
                        );
                        break ExitCode::SUCCESS;
                    }
                    DownloadStatus::Failed => {
                        printer.finish();
                        eprintln!("{}", snapshot.error.unwrap_or_else(|| "The download failed.".into()));
                        break ExitCode::FAILURE;
                    }
                    _ => {}
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
    fn long_names_are_shortened() {
        assert_eq!(shorten("short", 28), "short");
        let long = shorten(&"x".repeat(100), 28);
        assert_eq!(long.chars().count(), 28);
        assert!(long.ends_with("..."));
    }
}
