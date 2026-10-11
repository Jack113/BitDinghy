//! The `dinghy` command-line program. A thin front-end: it parses what you typed and hands the
//! real work to `dinghy-core`. More commands (`pause`, `resume`...) arrive feature by feature.

use std::io::{IsTerminal, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand, ValueEnum};
use dinghy_core::display::{
    format_bytes, format_eta, format_speed, fraction, ratio_text, waiting_text,
};
use dinghy_core::input::{self, Input};
use dinghy_core::{
    AddOptions, DownloadSnapshot, DownloadStatus, Engine, FileChoice, Move, Profile, SavedDownload,
    SavedList, file_choice, owned_files, remote, torrent_file, web_link,
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
    /// Download a magnet link, a .torrent file or a web link to one, showing progress, until done
    Get(GetArgs),
    /// Show the saved download list (it works while the window is open, too)
    List,
    /// Pause a saved download, so it stays paused until you resume it
    Pause(NumberArg),
    /// Resume a paused download (it carries on when BitDinghy next runs it)
    Resume(NumberArg),
    /// Remove a download from the saved list (its files stay, unless you add --delete-files)
    Remove(RemoveArgs),
    /// Show a download's files, or choose which of them to download (--pick)
    Files(FilesArgs),
    /// Check a download's files on disk again (with the window closed, nothing is downloaded)
    Check(NumberArg),
    /// Move a download up or down the list, which is the order the queue starts them in
    Move(MoveArgs),
    /// Show where BitDinghy keeps its files and what it remembers, or change a setting
    Settings(SettingsArgs),
}

#[derive(Args)]
struct NumberArg {
    /// The download's number, as shown by `dinghy list`
    number: usize,
}

#[derive(Args)]
struct MoveArgs {
    /// The download's number, as shown by `dinghy list`
    number: usize,
    /// Where to: up, down, top or bottom
    to: Direction,
}

#[derive(Clone, Copy, ValueEnum)]
enum Direction {
    Up,
    Down,
    Top,
    Bottom,
}

impl Direction {
    fn to_move(self) -> Move {
        match self {
            Direction::Up => Move::Up,
            Direction::Down => Move::Down,
            Direction::Top => Move::Top,
            Direction::Bottom => Move::Bottom,
        }
    }
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
struct FilesArgs {
    /// The download's number, as shown by `dinghy list`
    number: usize,

    /// Download only these files: their numbers and ranges, such as 1,3-5 (or "all")
    #[arg(long, value_name = "FILES")]
    pick: Option<String>,
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
    /// Also open plain http:// links to .torrent files: "on" or "off" (https always works)
    AllowHttpLinks,
    /// How many downloads run at the same time (the others wait in the queue); 0 = no limit
    MaxActiveDownloads,
    /// The most all downloads together may download, in KiB/s (at least 32); 0 = no limit
    DownloadLimit,
    /// The most all downloads together may upload (share), in KiB/s (at least 32); 0 = no limit
    UploadLimit,
    /// A finished download stops sharing at this ratio, like 1.5 (shared 1.5 times its size); none = never
    StopSharingAtRatio,
    /// A finished download stops sharing after this many minutes of sharing; none = never
    StopSharingAfter,
    /// Accept connections from other people: "on" or "off" (used at the next start)
    AcceptIncoming,
    /// The port other people connect to; 0 = automatic (used at the next start)
    ListenPort,
    /// Ask the router to forward the port (UPnP): "on" or "off" (used at the next start)
    Upnp,
}

#[derive(Args)]
struct GetArgs {
    /// A magnet link (in quotes, because it contains & characters), a .torrent file, or an
    /// https:// link to a .torrent file
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

    /// If files with the same names are already in the download folder, use them (they are
    /// checked first and completed) instead of stopping
    #[arg(long)]
    use_existing_files: bool,

    /// Show the engine's detailed log messages (useful when something goes wrong)
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    // If dinghy crashes, it saves a report first (it stays on this computer).
    let data_dir = dinghy_core::DataDir::locate()
        .ok()
        .map(|dir| dir.path().to_path_buf());
    dinghy_core::crash::install("dinghy", data_dir);
    dinghy_core::crash::crash_if_testing();
    match Cli::parse().command {
        Some(Command::Get(args)) => get(args).await,
        Some(Command::List) => list().await,
        Some(Command::Pause(which)) => set_paused(which.number, true).await,
        Some(Command::Resume(which)) => set_paused(which.number, false).await,
        Some(Command::Remove(args)) => remove(args).await,
        Some(Command::Files(args)) => files(args).await,
        Some(Command::Check(which)) => check(which.number).await,
        Some(Command::Move(args)) => move_download(args).await,
        Some(Command::Settings(args)) => settings(args).await,
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
    let reports = dinghy_core::crash::report_folder(profile.data_dir().map(|dir| dir.path()));
    let count = std::fs::read_dir(&reports)
        .map(|entries| entries.count())
        .unwrap_or(0);
    if count > 0 {
        println!(
            "Crash reports: {count} in {} (they stay on this computer)",
            reports.display()
        );
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
    println!(
        "allow-http-links:  {}",
        if profile.settings().allow_http_links {
            "on  (plain http:// links to .torrent files are opened too)"
        } else {
            "off  (only secure https:// links)"
        }
    );
    println!(
        "download-limit:  {}",
        limit_text(profile.settings().download_limit_kib)
    );
    println!(
        "upload-limit:  {}",
        limit_text(profile.settings().upload_limit_kib)
    );
    let ratio = profile.settings().stop_sharing_ratio_percent;
    println!(
        "stop-sharing-at-ratio:  {}",
        if ratio == 0 {
            "none  (finished downloads share while BitDinghy runs)".to_string()
        } else {
            format!(
                "{}  (a finished download stops sharing once it has shared that many times its size)",
                ratio_from_percent(ratio)
            )
        }
    );
    println!(
        "stop-sharing-after:  {}",
        match profile.settings().stop_sharing_after_minutes {
            0 => "none".to_string(),
            1 => "1 minute of sharing".to_string(),
            minutes => format!("{minutes} minutes of sharing"),
        }
    );
    let settings = profile.settings();
    println!(
        "accept-incoming:  {}",
        if settings.accept_incoming {
            "on  (other people can connect: sharing works much better)"
        } else {
            "off  (only BitDinghy makes connections)"
        }
    );
    println!(
        "listen-port:  {}",
        match settings.listen_port {
            0 => "0  (automatic: a different free port at each start)".to_string(),
            port => port.to_string(),
        }
    );
    println!(
        "upnp:  {}",
        if settings.upnp {
            "on  (BitDinghy asks the router to forward the port)"
        } else {
            "off  (the router is left alone)"
        }
    );
    println!(
        "max-active-downloads:  {}",
        match profile.settings().max_active_downloads {
            0 => "0  (no limit: every download runs at once)".to_string(),
            limit => format!("{limit}  (the others wait in the queue)"),
        }
    );
}

/// A speed limit for `dinghy settings`.
fn limit_text(kib: u32) -> String {
    match kib {
        0 => "0  (no limit)".to_string(),
        kib => format!("{kib}  KiB/s ({})", format_speed(u64::from(kib) * 1024)),
    }
}

/// A ratio typed on the command line ("1.5", or "none"/"off"/"0"), in percent (150).
fn parse_ratio(value: &str) -> Option<u32> {
    let value = value.trim();
    if ["none", "off", "no"].contains(&value.to_ascii_lowercase().as_str()) {
        return Some(0);
    }
    let ratio: f64 = value.parse().ok()?;
    (ratio.is_finite() && (0.0..=1000.0).contains(&ratio)).then(|| (ratio * 100.0).round() as u32)
}

/// A ratio in percent (150) as people write it ("1.5").
fn ratio_from_percent(percent: u32) -> String {
    let text = format!("{:.2}", f64::from(percent) / 100.0);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A speed limit typed on the command line: a number of KiB/s, or "none"/"off" for no limit.
fn parse_limit(value: &str) -> Option<u32> {
    let value = value.trim();
    if ["none", "off", "no"].contains(&value.to_ascii_lowercase().as_str()) {
        return Some(0);
    }
    value.parse().ok()
}

/// "on"/"off" (and the usual other ways of saying yes or no).
fn on_or_off(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "on" | "yes" | "true" | "1" => Some(true),
        "off" | "no" | "false" | "0" => Some(false),
        _ => None,
    }
}

/// `dinghy settings` and `dinghy settings set <key> <value>`. A change also reaches the open
/// window straight away, if there is one.
async fn settings(args: SettingsArgs) -> ExitCode {
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
        SettingKey::AllowHttpLinks => match on_or_off(&value) {
            Some(allow) => profile.set_allow_http_links(allow),
            None => {
                eprintln!("allow-http-links is \"on\" or \"off\".");
                return ExitCode::FAILURE;
            }
        },
        SettingKey::MaxActiveDownloads => match value.trim().parse::<u32>() {
            Ok(limit) => profile.set_max_active_downloads(limit),
            Err(_) => {
                eprintln!("max-active-downloads is a number, such as 3 (0 means no limit).");
                return ExitCode::FAILURE;
            }
        },
        SettingKey::AcceptIncoming | SettingKey::Upnp => match on_or_off(&value) {
            Some(on) => {
                let settings = profile.settings();
                let (mut accept, port, mut upnp) = (
                    settings.accept_incoming,
                    settings.listen_port,
                    settings.upnp,
                );
                if matches!(key, SettingKey::AcceptIncoming) {
                    accept = on;
                } else {
                    upnp = on;
                }
                profile.set_connections(accept, port, upnp)
            }
            None => {
                eprintln!("This setting is \"on\" or \"off\".");
                return ExitCode::FAILURE;
            }
        },
        SettingKey::ListenPort => match value.trim().parse::<u16>() {
            Ok(port) => {
                let settings = profile.settings();
                let (accept, upnp) = (settings.accept_incoming, settings.upnp);
                profile.set_connections(accept, port, upnp)
            }
            Err(_) => {
                eprintln!("listen-port is a number between 1024 and 65535, or 0 for automatic.");
                return ExitCode::FAILURE;
            }
        },
        SettingKey::StopSharingAtRatio => match parse_ratio(&value) {
            Some(percent) => {
                profile.set_sharing_limits(percent, profile.settings().stop_sharing_after_minutes)
            }
            None => {
                eprintln!("stop-sharing-at-ratio is a number such as 1.5 (up to 1000), or none.");
                return ExitCode::FAILURE;
            }
        },
        SettingKey::StopSharingAfter => match parse_limit(&value) {
            Some(minutes) => {
                profile.set_sharing_limits(profile.settings().stop_sharing_ratio_percent, minutes)
            }
            None => {
                eprintln!("stop-sharing-after is a number of minutes, such as 120, or none.");
                return ExitCode::FAILURE;
            }
        },
        SettingKey::DownloadLimit | SettingKey::UploadLimit => match parse_limit(&value) {
            Some(kib) => {
                let (mut download, mut upload) = (
                    profile.settings().download_limit_kib,
                    profile.settings().upload_limit_kib,
                );
                if matches!(key, SettingKey::DownloadLimit) {
                    download = kib;
                } else {
                    upload = kib;
                }
                profile.set_speed_limits(download, upload)
            }
            None => {
                eprintln!(
                    "A speed limit is a number of KiB/s, such as 500 (1024 KiB/s = 1 MiB/s), or 0 \
                     for no limit."
                );
                return ExitCode::FAILURE;
            }
        },
    };
    match result {
        Ok(()) => {
            // The open window gets the change straight away (the download folder only applies
            // at its next start, as before).
            let live = match key {
                // These are only used when BitDinghy starts.
                SettingKey::DownloadDir
                | SettingKey::AcceptIncoming
                | SettingKey::ListenPort
                | SettingKey::Upnp => None,
                SettingKey::AllowHttpLinks => Some(remote::Command::SetAllowHttpLinks {
                    allow: profile.settings().allow_http_links,
                }),
                SettingKey::MaxActiveDownloads => Some(remote::Command::SetMaxActiveDownloads {
                    limit: profile.settings().max_active_downloads as usize,
                }),
                SettingKey::StopSharingAtRatio | SettingKey::StopSharingAfter => {
                    Some(remote::Command::SetSharingLimits {
                        ratio_percent: profile.settings().stop_sharing_ratio_percent,
                        minutes: profile.settings().stop_sharing_after_minutes,
                    })
                }
                SettingKey::DownloadLimit | SettingKey::UploadLimit => {
                    Some(remote::Command::SetSpeedLimits {
                        download_kib: profile.settings().download_limit_kib,
                        upload_kib: profile.settings().upload_limit_kib,
                    })
                }
            };
            let at_next_start = live.is_none();
            let window = window_client(&profile).await.ok().flatten();
            let window_open = window.is_some();
            let told_window = match (live, window) {
                (Some(command), Some(mut window)) => window.ask(command).await.is_ok(),
                _ => false,
            };
            if told_window {
                println!("Saved. BitDinghy's open window uses it straight away.");
            } else if at_next_start && window_open {
                println!(
                    "Saved. BitDinghy's open window uses it the next time it starts (close it \
                     and open it again)."
                );
            } else {
                println!("Saved. It will be used from now on, in the window too.");
            }
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
fn open_for_change(
    profile: &Profile,
    number: usize,
    verb: &str,
) -> Result<(SavedList, SavedDownload), ExitCode> {
    let Some(path) = profile.saved_list_path() else {
        eprintln!("BitDinghy has no data folder right now, so there is no saved list to change.");
        return Err(ExitCode::FAILURE);
    };
    let opened = match SavedList::open(&path) {
        Ok(opened) => opened,
        Err(dinghy_core::Error::AlreadyRunning) => {
            // The window would have answered on its channel, so this is `dinghy get` running in
            // another terminal (or a window that is still starting up).
            eprintln!(
                "BitDinghy is busy in another terminal (`dinghy get`), so this can't {verb} \
                 anything right now. Stop it with Ctrl+C first, or use the window."
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

/// `dinghy pause <n>` and `dinghy resume <n>`: through the open window, or with the window closed,
/// in the saved list.
async fn set_paused(number: usize, paused: bool) -> ExitCode {
    let verb = if paused { "pause" } else { "resume" };
    let profile = Profile::load();
    print_notices(&profile);
    match window_client(&profile).await {
        Err(code) => return code,
        Ok(Some(mut window)) => {
            let target = remote::Target::Number(number);
            let command = if paused {
                remote::Command::Pause { target }
            } else {
                remote::Command::Resume { target }
            };
            return match window.ask(command).await {
                Ok(remote::Reply::Changed { name }) => {
                    let done = if paused { "Paused" } else { "Resumed" };
                    println!("{done}: {name} (in BitDinghy's open window).");
                    ExitCode::SUCCESS
                }
                other => explain_unexpected(other),
            };
        }
        Ok(None) => {}
    }

    let (list, download) = match open_for_change(&profile, number, verb) {
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
/// it downloaded (only those: see `owned_files.rs` in dinghy-core). Through the open window, or with
/// the window closed, in the saved list.
async fn remove(args: RemoveArgs) -> ExitCode {
    let profile = Profile::load();
    print_notices(&profile);
    match window_client(&profile).await {
        Err(code) => return code,
        Ok(Some(mut window)) => return remove_in_window(&mut window, &args).await,
        Ok(None) => {}
    }

    let (list, download) = match open_for_change(&profile, args.number, "remove") {
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
    // Keeping the files: the leftovers of unticked files become visible again, so nothing
    // BitDinghy no longer looks after stays hidden.
    if !args.delete_files
        && let (Some(torrent), Some(output_folder)) = (&download.torrent, &download.output_folder)
        && let Ok(marks) = owned_files::leftover_marks(
            torrent,
            &download.download_dir,
            output_folder,
            &Default::default(),
        )
    {
        owned_files::apply_hidden(&marks);
    }
    report_removed(
        &download.name,
        &folder,
        args.delete_files,
        deleted.files,
        &deleted.problems,
    )
}

/// `dinghy remove` through the open window. With `--delete-files` the window first says exactly
/// which files would go; after "yes", the removal names the download by its fingerprint, so a list
/// that changed in the meantime can never make it remove a different download.
async fn remove_in_window(window: &mut remote::Client, args: &RemoveArgs) -> ExitCode {
    let target = if args.delete_files {
        match window
            .ask(remote::Command::FilesToDelete {
                target: remote::Target::Number(args.number),
            })
            .await
        {
            Ok(remote::Reply::Files {
                info_hash,
                name,
                paths,
            }) => {
                if !paths.is_empty() && !args.yes && !confirm_deleting(&name, &paths) {
                    println!("Nothing was removed or deleted.");
                    return ExitCode::FAILURE;
                }
                remote::Target::InfoHash(info_hash)
            }
            other => return explain_unexpected(other),
        }
    } else {
        remote::Target::Number(args.number)
    };
    match window
        .ask(remote::Command::Remove {
            target,
            delete_files: args.delete_files,
        })
        .await
    {
        Ok(remote::Reply::Removed {
            name,
            folder,
            deleted,
            problems,
        }) => report_removed(&name, &folder, args.delete_files, deleted, &problems),
        other => explain_unexpected(other),
    }
}

/// What `dinghy remove` says afterwards, the same with or without the window.
fn report_removed(
    name: &str,
    folder: &Path,
    delete_files: bool,
    deleted: usize,
    problems: &[String],
) -> ExitCode {
    println!("Removed: {name}.");
    if !delete_files {
        println!("Its files are still in {}.", folder.display());
    } else if deleted > 0 {
        println!(
            "Deleted {deleted} file{} it downloaded.",
            if deleted == 1 { "" } else { "s" }
        );
    }
    if problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        eprintln!("These files couldn't be deleted; you can delete them yourself:");
        for problem in problems {
            eprintln!("  {problem}");
        }
        ExitCode::FAILURE
    }
}

/// `dinghy files <n> [--pick 1,3-5]`: shows a download's files, or chooses which to download.
/// Through the open window, or with the window closed, in the saved list.
async fn files(args: FilesArgs) -> ExitCode {
    let profile = Profile::load();
    print_notices(&profile);
    match window_client(&profile).await {
        Err(code) => return code,
        Ok(Some(mut window)) => return files_in_window(&mut window, &args).await,
        Ok(None) => {}
    }

    let Some(pick) = &args.pick else {
        // Just looking: no lock needed, so this works while `dinghy get` runs elsewhere.
        let Some(path) = profile.saved_list_path() else {
            eprintln!(
                "BitDinghy has no data folder right now, so it doesn't remember any downloads."
            );
            return ExitCode::FAILURE;
        };
        let saved = match SavedList::read(&path) {
            Ok(saved) => saved,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        };
        let Some(download) = args
            .number
            .checked_sub(1)
            .and_then(|index| saved.get(index))
        else {
            eprintln!(
                "There is no download number {}. `dinghy list` shows the numbers.",
                args.number
            );
            return ExitCode::FAILURE;
        };
        return match saved_choices(download) {
            Ok(choices) => {
                print_files(args.number, &download.name, &choices);
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::FAILURE
            }
        };
    };

    let (list, download) = match open_for_change(&profile, args.number, "change") {
        Ok(found) => found,
        Err(code) => return code,
    };
    let chosen = saved_choices(&download).and_then(|choices| {
        let numbers =
            file_choice::parse_numbers(pick, choices.len()).map_err(|error| error.to_string())?;
        let torrent = download.torrent.as_deref().unwrap_or_default();
        let files = file_choice::files_of_torrent(torrent).map_err(|error| error.to_string())?;
        let skipped =
            file_choice::skipped_for(&files, &numbers).map_err(|error| error.to_string())?;
        list.set_skipped_files(&download.info_hash, &skipped)
            .map_err(|error| error.to_string())?;
        Ok(file_choice::choices(&files, &skipped, None))
    });
    match chosen {
        Ok(choices) => {
            report_chosen(&download.name, &choices);
            println!("It is used the next time BitDinghy runs this download.");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// A saved download's files with tick boxes (how far each got isn't known without the engine).
fn saved_choices(download: &SavedDownload) -> Result<Vec<FileChoice>, String> {
    let Some(torrent) = &download.torrent else {
        return Err(format!(
            "BitDinghy doesn't know the files of \"{}\" yet: it is still looking for its file \
             list.",
            download.name
        ));
    };
    let files = file_choice::files_of_torrent(torrent).map_err(|error| error.to_string())?;
    let skipped = file_choice::clean_skipped(&files, &download.skipped_files);
    Ok(file_choice::choices(&files, &skipped, None))
}

/// `dinghy files` through the open window. A change names the download by its fingerprint, so a
/// list that changed in the meantime can never change a different download.
async fn files_in_window(window: &mut remote::Client, args: &FilesArgs) -> ExitCode {
    let (info_hash, name, choices) = match window
        .ask(remote::Command::Files {
            target: remote::Target::Number(args.number),
        })
        .await
    {
        Ok(remote::Reply::FileList {
            info_hash,
            name,
            files,
        }) => (info_hash, name, files),
        other => return explain_unexpected(other),
    };
    let Some(pick) = &args.pick else {
        print_files(args.number, &name, &choices);
        return ExitCode::SUCCESS;
    };
    let chosen = match file_choice::parse_numbers(pick, choices.len()) {
        Ok(chosen) => chosen,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    match window
        .ask(remote::Command::ChooseFiles {
            target: remote::Target::InfoHash(info_hash),
            chosen: chosen.clone(),
        })
        .await
    {
        Ok(remote::Reply::Changed { name }) => {
            let choices: Vec<FileChoice> = choices
                .into_iter()
                .map(|choice| FileChoice {
                    chosen: chosen.contains(&choice.number),
                    ..choice
                })
                .collect();
            report_chosen(&name, &choices);
            println!("BitDinghy's open window uses it straight away.");
            ExitCode::SUCCESS
        }
        other => explain_unexpected(other),
    }
}

/// Prints a download's files with tick boxes, and how to change them.
fn print_files(number: usize, name: &str, choices: &[FileChoice]) {
    let chosen: Vec<&FileChoice> = choices.iter().filter(|choice| choice.chosen).collect();
    println!(
        "{name}: {} of {} file{} chosen ({}):",
        chosen.len(),
        choices.len(),
        if choices.len() == 1 { "" } else { "s" },
        format_bytes(chosen.iter().map(|choice| choice.size).sum())
    );
    for choice in choices {
        println!("  {}", file_line(choice));
    }
    println!(
        "Choose with: dinghy files {number} --pick {}   (numbers and ranges, or \"all\")",
        example_pick(choices.len())
    );
}

/// An example choice that fits a download with `count` files.
fn example_pick(count: usize) -> String {
    match count {
        0..=2 => "1".to_string(),
        3 => "1,3".to_string(),
        _ => format!("1,3-{}", count.min(5)),
    }
}

/// One file: `[x]  3. folder/file.iso  1.5 GiB  42%`.
fn file_line(choice: &FileChoice) -> String {
    let tick = if choice.chosen { "[x]" } else { "[ ]" };
    let mut line = format!(
        "{tick} {:>3}. {}  {}",
        choice.number,
        choice.path.display(),
        format_bytes(choice.size)
    );
    if let Some(done) = choice.done_bytes {
        line.push_str(&format!("  {:.0}%", fraction(done, choice.size) * 100.0));
    }
    line
}

/// What `dinghy files --pick` says afterwards.
fn report_chosen(name: &str, choices: &[FileChoice]) {
    let chosen: Vec<&FileChoice> = choices.iter().filter(|choice| choice.chosen).collect();
    println!(
        "{name}: now downloading {} of {} file{} ({}).",
        chosen.len(),
        choices.len(),
        if choices.len() == 1 { "" } else { "s" },
        format_bytes(chosen.iter().map(|choice| choice.size).sum())
    );
}

/// `dinghy move <n> up|down|top|bottom`: changes the list's order, which is the order the queue
/// starts downloads in. Through the open window, or with the window closed, in the saved list.
async fn move_download(args: MoveArgs) -> ExitCode {
    let profile = Profile::load();
    print_notices(&profile);
    let to = args.to.to_move();
    match window_client(&profile).await {
        Err(code) => return code,
        Ok(Some(mut window)) => {
            return match window
                .ask(remote::Command::Move {
                    target: remote::Target::Number(args.number),
                    to,
                })
                .await
            {
                Ok(remote::Reply::Changed { name }) => {
                    println!("Moved: {name} (in BitDinghy's open window).");
                    ExitCode::SUCCESS
                }
                other => explain_unexpected(other),
            };
        }
        Ok(None) => {}
    }
    let (list, download) = match open_for_change(&profile, args.number, "move") {
        Ok(found) => found,
        Err(code) => return code,
    };
    let mut order: Vec<String> = match list.all() {
        Ok(saved) => saved.into_iter().map(|d| d.info_hash).collect(),
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let from = args.number - 1;
    let place = new_place(from, order.len(), to);
    let moved = order.remove(from);
    order.insert(place, moved);
    if let Err(error) = list.set_order(&order) {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }
    println!(
        "Moved: {} is now number {} of {}.",
        download.name,
        place + 1,
        order.len()
    );
    ExitCode::SUCCESS
}

/// Where a download at `from` (counting from 0) ends up in a list of `count`.
fn new_place(from: usize, count: usize, to: Move) -> usize {
    let last = count.saturating_sub(1);
    match to {
        Move::Up => from.saturating_sub(1),
        Move::Down => (from + 1).min(last),
        Move::Top => 0,
        Move::Bottom => last,
    }
}

/// A problem reported by the window (or an answer that doesn't fit the question).
fn explain_unexpected(answer: dinghy_core::Result<remote::Reply>) -> ExitCode {
    match answer {
        Err(error) => eprintln!("{error}"),
        Ok(_) => eprintln!("BitDinghy's window gave an unexpected answer."),
    }
    ExitCode::FAILURE
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
async fn get(mut args: GetArgs) -> ExitCode {
    if args.verbose {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "warn,librqbit=info,dinghy_core=debug".into());
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .init();
    }

    // Check what was given BEFORE starting anything, so a typo or a bad file fails instantly
    // and quietly. A .torrent file is read and checked now too.
    let input = match input::classify(&args.link) {
        Ok(input) => input,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let mut clean_link = None;
    if let Input::TorrentFile(path) = &input {
        match torrent_file::read(path) {
            Ok(torrent) => clean_link = Some(torrent.magnet),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        }
        // The window (or the engine) may run in another folder: give it the full path.
        if let Ok(full) = std::path::absolute(path) {
            args.link = full.display().to_string();
        }
    }
    if let Input::Magnet(parsed) = &input {
        clean_link = Some(parsed.clean_link.clone());
    }
    // Without the DHT, only the link's trackers or --peer can find anyone. Say so now instead of
    // waiting forever. (A web link's trackers aren't known before it is fetched.)
    if args.no_dht && args.peers.is_empty() && clean_link.is_some_and(|link| !link.contains("tr="))
    {
        eprintln!(
            "With --no-dht, BitDinghy can only find people through trackers in the link or with \
             --peer, and this link has no trackers. Leave out --no-dht (or add --peer IP:PORT)."
        );
        return ExitCode::FAILURE;
    }

    let profile = Profile::load();
    print_notices(&profile);
    // The window is open? Then it owns the engine: hand the link over and follow it from here.
    // (A web link is then fetched by the window, with the window's own http setting.)
    match window_client(&profile).await {
        Err(code) => return code,
        Ok(Some(window)) => return get_through_window(window, args).await,
        Ok(None) => {}
    }
    if let Input::WebLink(link) = &input {
        let rules = web_link::Rules::new(profile.settings().allow_http_links);
        match web_link::check_link(link, rules) {
            Ok(url) => println!(
                "Fetching the .torrent file from {}...",
                url.host_str().unwrap_or_default()
            ),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        }
    }

    // Remembered settings first; `--dir` overrides the folder for this run only.
    let mut config = profile.engine_config();
    if let Some(dir) = &args.dir {
        config.download_dir = dir.clone();
    }
    config.use_dht = !args.no_dht;
    // The setting decides, unless --no-listen says no for this run.
    if args.no_listen {
        config.accept_incoming = false;
    }
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

    let options = AddOptions {
        peers: args.peers,
        use_existing_files: args.use_existing_files,
    };
    let id = match engine.add_any_with(&args.link, options).await {
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
    // A warning (such as "the drive may be too full") is said once, above the progress line.
    if let Some(warning) = &snapshot.warning
        && printer.warned.as_ref() != Some(warning)
    {
        printer.finish();
        eprintln!("Note: {warning}");
        printer.warned = Some(warning.clone());
    }
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
            if snapshot.can_use_existing_files {
                eprintln!(
                    "To use the file that is already there instead, run the same command again \
                     with --use-existing-files (BitDinghy checks it first and completes it)."
                );
            }
            Some(ExitCode::FAILURE)
        }
        _ => None,
    }
}

/// `dinghy check <n>`: checks a download's files on disk again. Through the open window, or with
/// the window closed in an engine of its own that contacts nobody and downloads nothing.
async fn check(number: usize) -> ExitCode {
    let profile = Profile::load();
    print_notices(&profile);
    match window_client(&profile).await {
        Err(code) => return code,
        Ok(Some(mut window)) => return check_in_window(&mut window, number).await,
        Ok(None) => {}
    }
    let Some(path) = profile.saved_list_path() else {
        eprintln!("BitDinghy has no data folder right now, so it doesn't remember any downloads.");
        return ExitCode::FAILURE;
    };
    let saved = match SavedList::read(&path) {
        Ok(saved) => saved,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let Some(download) = number.checked_sub(1).and_then(|index| saved.get(index)) else {
        eprintln!("There is no download number {number}. `dinghy list` shows the numbers.");
        return ExitCode::FAILURE;
    };

    // Checking needs no one else: no DHT, no listening, no local search.
    let mut config = profile.engine_config();
    config.use_dht = false;
    config.accept_incoming = false;
    config.local_discovery = false;
    config.restore_saved = false;
    let engine = match Engine::start(config).await {
        Ok(engine) => engine,
        Err(dinghy_core::Error::AlreadyRunning) => {
            eprintln!(
                "BitDinghy is busy in another terminal (`dinghy get`), so this can't check \
                 anything right now. Stop it with Ctrl+C first, or use the window."
            );
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let id = match engine.check_saved(&download.info_hash) {
        Ok(id) => id,
        Err(error) => {
            eprintln!("{error}");
            engine.shutdown().await;
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Checking the files of \"{}\" (nothing is downloaded)...",
        download.name
    );
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
                if let Some(code) = show_check(&mut printer, &snapshot) {
                    break code;
                }
            }
            _ = &mut ctrl_c => {
                printer.finish();
                println!("Stopped checking. Nothing was changed.");
                break ExitCode::from(130);
            }
        }
    };
    engine.shutdown().await;
    exit
}

/// `dinghy check` while the window is open: the window checks, this terminal follows it.
async fn check_in_window(window: &mut remote::Client, number: usize) -> ExitCode {
    // The fingerprint first, so the terminal follows the right download even if the list changes.
    let info_hash = match window.ask(remote::Command::List).await {
        Ok(remote::Reply::Downloads { downloads, .. }) => {
            match number.checked_sub(1).and_then(|index| downloads.get(index)) {
                Some(download) => download.info_hash.clone(),
                None => {
                    eprintln!(
                        "There is no download number {number} in BitDinghy's window. \
                         `dinghy list` shows the numbers."
                    );
                    return ExitCode::FAILURE;
                }
            }
        }
        other => return explain_unexpected(other),
    };
    match window
        .ask(remote::Command::Check {
            target: remote::Target::InfoHash(info_hash.clone()),
        })
        .await
    {
        Ok(remote::Reply::Changed { name }) => {
            println!("Checking the files of \"{name}\" in BitDinghy's window...");
        }
        other => return explain_unexpected(other),
    }
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
                        eprintln!("BitDinghy's window was closed during the check.");
                        return ExitCode::FAILURE;
                    }
                };
                let Some(snapshot) = downloads.iter().find(|d| d.info_hash == info_hash) else {
                    printer.finish();
                    println!("The download was removed in the window.");
                    return ExitCode::SUCCESS;
                };
                if let Some(code) = show_check(&mut printer, snapshot) {
                    return code;
                }
            }
            _ = &mut ctrl_c => {
                printer.finish();
                println!("Stopped following. The check carries on in BitDinghy's window.");
                return ExitCode::from(130);
            }
        }
    }
}

/// Draws a check's progress. Once it is done, says what it found and returns the exit code.
fn show_check(printer: &mut ProgressPrinter, snapshot: &DownloadSnapshot) -> Option<ExitCode> {
    if snapshot.status == DownloadStatus::Failed {
        printer.finish();
        eprintln!(
            "{}",
            snapshot.error.as_deref().unwrap_or("The check failed.")
        );
        return Some(ExitCode::FAILURE);
    }
    match &snapshot.note {
        Some(note) if snapshot.status != DownloadStatus::Checking => {
            printer.finish();
            println!("{note}");
            Some(ExitCode::SUCCESS)
        }
        _ => {
            printer.show(snapshot);
            None
        }
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
            use_existing_files: args.use_existing_files,
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
        if let Some(warning) = &download.warning {
            println!("     Note: {warning}");
        }
    }
    ExitCode::SUCCESS
}

/// A download's state, as the open window sees it right now.
fn live_state(download: &DownloadSnapshot) -> String {
    let percent = fraction(download.progress_bytes, download.total_bytes) * 100.0;
    match download.status {
        DownloadStatus::GettingInfo => "looking for people sharing it".to_string(),
        DownloadStatus::Checking => format!("checking files on disk, {percent:.0}%"),
        DownloadStatus::Downloading => format!(
            "{percent:.1}% of {}, {}, {} left",
            format_bytes(download.total_bytes),
            format_speed(download.download_bytes_per_second),
            format_eta(download.eta)
        ),
        DownloadStatus::Queued => "queued: starts when another download finishes".to_string(),
        DownloadStatus::Paused if download.total_bytes == 0 => "paused".to_string(),
        DownloadStatus::Paused => format!("paused at {percent:.1}%"),
        DownloadStatus::Finished if download.sharing_stopped => format!(
            "finished, stopped sharing at ratio {} ({} uploaded)",
            ratio_text(download.uploaded_bytes, download.total_bytes),
            format_bytes(download.uploaded_bytes)
        ),
        DownloadStatus::Finished => format!(
            "finished, sharing ({} uploaded, ratio {})",
            format_bytes(download.uploaded_bytes),
            ratio_text(download.uploaded_bytes, download.total_bytes)
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
                "{}: checking files already on disk... {:.0}%",
                shorten(&s.name, 40),
                fraction(s.progress_bytes, s.total_bytes) * 100.0
            )
        }
        DownloadStatus::Paused => format!(
            "{}: paused at {:.1}% ({} of {}). Resume it in the window or with dinghy resume.",
            shorten(&s.name, 40),
            fraction(s.progress_bytes, s.total_bytes) * 100.0,
            format_bytes(s.progress_bytes),
            format_bytes(s.total_bytes)
        ),
        DownloadStatus::Queued => format!(
            "{}: waiting in the queue (it starts when another download finishes)...",
            shorten(&s.name, 40)
        ),
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
    /// The last warning said, so it isn't repeated at every update.
    warned: Option<String>,
}

impl ProgressPrinter {
    fn new() -> Self {
        Self {
            is_terminal: std::io::stdout().is_terminal(),
            last_width: 0,
            last_print: None,
            warned: None,
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
            files_total: 3,
            files_chosen: 3,
            note: None,
            can_use_existing_files: false,
            shared_seconds: 0,
            sharing_stopped: false,
            warning: None,
        }
    }

    #[test]
    fn a_paused_download_says_so_instead_of_looking_stuck() {
        let line = progress_line(&snapshot(DownloadStatus::Paused));
        assert!(
            line.contains("paused at 25.0% (512.0 MiB of 2.0 GiB)"),
            "{line}"
        );
        assert!(!line.contains("ETA"), "{line}");
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
    fn a_file_line_shows_its_tick_number_size_and_progress() {
        let mut choice = FileChoice {
            number: 3,
            path: PathBuf::from("folder").join("file.iso"),
            size: 2048,
            done_bytes: Some(512),
            chosen: true,
        };
        let line = file_line(&choice);
        assert!(line.starts_with("[x]   3. "), "{line}");
        assert!(line.contains("file.iso  2.0 KiB  25%"), "{line}");
        choice.chosen = false;
        choice.done_bytes = None;
        let line = file_line(&choice);
        assert!(line.starts_with("[ ]"), "{line}");
        assert!(line.ends_with("2.0 KiB"), "{line}");
    }

    #[test]
    fn a_download_still_looking_for_its_file_list_has_no_files_to_show() {
        let saved = SavedDownload::new(
            "c9e15763f722f23e98a29decdfae341b98d53056",
            "Ubuntu",
            "magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056",
            Path::new("downloads"),
        );
        let message = saved_choices(&saved).unwrap_err();
        assert!(
            message.contains("still looking for its file list"),
            "{message}"
        );
    }

    #[test]
    fn the_example_choice_always_fits_the_download() {
        assert_eq!(example_pick(2), "1");
        assert_eq!(example_pick(3), "1,3");
        assert_eq!(example_pick(4), "1,3-4");
        assert_eq!(example_pick(400), "1,3-5");
    }

    #[test]
    fn ratios_are_typed_like_people_write_them() {
        assert_eq!(parse_ratio("1.5"), Some(150));
        assert_eq!(parse_ratio("2"), Some(200));
        assert_eq!(parse_ratio("none"), Some(0));
        assert_eq!(parse_ratio("-1"), None);
        assert_eq!(parse_ratio("5000"), None);
        assert_eq!(parse_ratio("lots"), None);
        assert_eq!(ratio_from_percent(150), "1.5");
        assert_eq!(ratio_from_percent(200), "2");
        assert_eq!(ratio_from_percent(125), "1.25");
    }

    #[test]
    fn speed_limits_are_typed_and_shown_in_kib() {
        assert_eq!(parse_limit("500"), Some(500));
        assert_eq!(parse_limit(" none "), Some(0));
        assert_eq!(parse_limit("Off"), Some(0));
        assert_eq!(parse_limit("fast"), None);
        assert_eq!(limit_text(0), "0  (no limit)");
        assert_eq!(limit_text(2048), "2048  KiB/s (2.0 MiB/s)");
    }

    #[test]
    fn moving_lands_in_the_right_place() {
        assert_eq!(new_place(2, 5, Move::Up), 1);
        assert_eq!(new_place(0, 5, Move::Up), 0, "already at the top");
        assert_eq!(new_place(2, 5, Move::Down), 3);
        assert_eq!(new_place(4, 5, Move::Down), 4, "already at the bottom");
        assert_eq!(new_place(3, 5, Move::Top), 0);
        assert_eq!(new_place(1, 5, Move::Bottom), 4);
    }

    #[test]
    fn a_queued_download_says_so() {
        let line = progress_line(&snapshot(DownloadStatus::Queued));
        assert!(line.contains("waiting in the queue"), "{line}");
        assert!(live_state(&snapshot(DownloadStatus::Queued)).starts_with("queued"));
    }

    #[test]
    fn long_names_are_shortened() {
        assert_eq!(shorten("short", 28), "short");
        let long = shorten(&"x".repeat(100), 28);
        assert_eq!(long.chars().count(), 28);
        assert!(long.ends_with("..."));
    }
}
