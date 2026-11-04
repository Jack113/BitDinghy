//! The `BitDinghy` window. A thin front-end over `dinghy-core`.

// In release builds on Windows, don't pop up a black console window behind the app.
// (Debug builds keep the console so log messages are visible while developing.)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod brand;
mod logo;
mod widgets;

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use dinghy_core::default_app::{self, Kind, Opener};
use dinghy_core::display::{
    format_bytes, format_eta, format_speed, fraction, history_line, sharing_text, waiting_text,
};
use dinghy_core::input::{self, Input};
use dinghy_core::settings::{Look, MAX_ACTIVE_DOWNLOADS_LIMIT, MAX_SPEED_LIMIT_KIB, WhenFinished};
use dinghy_core::{
    DayTotal, DownloadId, DownloadSnapshot, DownloadStatus, Engine, FileChoice, Move, Profile,
    days, remote,
};
use eframe::egui;
use widgets::{Tone, banner, button, card, card_title, primary_button, quiet_button, spinner};

/// The brand's error color for the current theme (light or dark), so it stays readable on both
/// backgrounds.
fn error_color(ui: &egui::Ui) -> egui::Color32 {
    brand::palette(ui.ctx().theme()).danger
}

/// The brand's warning color for the current theme.
fn warning_color(ui: &egui::Ui) -> egui::Color32 {
    brand::palette(ui.ctx().theme()).warning
}

/// Teal: something is in progress.
fn info_color(ui: &egui::Ui) -> egui::Color32 {
    brand::palette(ui.ctx().theme()).info
}

/// A speed-limit number box (KiB/s). Returns whether the person has finished changing it (not in
/// the middle of a drag or typing), so the setting is saved once, not at every step.
fn limit_box(ui: &mut egui::Ui, kib: &mut u32) -> bool {
    let editing = ui.add(
        egui::DragValue::new(kib)
            .range(0..=MAX_SPEED_LIMIT_KIB)
            .speed(16.0),
    );
    !editing.dragged() && !editing.has_focus()
}

/// The History page (M14): today's and the last 30 days' totals, speeds of the last ten minutes,
/// each day's totals, and downloads that were removed.
fn draw_history_page(ui: &mut egui::Ui, engine: &Engine) {
    let palette = brand::palette(ui.ctx().theme());
    let (down_color, up_color) = (palette.accent, palette.info);
    let today = days::today();
    let totals = engine.daily_totals(30);
    let sums = day_sums(&totals, today);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Four small tiles with the totals.
            ui.columns(4, |columns| {
                let tiles = [
                    ("Today, downloaded", sums.today_down),
                    ("Today, uploaded", sums.today_up),
                    ("30 days, downloaded", sums.month_down),
                    ("30 days, uploaded", sums.month_up),
                ];
                for (column, (label, bytes)) in columns.iter_mut().zip(tiles) {
                    card(column, |ui| {
                        ui.small(label);
                        ui.label(
                            egui::RichText::new(format_bytes(bytes))
                                .font(brand::semibold(18.0))
                                .color(palette.text),
                        );
                    });
                }
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                card_title(ui, "Speed in the last 10 minutes (MiB/s)");
                draw_speed_chart(ui, engine, down_color, up_color);
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                card_title(ui, "Each day, last 30 days (MiB)");
                draw_days_chart(ui, &totals, today, down_color, up_color);
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                card_title(ui, "Removed downloads");
                let removed = engine.history(20);
                if removed.is_empty() {
                    ui.weak("Downloads you remove are listed here.");
                }
                for entry in &removed {
                    ui.label(history_line(entry));
                }
            });
        });
}

/// The speed lines of the last ten minutes.
fn draw_speed_chart(
    ui: &mut egui::Ui,
    engine: &Engine,
    down_color: egui::Color32,
    up_color: egui::Color32,
) {
    let speeds = engine.speed_history();
    let count = speeds.len();
    let line = |pick: fn(&(u64, u64)) -> u64| -> Vec<[f64; 2]> {
        speeds
            .iter()
            .enumerate()
            .map(|(at, sample)| {
                let seconds_ago = at as f64 - count as f64 + 1.0;
                [seconds_ago, pick(sample) as f64 / MIB]
            })
            .collect()
    };
    egui_plot::Plot::new("speeds")
        .height(110.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .include_y(0.0)
        .x_axis_label("seconds ago")
        .legend(egui_plot::Legend::default())
        .show(ui, |plot| {
            plot.line(
                egui_plot::Line::new("Downloading", line(|sample| sample.0)).color(down_color),
            );
            plot.line(egui_plot::Line::new("Uploading", line(|sample| sample.1)).color(up_color));
        });
}

/// One pair of bars (downloaded, uploaded) for each of the last 30 days.
fn draw_days_chart(
    ui: &mut egui::Ui,
    totals: &[DayTotal],
    today: i64,
    down_color: egui::Color32,
    up_color: egui::Color32,
) {
    let bars = |pick: fn(&DayTotal) -> u64, offset: f64| -> Vec<egui_plot::Bar> {
        totals
            .iter()
            .map(|total| {
                egui_plot::Bar::new(
                    (total.day - today) as f64 + offset,
                    pick(total) as f64 / MIB,
                )
                .width(0.4)
            })
            .collect()
    };
    egui_plot::Plot::new("days")
        .height(110.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .include_x(-29.5)
        .include_x(0.5)
        .include_y(0.0)
        .x_axis_label("days ago (MiB per day)")
        .legend(egui_plot::Legend::default())
        .show(ui, |plot| {
            plot.bar_chart(
                egui_plot::BarChart::new("Downloaded", bars(|t| t.downloaded_bytes, -0.2))
                    .color(down_color),
            );
            plot.bar_chart(
                egui_plot::BarChart::new("Uploaded", bars(|t| t.uploaded_bytes, 0.2))
                    .color(up_color),
            );
        });
}

/// Bytes in a MiB (the charts' unit).
const MIB: f64 = 1024.0 * 1024.0;

/// The History page's four totals.
#[derive(Debug, Default, PartialEq, Eq)]
struct DaySums {
    today_down: u64,
    today_up: u64,
    month_down: u64,
    month_up: u64,
}

/// Adds up today's and all the given days' totals.
fn day_sums(totals: &[DayTotal], today: i64) -> DaySums {
    let mut sums = DaySums::default();
    for day in totals {
        sums.month_down += day.downloaded_bytes;
        sums.month_up += day.uploaded_bytes;
        if day.day == today {
            sums.today_down += day.downloaded_bytes;
            sums.today_up += day.uploaded_bytes;
        }
    }
    sums
}

/// What an offered link is, in a few words.
fn offer_text(link: &str) -> String {
    match input::classify(link) {
        Ok(Input::Magnet(parsed)) => match parsed.display_name {
            Some(name) => format!("A magnet link: \"{name}\""),
            None => format!("A magnet link (fingerprint {})", &parsed.info_hash[..8]),
        },
        Ok(Input::TorrentFile(path)) => format!("The .torrent file {}", path.display()),
        Ok(Input::WebLink(link)) => format!("A .torrent file from {}", url_host(&link)),
        Err(error) => error.to_string(),
    }
}

/// Registers this BitDinghy and opens Windows' Default apps, where the person chooses it.
fn make_default() -> dinghy_core::Result<&'static str> {
    std::env::current_exe()
        .map_err(|e| dinghy_core::Error::Settings(e.to_string()))
        .and_then(|exe| default_app::register(&default_app::Place::real(), &exe))
        .map(|()| {
            default_app::open_default_apps();
            "Registered. Windows' Default apps is open: choose BitDinghy for .torrent and for \
             MAGNET there."
        })
}

/// What the window says after a crash.
fn crash_notice_text(folder: &Path) -> String {
    format!(
        "BitDinghy closed unexpectedly last time. A report was saved in {}. It stays on this \
         computer; if you report the problem, sending it along helps to find the cause.",
        folder.display()
    )
}

/// Opens a folder in Explorer (or the desktop's file manager).
fn open_folder(folder: &Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(folder).spawn();
}

/// Opens a web page in the person's own browser. Only BitDinghy's own fixed `https://` links
/// are ever passed here, never text from a download or the network.
fn open_web_page(link: &str) {
    if !link.starts_with("https://") {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let wide = |text: &str| -> Vec<u16> {
            std::ffi::OsStr::new(text)
                .encode_wide()
                .chain([0])
                .collect()
        };
        let (operation, target) = (wide("open"), wide(link));
        // SAFETY: both texts are zero-terminated; the window, parameters and folder may be null.
        // Windows' own "open this" starts the default browser; nothing to clean up.
        unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(link).spawn();
    }
}

/// Whether, and on which port, BitDinghy accepts connections right now.
fn listening_text(port: Option<u16>) -> String {
    match port {
        Some(port) => format!("Right now: accepting connections on port {port}."),
        None => "Right now: not accepting connections from other people.".to_string(),
    }
}

/// Whether (and which) BitDinghy opens magnet links and .torrent files.
fn default_app_text(status: &dinghy_core::default_app::Status, this_exe: Option<&Path>) -> String {
    match status {
        dinghy_core::default_app::Status::NotRegistered => {
            "BitDinghy isn't set up to open them yet.".to_string()
        }
        dinghy_core::default_app::Status::Registered { exe } if same_exe(exe, this_exe) => {
            "BitDinghy is registered to open them. To make it the default, choose it in Windows' \
             Default apps."
                .to_string()
        }
        dinghy_core::default_app::Status::Registered { exe } => format!(
            "Another copy of BitDinghy is registered to open them: {}. Make this copy the \
             default instead, or undo it.",
            exe.display()
        ),
    }
}

/// "Download limit 4.5 MiB/s, upload limit 819.0 KiB/s, 3 downloads at the same time."
fn recommended_text(recommended: &dinghy_core::recommend::Recommended) -> String {
    format!(
        "Download limit {}, upload limit {}, {} downloads at the same time.",
        limit_words(recommended.download_limit_kib),
        limit_words(recommended.upload_limit_kib),
        recommended.max_active_downloads
    )
}

/// What the sharing limits mean, after a change.
fn sharing_limits_text(ratio_percent: u32, minutes: u32) -> String {
    let ratio = (ratio_percent > 0).then(|| {
        format!(
            "once it has shared {:.2} times its size",
            f64::from(ratio_percent) / 100.0
        )
    });
    let time = (minutes > 0).then(|| {
        format!(
            "after {minutes} minute{} of sharing",
            if minutes == 1 { "" } else { "s" }
        )
    });
    match (ratio, time) {
        (None, None) => {
            "Finished downloads share for as long as BitDinghy runs, from now on.".to_string()
        }
        (Some(one), None) | (None, Some(one)) => {
            format!("A finished download stops sharing {one}, from now on.")
        }
        (Some(ratio), Some(time)) => {
            format!("A finished download stops sharing {ratio} or {time}, from now on.")
        }
    }
}

/// The three choices for "when a download is finished", as the Settings page shows them.
const WHEN_FINISHED_CHOICES: [(WhenFinished, &str); 3] = [
    (
        WhenFinished::KeepSharing,
        "Keep sharing it with others while BitDinghy runs (the standard)",
    ),
    (WhenFinished::StopRightAway, "Stop it right away"),
    (
        WhenFinished::StopAfterCheck,
        "Stop it once \"Check files\" finds it complete and correct",
    ),
];

/// What a new "when a download is finished" choice means, after a change.
fn when_finished_text(choice: WhenFinished) -> &'static str {
    match choice {
        WhenFinished::KeepSharing => {
            "Finished downloads share with others, from now on (and the ones this setting \
             stopped share again)."
        }
        WhenFinished::StopRightAway => {
            "Finished downloads stop right away, from now on (also the ones sharing now). \
             \"Share again\" lets one share anyway."
        }
        WhenFinished::StopAfterCheck => {
            "From now on, a download stops once \"Check files\" finds it complete and correct."
        }
    }
}

/// A limit in words: "none", or "500 KiB/s".
fn limit_words(kib: u32) -> String {
    match kib {
        0 => "none".to_string(),
        kib => format_speed(u64::from(kib) * 1024),
    }
}

/// All downloads' speeds added up: (downloading, uploading), in bytes per second.
fn total_speeds(downloads: &[DownloadSnapshot]) -> (u64, u64) {
    downloads.iter().fold((0, 0), |(down, up), download| {
        (
            down + download.download_bytes_per_second,
            up + download.upload_bytes_per_second,
        )
    })
}

/// The speed limits in a few words, for the sidebar.
fn limits_text((download_kib, upload_kib): (u32, u32)) -> String {
    if download_kib == 0 && upload_kib == 0 {
        "No speed limits".to_string()
    } else {
        format!(
            "Limits: {} down, {} up",
            limit_words(download_kib),
            limit_words(upload_kib)
        )
    }
}

/// The small up and down triangles on the "move in the list" buttons, and the arrows beside the
/// speeds. A test checks the fonts have them, so a font change can't turn them into empty boxes.
const UP: &str = "\u{23F6}";
const DOWN: &str = "\u{23F7}";
const ARROW_UP: &str = "\u{2191}";
const ARROW_DOWN: &str = "\u{2193}";

/// The sidebar's page symbols (from egui's own symbol font, behind Inter).
const DOWNLOADS_SYMBOL: &str = "\u{2913}";
const HISTORY_SYMBOL: &str = "\u{1F5E0}";
const SETTINGS_SYMBOL: &str = "\u{2699}";

/// How tall the logo is drawn at the top of the sidebar, in points.
const LOGO_HEIGHT: f32 = 34.0;

/// Where "Support BitDinghy" leads (the website's page, so the app never needs changing when the
/// ways to support it do).
const SUPPORT_LINK: &str = "https://bitdinghy.com/support/";

/// The window's pages, chosen in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Downloads,
    History,
    Settings,
}

/// The status badge on a download's card: its word and color.
fn status_badge(download: &DownloadSnapshot) -> (&'static str, Tone) {
    match download.status {
        DownloadStatus::GettingInfo => ("Starting", Tone::Busy),
        DownloadStatus::Checking => ("Checking", Tone::Busy),
        DownloadStatus::Downloading => ("Downloading", Tone::Busy),
        DownloadStatus::Queued => ("Queued", Tone::Waiting),
        DownloadStatus::Paused => ("Paused", Tone::Quiet),
        DownloadStatus::Finished if download.sharing_stopped => ("Finished", Tone::Done),
        DownloadStatus::Finished => ("Finished, sharing", Tone::Done),
        DownloadStatus::Failed => ("Problem", Tone::Problem),
    }
}

/// One line saying which program Windows opens magnet links or `.torrent` files with.
fn opener_text(kind: Kind, opener: &Opener, this_exe: Option<&Path>) -> (String, Tone) {
    let what = match kind {
        Kind::MagnetLinks => "Magnet links",
        Kind::TorrentFiles => ".torrent files",
    };
    match opener {
        Opener::BitDinghy { exe: Some(exe) } if same_exe(exe, this_exe) => {
            (format!("{what} open in this BitDinghy."), Tone::Done)
        }
        Opener::BitDinghy { exe: Some(exe) } => (
            format!(
                "{what} open in another copy of BitDinghy: {}",
                exe.display()
            ),
            Tone::Waiting,
        ),
        Opener::BitDinghy { exe: None } => (
            format!("{what} open in a copy of BitDinghy, but Windows doesn't say which."),
            Tone::Waiting,
        ),
        Opener::Other { name } => (format!("{what} open in {name}."), Tone::Quiet),
        Opener::NotChosen => (
            format!("{what}: no program chosen in Windows yet."),
            Tone::Quiet,
        ),
    }
}

/// Whether `exe` is this program (Windows doesn't care about upper and lower case in paths).
fn same_exe(exe: &Path, this_exe: Option<&Path>) -> bool {
    this_exe.is_some_and(|this| {
        if cfg!(windows) {
            exe.to_string_lossy().to_lowercase() == this.to_string_lossy().to_lowercase()
        } else {
            exe == this
        }
    })
}

/// Whether Windows opens both magnet links and `.torrent` files with this very copy.
fn opens_both_here(magnet: &Opener, torrent: &Opener, this_exe: Option<&Path>) -> bool {
    let here = |opener: &Opener| matches!(opener, Opener::BitDinghy { exe: Some(exe) } if same_exe(exe, this_exe));
    here(magnet) && here(torrent)
}

/// Whether to ask, once, "Open magnet links and .torrent files with BitDinghy?": only on Windows,
/// only if it was never answered, and not when this copy already opens both.
fn should_ask_about_default_app(
    already_asked: bool,
    magnet: &Opener,
    torrent: &Opener,
    this_exe: Option<&Path>,
) -> bool {
    cfg!(windows) && !already_asked && !opens_both_here(magnet, torrent, this_exe)
}

/// The whole window's state.
///
/// egui is an "immediate mode" toolkit: instead of building widgets once and updating them,
/// `ui()` below is called again for every frame drawn and describes the screen from scratch,
/// using the latest numbers from the engine.
struct BitDinghyApp {
    /// Owns the background threads the engine runs on. It must outlive the engine.
    runtime: tokio::runtime::Runtime,
    /// Where BitDinghy's files are and what it remembers (shared with the command line).
    profile: Profile,
    /// `Err` holds the reason the engine couldn't start, so the window can show it.
    engine: Result<Arc<Engine>, String>,
    /// The still logo pictures. `None` if they couldn't be read, in which case plain text is shown.
    logos: Option<brand::Logos>,
    /// The animated logo at the top of the sidebar (the still one is shown if it can't load).
    animated_logo: Option<logo::AnimatedLogo>,
    /// The text in the link box.
    link: String,
    /// A problem with the link just pasted.
    link_problem: Option<String>,
    /// The text in the "Download folder" box of the settings.
    folder_text: String,
    /// The result of the last settings change: the message, and whether it is an error.
    settings_message: Option<(String, bool)>,
    /// The result of the last Pause/Resume/Remove press: the message, and whether it is an error.
    control_message: Option<(String, bool)>,
    /// The download whose "Remove?" question is showing, if any.
    confirming_remove: Option<DownloadId>,
    /// The downloads whose file list (with tick boxes) is open.
    showing_files: HashSet<DownloadId>,
    /// Crash reports saved since the person was last told (M13).
    crash_reports: Vec<std::path::PathBuf>,
    /// Links other programs asked BitDinghy to open, waiting for the person's yes (M16).
    offers: Vec<String>,
    /// The "downloads at the same time" number being edited in the settings.
    max_active_choice: u32,
    /// The speed limits being edited in the settings (KiB/s, 0: no limit): download, upload.
    limits_choice: (u32, u32),
    /// The sharing limits being edited in the settings: ratio (1.5) and minutes (0: none).
    sharing_choice: (f64, u32),
    /// The connection settings being edited: accept incoming, port (0: automatic), UPnP.
    connections_choice: (bool, u16, bool),
    /// Why the command line can't talk to this window, if it can't.
    channel_problem: Option<String>,
    /// A web link being fetched in the background, if any.
    fetching: Option<Fetching>,
    /// Button actions still running in the background (see [`Running`]).
    running: Vec<Running>,
    /// The page chosen in the sidebar.
    page: Page,
    /// Ask once "Open magnet links and .torrent files with BitDinghy?" (decided at start).
    asking_about_default_app: bool,
    /// The answer to a question in a banner (the message, and whether it is an error), shown
    /// until the person presses OK.
    notice: Option<(String, bool)>,
}

/// A web link to a `.torrent` file being fetched in the background, so the window never freezes.
struct Fetching {
    /// The web site's name, for the "Fetching..." line.
    host: String,
    task: tokio::task::JoinHandle<dinghy_core::Result<DownloadId>>,
}

/// A button action (Pause, Resume, Check files, choosing files, Remove) running in the
/// background. The engine library can take many seconds to pause a busy download; if the window
/// waited, it would freeze, and clicks made meanwhile would land on whatever button is under the
/// mouse afterwards (a Pause click turning into Resume). So the window keeps drawing, and the
/// download's buttons are greyed out until the action is done.
struct Running {
    id: DownloadId,
    task: tokio::task::JoinHandle<dinghy_core::Result<Option<(String, bool)>>>,
}

impl BitDinghyApp {
    fn new(creation_context: &eframe::CreationContext<'_>, opened_with: Option<String>) -> Self {
        let profile = Profile::load();
        brand::apply(&creation_context.egui_ctx, profile.settings().look);
        let logos = brand::Logos::load(&creation_context.egui_ctx);
        let animated_logo = logo::AnimatedLogo::load(&creation_context.egui_ctx);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("couldn't start BitDinghy's background threads");
        let engine = runtime
            .block_on(Engine::start(profile.engine_config()))
            .map(Arc::new)
            .map_err(|error| error.to_string());
        // Let `dinghy` (the command line) talk to this window while it is open. The channel only
        // works on this computer (see `remote.rs` in dinghy-core).
        let channel_problem = match (&engine, profile.data_dir()) {
            (Ok(engine), Some(data_dir)) => {
                let _inside_runtime = runtime.enter();
                let address = remote::Address::for_data_dir(data_dir.path());
                remote::start_server(engine.clone(), &address)
                    .err()
                    .map(|error| error.to_string())
            }
            _ => None,
        };
        let crash_reports =
            dinghy_core::crash::unseen_reports(profile.data_dir().map(|dir| dir.path()));
        let max_active_choice = profile.settings().max_active_downloads;
        let connections_choice = (
            profile.settings().accept_incoming,
            profile.settings().listen_port,
            profile.settings().upnp,
        );
        let sharing_choice = (
            f64::from(profile.settings().stop_sharing_ratio_percent) / 100.0,
            profile.settings().stop_sharing_after_minutes,
        );
        let limits_choice = (
            profile.settings().download_limit_kib,
            profile.settings().upload_limit_kib,
        );
        let folder_text = profile
            .settings()
            .download_dir_or_default()
            .display()
            .to_string();
        let asking_about_default_app = {
            let place = default_app::Place::real();
            should_ask_about_default_app(
                profile.settings().asked_about_default_app,
                &default_app::opener(&place, Kind::MagnetLinks),
                &default_app::opener(&place, Kind::TorrentFiles),
                std::env::current_exe().ok().as_deref(),
            )
        };
        // When the engine couldn't start, the fix is probably in the settings.
        let page = if engine.is_err() {
            Page::Settings
        } else {
            Page::Downloads
        };
        Self {
            page,
            asking_about_default_app,
            notice: None,
            runtime,
            profile,
            engine,
            logos,
            animated_logo,
            link: String::new(),
            link_problem: None,
            folder_text,
            settings_message: None,
            control_message: None,
            confirming_remove: None,
            showing_files: HashSet::new(),
            crash_reports,
            offers: opened_with.into_iter().collect(),
            max_active_choice,
            limits_choice,
            sharing_choice,
            connections_choice,
            channel_problem,
            fetching: None,
            running: Vec::new(),
        }
    }

    /// Adds what is in the link box: a magnet link or the path of a `.torrent` file.
    /// Adds what is in the link box: a magnet link, the path of a `.torrent` file, or a web link
    /// to one. A web link is fetched in the background (see [`Fetching`]).
    fn submit_link(&mut self, engine: &Arc<Engine>) {
        if let Ok(Input::WebLink(link)) = input::classify(&self.link) {
            if self.fetching.is_some() {
                self.link_problem = Some(
                    "Wait until the .torrent file being fetched has arrived, then try again."
                        .to_string(),
                );
                return;
            }
            let host = url_host(&link);
            let engine = engine.clone();
            let task = self
                .runtime
                .spawn(async move { engine.add_any(&link, Vec::new()).await });
            self.fetching = Some(Fetching { host, task });
            self.link_problem = None;
            return;
        }
        match engine.add(&self.link, Vec::new()) {
            Ok(_) => {
                self.link.clear();
                self.link_problem = None;
            }
            Err(error) => self.link_problem = Some(error.to_string()),
        }
    }

    /// When a background fetch has finished: clear the box, or show what went wrong.
    fn collect_fetch(&mut self) {
        if !self
            .fetching
            .as_ref()
            .is_some_and(|fetching| fetching.task.is_finished())
        {
            return;
        }
        let Some(finished) = self.fetching.take() else {
            return;
        };
        match self.runtime.block_on(finished.task) {
            Ok(Ok(_)) => {
                self.link.clear();
                self.link_problem = None;
            }
            Ok(Err(error)) => self.link_problem = Some(error.to_string()),
            Err(_) => {
                self.link_problem = Some("Fetching the .torrent file stopped unexpectedly.".into())
            }
        }
    }

    /// Adds `.torrent` files (chosen in the file dialog, or dropped on the window). Problems are
    /// collected and shown under the link box.
    fn add_files(&mut self, engine: &Engine, paths: &[std::path::PathBuf]) {
        let problems: Vec<String> = paths
            .iter()
            .filter_map(|path| {
                engine
                    .add(&path.display().to_string(), Vec::new())
                    .err()
                    .map(|error| error.to_string())
            })
            .collect();
        self.link_problem = (!problems.is_empty()).then(|| problems.join("\n"));
    }

    /// Remembers the folder typed in the settings (or the standard folder when `use_standard`).
    fn save_download_folder(&mut self, use_standard: bool) {
        // People often paste a path with quotation marks around it.
        let typed = self.folder_text.trim().trim_matches('"').trim();
        let result = if use_standard || typed.is_empty() {
            self.profile.set_download_dir(None)
        } else {
            self.profile.set_download_dir(Some(Path::new(typed)))
        };
        self.settings_message = Some(match result {
            Ok(()) => {
                let now = self.profile.settings().download_dir_or_default();
                self.folder_text = now.display().to_string();
                (
                    format!(
                        "Saved: {}. It is used the next time you start BitDinghy.",
                        now.display()
                    ),
                    false,
                )
            }
            Err(error) => (error.to_string(), true),
        });
    }

    /// Carries out a button press on one download. Actions that wait for the engine run in the
    /// background (see [`Running`]); their result is shown when they finish.
    fn act(&mut self, engine: &Arc<Engine>, downloads: &[DownloadSnapshot], control: Control) {
        let name_of = |id| {
            downloads
                .iter()
                .find(|download| download.id == id)
                .map(|download| download.name.clone())
                .unwrap_or_default()
        };
        let outcome = match control {
            Control::Pause(id) => {
                let engine = engine.clone();
                self.run(id, async move { engine.pause(id).await.map(|()| None) });
                return;
            }
            Control::Resume(id) => {
                let engine = engine.clone();
                self.run(id, async move { engine.resume(id).await.map(|()| None) });
                return;
            }
            Control::AskRemove(id) => {
                self.confirming_remove = Some(id);
                Ok(None)
            }
            Control::CancelRemove => {
                self.confirming_remove = None;
                Ok(None)
            }
            Control::ToggleFiles(id) => {
                if !self.showing_files.remove(&id) {
                    self.showing_files.insert(id);
                }
                Ok(None)
            }
            Control::Check(id) => {
                let engine = engine.clone();
                self.run(id, async move { engine.check(id).await.map(|()| None) });
                return;
            }
            Control::Move { id, to } => engine.move_download(id, to).map(|()| None),
            Control::UseExistingFiles(id) => engine.use_existing_files(id).map(|()| None),
            Control::ChooseFiles { id, chosen } => {
                let engine = engine.clone();
                self.run(id, async move {
                    engine.choose_files(id, &chosen).await.map(|()| None)
                });
                return;
            }
            Control::Remove { id, delete_files } => {
                self.confirming_remove = None;
                self.showing_files.remove(&id);
                let name = name_of(id);
                let folder = engine
                    .download_dir_of(id)
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_default();
                let engine = engine.clone();
                self.run(id, async move {
                    engine.remove(id, delete_files).await.map(|deleted| {
                        Some(removed_message(&name, &folder, delete_files, &deleted))
                    })
                });
                return;
            }
        };
        self.show_outcome(outcome);
    }

    /// Starts a button's action in the background.
    fn run(
        &mut self,
        id: DownloadId,
        action: impl Future<Output = dinghy_core::Result<Option<(String, bool)>>> + Send + 'static,
    ) {
        self.control_message = None;
        let task = self.runtime.spawn(action);
        self.running.push(Running { id, task });
    }

    /// Shows the result of the background actions that have finished.
    fn collect_running(&mut self) {
        let (finished, still): (Vec<Running>, Vec<Running>) = std::mem::take(&mut self.running)
            .into_iter()
            .partition(|running| running.task.is_finished());
        self.running = still;
        for running in finished {
            let outcome = match self.runtime.block_on(running.task) {
                Ok(outcome) => outcome,
                Err(_) => Ok(Some(("That stopped unexpectedly.".to_string(), true))),
            };
            self.show_outcome(outcome);
        }
    }

    fn show_outcome(&mut self, outcome: dinghy_core::Result<Option<(String, bool)>>) {
        self.control_message = match outcome {
            Ok(None) => None,
            Ok(Some(message)) => Some(message),
            Err(error) => Some((error.to_string(), true)),
        };
    }

    /// The first link another program asked to open, with Download and Not now.
    fn draw_offer(&mut self, ui: &mut egui::Ui, engine: &Arc<Engine>) {
        let Some(link) = self.offers.first().cloned() else {
            return;
        };
        let mut answer = None;
        banner(ui, Tone::Busy, |ui| {
            ui.colored_label(
                info_color(ui),
                "Another program asked BitDinghy to open this. Download it?",
            );
            ui.label(offer_text(&link));
            ui.horizontal(|ui| {
                if primary_button(ui, "Download").clicked() {
                    answer = Some(true);
                }
                if button(ui, "Not now").clicked() {
                    answer = Some(false);
                }
            });
        });
        ui.add_space(8.0);
        match answer {
            Some(true) => {
                self.offers.remove(0);
                self.link = link;
                self.page = Page::Downloads;
                self.submit_link(engine);
            }
            Some(false) => {
                self.offers.remove(0);
            }
            None => {}
        }
    }

    /// After a crash: says a report was saved, with a button to open its folder.
    fn draw_crash_notice(&mut self, ui: &mut egui::Ui) {
        let Some(newest) = self.crash_reports.last() else {
            return;
        };
        let folder = newest.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut done = false;
        banner(ui, Tone::Waiting, |ui| {
            ui.colored_label(warning_color(ui), crash_notice_text(&folder));
            ui.horizontal(|ui| {
                if button(ui, "Open the folder").clicked() {
                    open_folder(&folder);
                }
                done = button(ui, "OK").clicked();
            });
        });
        ui.add_space(8.0);
        if done {
            dinghy_core::crash::mark_seen(&self.crash_reports);
            self.crash_reports.clear();
        }
    }

    /// The once-only question: open magnet links and .torrent files with BitDinghy? Either answer
    /// is remembered, so it isn't asked again (Settings keeps the buttons for later).
    fn draw_default_app_question(&mut self, ui: &mut egui::Ui) {
        if !self.asking_about_default_app {
            return;
        }
        let mut answer = None;
        banner(ui, Tone::Busy, |ui| {
            ui.label(
                egui::RichText::new("Open magnet links and .torrent files with BitDinghy?")
                    .font(brand::semibold(13.5))
                    .color(info_color(ui)),
            );
            ui.small(
                "Windows lets you choose this yourself: BitDinghy registers itself (for your \
                 Windows user only) and opens Windows' Default apps, where you pick BitDinghy. \
                 You can change it later in Settings.",
            );
            ui.horizontal(|ui| {
                if primary_button(ui, "Yes").clicked() {
                    answer = Some(true);
                }
                if button(ui, "Not now").clicked() {
                    answer = Some(false);
                }
            });
        });
        ui.add_space(8.0);
        let Some(yes) = answer else {
            return;
        };
        self.asking_about_default_app = false;
        let remembered = self.profile.set_asked_about_default_app();
        let outcome = if yes {
            make_default()
        } else {
            Ok("OK. If you change your mind: Settings, \"Magnet links and .torrent files\".")
        };
        self.notice = Some(match (outcome, remembered) {
            (Err(error), _) | (Ok(_), Err(error)) => (error.to_string(), true),
            (Ok(message), Ok(())) => (message.to_string(), false),
        });
    }

    /// Everything shown above the page: crash report, problems, questions, offers.
    fn draw_notices(&mut self, ui: &mut egui::Ui, engine: Option<&Arc<Engine>>) {
        self.draw_crash_notice(ui);
        let mut problems: Vec<String> = self.profile.notices().to_vec();
        match (&self.engine, engine) {
            (Err(message), _) => problems.push(message.clone()),
            (Ok(_), Some(engine)) => problems.extend(engine.notices()),
            _ => {}
        }
        problems.extend(self.channel_problem.clone());
        for problem in problems {
            banner(ui, Tone::Waiting, |ui| {
                ui.colored_label(warning_color(ui), problem);
            });
            ui.add_space(8.0);
        }
        self.draw_default_app_question(ui);
        if let Some((message, is_error)) = self.notice.clone() {
            let tone = if is_error { Tone::Problem } else { Tone::Done };
            let mut done = false;
            banner(ui, tone, |ui| {
                ui.horizontal(|ui| {
                    done = button(ui, "OK").clicked();
                    ui.label(message);
                });
            });
            ui.add_space(8.0);
            if done {
                self.notice = None;
            }
        }
        if let Some(engine) = engine {
            self.draw_offer(ui, engine);
        }
    }

    /// The sidebar: the logo, the three pages, and the total speeds at the bottom.
    fn draw_sidebar(&mut self, ui: &mut egui::Ui, engine: Option<&Arc<Engine>>, count: usize) {
        let downloading = engine.is_some_and(|engine| {
            engine
                .snapshot()
                .iter()
                .any(|download| download.status == DownloadStatus::Downloading)
        });
        match (&self.animated_logo, &self.logos) {
            (Some(animated), _) => animated.show(ui, downloading),
            (None, Some(logos)) => {
                let texture = logos.for_theme(ui.ctx().theme());
                let width = (LOGO_HEIGHT * texture.aspect_ratio()).min(ui.available_width());
                let size = egui::vec2(width, width / texture.aspect_ratio());
                ui.add(
                    egui::Image::from_texture(egui::load::SizedTexture::from_handle(texture))
                        .fit_to_exact_size(size),
                );
            }
            (None, None) => {
                ui.heading(dinghy_core::APP_NAME);
            }
        }
        ui.add_space(14.0);
        let pages = [
            (Page::Downloads, DOWNLOADS_SYMBOL, "Downloads", Some(count)),
            (Page::History, HISTORY_SYMBOL, "History", None),
            (Page::Settings, SETTINGS_SYMBOL, "Settings", None),
        ];
        for (page, symbol, label, count) in pages {
            let count = count.filter(|&count| count > 0);
            if widgets::nav_item(ui, self.page == page, symbol, label, count).clicked() {
                self.page = page;
            }
            ui.add_space(2.0);
        }
        // From the bottom up: the version, the limits, then the speeds.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.weak(format!("Version {}", dinghy_core::VERSION));
            let Some(engine) = engine else {
                return;
            };
            ui.small(limits_text(engine.speed_limits()));
            let palette = brand::palette(ui.ctx().theme());
            let (down, up) = total_speeds(&engine.snapshot());
            ui.label(
                egui::RichText::new(format!("{ARROW_UP} {}", format_speed(up))).color(palette.info),
            )
            .on_hover_text("Uploading (sharing), all downloads together");
            ui.label(
                egui::RichText::new(format!("{ARROW_DOWN} {}", format_speed(down)))
                    .color(palette.accent),
            )
            .on_hover_text("Downloading, all downloads together");
            ui.separator();
        });
    }

    /// The Settings page: the settings in groups (cards), with the result of the last change in a
    /// line along the bottom, where it is seen whichever group was changed.
    fn draw_settings_page(&mut self, ui: &mut egui::Ui) {
        let mut save = false;
        let mut use_standard = false;
        let mut change_http = None;
        let mut change_max_active = None;
        let mut change_limits = None;
        let mut change_sharing = None;
        let mut change_connections = None;
        let mut use_recommended = None;
        let mut default_app = None;
        let mut change_look = None;
        let mut change_when_finished = None;
        let fastest = self
            .engine
            .as_ref()
            .ok()
            .map(|engine| engine.fastest_seen());
        let listening_now = self.engine.as_ref().ok().map(|engine| engine.listen_port());

        if let Some((message, is_error)) = &self.settings_message {
            let color = if *is_error {
                error_color(ui)
            } else {
                info_color(ui)
            };
            egui::Panel::bottom("settings-message")
                .frame(egui::Frame::new().inner_margin(egui::Margin {
                    left: 0,
                    right: 0,
                    top: 8,
                    bottom: 0,
                }))
                .show(ui, |ui| {
                    ui.colored_label(color, message);
                });
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                card(ui, |ui| {
                    card_title(ui, "Downloads");
                    ui.horizontal(|ui| {
                        ui.label("Download folder:");
                        // Room for the two buttons beside it, so the card never gets wider than
                        // the page.
                        let width = (ui.available_width() - 290.0).max(120.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.folder_text).desired_width(width),
                        );
                        save |= button(ui, "Save").clicked();
                        use_standard |= quiet_button(ui, "Use the standard folder").clicked();
                    });
                    ui.small("A new download folder is used the next time you start BitDinghy.");
                    ui.horizontal(|ui| {
                        ui.label("Downloads at the same time:");
                        let editing = ui.add(
                            egui::DragValue::new(&mut self.max_active_choice)
                                .range(0..=MAX_ACTIVE_DOWNLOADS_LIMIT),
                        );
                        // Saved once the person has finished changing it (not at every step of a
                        // drag).
                        if self.max_active_choice != self.profile.settings().max_active_downloads
                            && !editing.dragged()
                            && !editing.has_focus()
                        {
                            change_max_active = Some(self.max_active_choice);
                        }
                    });
                    ui.small(
                        "0 = no limit. The others wait in the queue; finished downloads that are \
                         sharing don't count.",
                    );
                    let mut allow_http = self.profile.settings().allow_http_links;
                    if ui
                        .checkbox(
                            &mut allow_http,
                            "Also open plain http:// links to .torrent files (less safe: they can \
                             be changed on the way). Secure https:// links always work.",
                        )
                        .changed()
                    {
                        change_http = Some(allow_http);
                    }
                });
                ui.add_space(8.0);

                card(ui, |ui| {
                    card_title(ui, "Speed");
                    let saved_limits = (
                        self.profile.settings().download_limit_kib,
                        self.profile.settings().upload_limit_kib,
                    );
                    let mut settled = true;
                    ui.horizontal(|ui| {
                        ui.label("Download limit:");
                        settled &= limit_box(ui, &mut self.limits_choice.0);
                        ui.label("Upload limit:");
                        settled &= limit_box(ui, &mut self.limits_choice.1);
                    });
                    ui.small("KiB/s, for all downloads together; 0 = no limit, else at least 32.");
                    if settled && self.limits_choice != saved_limits {
                        change_limits = Some(self.limits_choice);
                    }
                    ui.add_space(4.0);
                    ui.label("Recommended settings (learned from your own downloads):");
                    match fastest.and_then(dinghy_core::recommend::recommend) {
                        None => {
                            ui.weak(dinghy_core::recommend::NOTHING_LEARNED);
                        }
                        Some(recommended) => {
                            ui.label(dinghy_core::recommend::fastest_text(
                                fastest.unwrap_or_default(),
                            ));
                            ui.horizontal(|ui| {
                                ui.label(recommended_text(&recommended));
                                if button(ui, "Use these").clicked() {
                                    use_recommended = Some(recommended);
                                }
                            });
                            ui.small(dinghy_core::recommend::REASONS);
                        }
                    }
                });
                ui.add_space(8.0);

                card(ui, |ui| {
                    card_title(ui, "Sharing");
                    ui.label("When a download is finished (and so checked):");
                    let now = self.profile.settings().when_finished;
                    for (choice, label) in WHEN_FINISHED_CHOICES {
                        if ui.radio(now == choice, label).clicked() && now != choice {
                            change_when_finished = Some(choice);
                        }
                    }
                    ui.add_space(4.0);
                    let saved_sharing = (
                        self.profile.settings().stop_sharing_ratio_percent,
                        self.profile.settings().stop_sharing_after_minutes,
                    );
                    let mut settled = true;
                    ui.horizontal(|ui| {
                        ui.label("Stop sharing a finished download at ratio");
                        let ratio = ui.add(
                            egui::DragValue::new(&mut self.sharing_choice.0)
                                .range(0.0..=1000.0)
                                .speed(0.05)
                                .fixed_decimals(2),
                        );
                        settled &= !ratio.dragged() && !ratio.has_focus();
                        ui.label("or after");
                        let minutes = ui.add(
                            egui::DragValue::new(&mut self.sharing_choice.1)
                                .range(0..=dinghy_core::settings::MAX_SHARING_MINUTES),
                        );
                        settled &= !minutes.dragged() && !minutes.has_focus();
                        ui.label("minutes");
                    });
                    ui.small("0 = never. Ratio 1 = it has shared as much as its size.");
                    let chosen_sharing = (
                        (self.sharing_choice.0 * 100.0).round() as u32,
                        self.sharing_choice.1,
                    );
                    if settled && chosen_sharing != saved_sharing {
                        change_sharing = Some(chosen_sharing);
                    }
                });
                ui.add_space(8.0);

                card(ui, |ui| {
                    card_title(ui, "Connections");
                    ui.small("Used the next time you start BitDinghy.");
                    let saved_connections = (
                        self.profile.settings().accept_incoming,
                        self.profile.settings().listen_port,
                        self.profile.settings().upnp,
                    );
                    ui.checkbox(
                        &mut self.connections_choice.0,
                        "Accept connections from other people (sharing works much better)",
                    );
                    let mut settled = true;
                    ui.add_enabled_ui(self.connections_choice.0, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Port:");
                            let port = ui.add(egui::DragValue::new(&mut self.connections_choice.1));
                            settled &= !port.dragged() && !port.has_focus();
                            ui.weak("(0 = automatic: a different free port at each start)");
                        });
                        ui.checkbox(
                            &mut self.connections_choice.2,
                            "Ask my router to forward this port (UPnP), so people outside your \
                             home network can connect",
                        );
                    });
                    if settled && self.connections_choice != saved_connections {
                        change_connections = Some(self.connections_choice);
                    }
                    ui.small(listening_text(listening_now.flatten()));
                    ui.small(dinghy_core::ENCRYPTION_NOTE);
                    ui.small(
                        "The first time BitDinghy accepts connections, Windows Firewall may ask \
                         whether to allow it. At home, allowing it on private networks is enough.",
                    );
                });
                ui.add_space(8.0);

                card(ui, |ui| {
                    card_title(ui, "Magnet links and .torrent files");
                    let this_exe = std::env::current_exe().ok();
                    let place = default_app::Place::real();
                    for kind in [Kind::MagnetLinks, Kind::TorrentFiles] {
                        let (text, tone) = opener_text(
                            kind,
                            &default_app::opener(&place, kind),
                            this_exe.as_deref(),
                        );
                        let color = tone.color(&widgets::palette(ui));
                        ui.colored_label(color, text);
                    }
                    let status = default_app::status(&place);
                    ui.small(default_app_text(&status, this_exe.as_deref()));
                    ui.horizontal(|ui| {
                        let registered_here = matches!(
                            &status,
                            default_app::Status::Registered { exe }
                                if same_exe(exe, this_exe.as_deref())
                        );
                        if !registered_here
                            && button(ui, "Make BitDinghy the default...")
                                .on_hover_text(
                                    "Registers BitDinghy (for your Windows user only) and opens \
                                     Windows' Default apps, where you choose it.",
                                )
                                .clicked()
                        {
                            default_app = Some(true);
                        }
                        if registered_here && button(ui, "Open Default apps").clicked() {
                            default_app::open_default_apps();
                        }
                        if status != default_app::Status::NotRegistered
                            && quiet_button(ui, "Undo")
                                .on_hover_text("Removes everything BitDinghy registered.")
                                .clicked()
                        {
                            default_app = Some(false);
                        }
                    });
                });
                ui.add_space(8.0);

                card(ui, |ui| {
                    card_title(ui, "Look");
                    let now = self.profile.settings().look;
                    ui.horizontal(|ui| {
                        ui.label("Colors:");
                        for (look, label) in [
                            (Look::Dark, "Dark"),
                            (Look::Light, "Light"),
                            (Look::LikeWindows, "Like Windows"),
                        ] {
                            if ui.selectable_label(now == look, label).clicked() && now != look {
                                change_look = Some(look);
                            }
                        }
                    });
                    ui.small("\"Like Windows\" follows Windows' own light or dark setting.");
                });
                ui.add_space(8.0);

                card(ui, |ui| {
                    card_title(ui, "About");
                    ui.label(format!(
                        "{} {}: a fast, lightweight download tool for magnet links and .torrent \
                         files.",
                        dinghy_core::APP_NAME,
                        dinghy_core::VERSION
                    ));
                    match self.profile.data_dir() {
                        Some(dir) => {
                            ui.label(format!(
                                "BitDinghy keeps its files in {} ({}).",
                                dir.path().display(),
                                dir.kind().describe()
                            ));
                        }
                        None => {
                            ui.colored_label(
                                error_color(ui),
                                "There is nowhere to keep BitDinghy's files, so settings can't \
                                 be remembered.",
                            );
                        }
                    }
                    ui.horizontal(|ui| {
                        if let Some(dir) = self.profile.data_dir()
                            && quiet_button(ui, "Open data folder").clicked()
                        {
                            open_folder(dir.path());
                        }
                        // Not `hyperlink_to`: egui only asks for the link to be opened, and the
                        // part of eframe that would do it is switched off in this build, so the
                        // click did nothing (found in the guided test). Windows opens it.
                        if ui
                            .link("Support BitDinghy")
                            .on_hover_text(SUPPORT_LINK)
                            .clicked()
                        {
                            open_web_page(SUPPORT_LINK);
                        }
                    });
                });
            });

        if let Some(choice) = change_when_finished {
            self.settings_message = Some(match self.profile.set_when_finished(choice) {
                Ok(()) => {
                    if let Ok(engine) = &self.engine {
                        engine.set_when_finished(choice);
                    }
                    (when_finished_text(choice).to_string(), false)
                }
                Err(error) => (error.to_string(), true),
            });
        }
        if let Some(look) = change_look {
            brand::set_look(ui.ctx(), look);
            self.settings_message = Some(match self.profile.set_look(look) {
                Ok(()) => (
                    "Saved: the new colors are used from now on.".to_string(),
                    false,
                ),
                Err(error) => (error.to_string(), true),
            });
        }
        if save || use_standard {
            self.save_download_folder(use_standard);
        }
        if let Some(make_it_default) = default_app {
            let result = if make_it_default {
                make_default()
            } else {
                default_app::unregister(&default_app::Place::real()).map(|()| {
                    "BitDinghy is no longer registered. Windows asks which program to use the \
                     next time you open a magnet link or a .torrent file."
                })
            };
            self.settings_message = Some(match result {
                Ok(message) => (message.to_string(), false),
                Err(error) => (error.to_string(), true),
            });
        }
        if let Some(recommended) = use_recommended {
            let saved = self
                .profile
                .set_speed_limits(recommended.download_limit_kib, recommended.upload_limit_kib)
                .and_then(|()| {
                    self.profile
                        .set_max_active_downloads(recommended.max_active_downloads)
                });
            self.settings_message = Some(match saved {
                Ok(()) => {
                    if let Ok(engine) = &self.engine {
                        engine.set_speed_limits(
                            recommended.download_limit_kib,
                            recommended.upload_limit_kib,
                        );
                        engine.set_max_active_downloads(recommended.max_active_downloads as usize);
                    }
                    self.limits_choice =
                        (recommended.download_limit_kib, recommended.upload_limit_kib);
                    self.max_active_choice = recommended.max_active_downloads;
                    (
                        "The recommended settings are used from now on.".to_string(),
                        false,
                    )
                }
                Err(error) => (error.to_string(), true),
            });
        }
        if let Some((accept, port, upnp)) = change_connections {
            self.settings_message = Some(match self.profile.set_connections(accept, port, upnp) {
                Ok(()) => (
                    "Saved. It is used the next time you start BitDinghy.".to_string(),
                    false,
                ),
                Err(error) => {
                    let settings = self.profile.settings();
                    self.connections_choice = (
                        settings.accept_incoming,
                        settings.listen_port,
                        settings.upnp,
                    );
                    (error.to_string(), true)
                }
            });
        }
        if let Some((ratio_percent, minutes)) = change_sharing {
            self.settings_message = Some(
                match self.profile.set_sharing_limits(ratio_percent, minutes) {
                    Ok(()) => {
                        if let Ok(engine) = &self.engine {
                            engine.set_sharing_limits(ratio_percent, minutes);
                        }
                        (sharing_limits_text(ratio_percent, minutes), false)
                    }
                    Err(error) => {
                        self.sharing_choice = (
                            f64::from(self.profile.settings().stop_sharing_ratio_percent) / 100.0,
                            self.profile.settings().stop_sharing_after_minutes,
                        );
                        (error.to_string(), true)
                    }
                },
            );
        }
        if let Some((download, upload)) = change_limits {
            self.settings_message = Some(match self.profile.set_speed_limits(download, upload) {
                Ok(()) => {
                    if let Ok(engine) = &self.engine {
                        engine.set_speed_limits(download, upload);
                    }
                    (
                        format!(
                            "Speed limits, from now on: download {}, upload {}.",
                            limit_words(download),
                            limit_words(upload)
                        ),
                        false,
                    )
                }
                Err(error) => {
                    self.limits_choice = (
                        self.profile.settings().download_limit_kib,
                        self.profile.settings().upload_limit_kib,
                    );
                    (error.to_string(), true)
                }
            });
        }
        if let Some(limit) = change_max_active {
            self.settings_message = Some(match self.profile.set_max_active_downloads(limit) {
                Ok(()) => {
                    if let Ok(engine) = &self.engine {
                        engine.set_max_active_downloads(limit as usize);
                    }
                    let now = match limit {
                        0 => "Every download runs at once, from now on.".to_string(),
                        1 => "One download runs at a time, from now on.".to_string(),
                        _ => format!("{limit} downloads run at the same time, from now on."),
                    };
                    (now, false)
                }
                Err(error) => {
                    self.max_active_choice = self.profile.settings().max_active_downloads;
                    (error.to_string(), true)
                }
            });
        }
        if let Some(allow) = change_http {
            self.settings_message = Some(match self.profile.set_allow_http_links(allow) {
                Ok(()) => {
                    if let Ok(engine) = &self.engine {
                        engine.set_allow_http_links(allow);
                    }
                    let now = if allow {
                        "Plain http:// links are opened too, from now on."
                    } else {
                        "Only secure https:// links are opened, from now on."
                    };
                    (now.to_string(), false)
                }
                Err(error) => (error.to_string(), true),
            });
        }
    }
}

impl BitDinghyApp {
    /// The Downloads page: the link box at the top, then one card per download.
    fn draw_downloads_page(&mut self, ui: &mut egui::Ui, engine: &Arc<Engine>, hovering: bool) {
        ui.horizontal(|ui| {
            let box_width = (ui.available_width() - 250.0).max(120.0);
            let link_box = ui.add(
                egui::TextEdit::singleline(&mut self.link)
                    .hint_text("Paste a magnet link, a .torrent file's path, or a web link to one")
                    .desired_width(box_width),
            );
            let enter_pressed =
                link_box.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if primary_button(ui, "Download").clicked() || enter_pressed {
                self.submit_link(engine);
            }
            if button(ui, "Open .torrent file...").clicked() {
                // Windows' own file dialog. The window waits while it is open.
                let chosen = rfd::FileDialog::new()
                    .set_title("Open .torrent files")
                    .add_filter(".torrent files", &["torrent"])
                    .pick_files()
                    .unwrap_or_default();
                self.add_files(engine, &chosen);
            }
        });
        let folder = engine.download_dir();
        egui::Sides::new().shrink_left().truncate().show(
            ui,
            |ui| {
                ui.small(format!(
                    "You can also drag .torrent files onto this window. Saving to: {}",
                    folder.display()
                ));
            },
            |ui| {
                if quiet_button(ui, "Open folder")
                    .on_hover_text("Opens the download folder")
                    .clicked()
                {
                    open_folder(folder);
                }
            },
        );
        if let Some(fetching) = &self.fetching {
            ui.horizontal(|ui| {
                spinner(ui);
                ui.colored_label(
                    info_color(ui),
                    format!("Fetching the .torrent file from {}...", fetching.host),
                );
            });
        }
        if let Some(problem) = &self.link_problem {
            ui.colored_label(error_color(ui), problem);
        }
        if hovering {
            ui.colored_label(info_color(ui), "Drop the .torrent files here to add them.");
        }
        if let Some((message, is_error)) = &self.control_message {
            let color = if *is_error {
                error_color(ui)
            } else {
                ui.visuals().text_color()
            };
            ui.colored_label(color, message);
        }
        ui.add_space(8.0);

        let downloads = engine.snapshot();
        if downloads.is_empty() {
            card(ui, |ui| {
                ui.label("Nothing downloading yet. Paste a magnet link above and press Download.");
            });
            return;
        }
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for download in &downloads {
                    let confirming = self.confirming_remove == Some(download.id);
                    let files = self
                        .showing_files
                        .contains(&download.id)
                        .then(|| engine.files(download.id).map_err(|error| error.to_string()));
                    let movable = downloads.len() > 1;
                    // Greyed out while one of its actions is still running.
                    let busy = self.running.iter().any(|running| running.id == download.id);
                    let pressed = card(ui, |ui| {
                        ui.add_enabled_ui(!busy, |ui| {
                            draw_download(ui, download, confirming, files, movable)
                        })
                        .inner
                    });
                    clicked = pressed.or(clicked.take());
                    ui.add_space(8.0);
                }
            });
        // Act on a button only after drawing, so the list isn't changed halfway through.
        if let Some(control) = clicked {
            self.act(engine, &downloads, control);
        }
    }
}

impl eframe::App for BitDinghyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        dinghy_core::crash::crash_if_testing();
        // Redraw a couple of times a second so progress keeps moving.
        ui.ctx().request_repaint_after(Duration::from_millis(500));
        let engine = self.engine.as_ref().ok().cloned();

        // Files dragged over the window: say what dropping them does (on the Downloads page);
        // add them when dropped, whichever page is showing.
        let (hovering, dropped) = ui.ctx().input(|input| {
            (
                !input.raw.hovered_files.is_empty(),
                input
                    .raw
                    .dropped_files
                    .iter()
                    .map(|file| file.path().to_path_buf())
                    .collect::<Vec<_>>(),
            )
        });
        if hovering {
            self.page = Page::Downloads;
        }
        let mut count = 0;
        if let Some(engine) = &engine {
            // Links from other programs (a browser, Explorer): ask first. A new one brings the
            // window to the front.
            let arrived = engine.take_offers();
            if !arrived.is_empty() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
                self.offers.extend(arrived);
            }
            if !dropped.is_empty() {
                self.page = Page::Downloads;
                self.add_files(engine, &dropped);
            }
            self.collect_fetch();
            self.collect_running();
            count = engine.snapshot().len();
        }

        let palette = brand::palette(ui.ctx().theme());
        egui::Panel::left("pages")
            .exact_size(196.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(egui::Margin::symmetric(10, 14)),
            )
            .show(ui, |ui| self.draw_sidebar(ui, engine.as_ref(), count));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ui, |ui| {
                self.draw_notices(ui, engine.as_ref());
                match (self.page, &engine) {
                    (Page::Settings, _) => self.draw_settings_page(ui),
                    (Page::Downloads, Some(engine)) => {
                        self.draw_downloads_page(ui, engine, hovering);
                    }
                    (Page::History, Some(engine)) => draw_history_page(ui, engine),
                    // The engine couldn't start: the reason is in the notices above.
                    (_, None) => {}
                }
            });
    }

    /// Called when the window closes: stop sharing and close connections tidily.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Ok(engine) = &self.engine {
            shut_down(&self.runtime, engine);
        }
    }
}

/// Gives a link to the BitDinghy window that is already open, if there is one. Returns whether
/// it was handled (given, or refused with a message shown).
fn hand_to_open_window(data_dir: &Path, link: &str) -> bool {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return false;
    };
    let address = remote::Address::for_data_dir(data_dir);
    let answer = runtime.block_on(async {
        let mut window = remote::Client::connect(&address).await.ok()??;
        Some(
            window
                .ask(remote::Command::Offer {
                    link: link.to_string(),
                })
                .await,
        )
    });
    match answer {
        None => false,
        Some(Ok(remote::Reply::Changed { .. })) => true,
        Some(other) => {
            let message = match other {
                Ok(remote::Reply::Problem { message }) => message,
                Err(error) => error.to_string(),
                Ok(_) => "BitDinghy's window gave an unexpected answer.".to_string(),
            };
            rfd::MessageDialog::new()
                .set_title(dinghy_core::APP_NAME)
                .set_description(message)
                .set_level(rfd::MessageLevel::Warning)
                .show();
            true
        }
    }
}

/// Stops the engine and waits up to three seconds for it.
///
/// The timeout has to be created INSIDE the `async` block. Written as
/// `block_on(tokio::time::timeout(..))`, the timeout is built before `block_on` has entered the
/// runtime, and tokio panics with "there is no reactor running". That crashed the window every time
/// it was closed (found by starting the real program and closing it).
fn shut_down(runtime: &tokio::runtime::Runtime, engine: &Engine) {
    runtime.block_on(async {
        let _ = tokio::time::timeout(Duration::from_secs(3), engine.shutdown()).await;
    });
}

/// The web site's name in a link, for messages ("archlinux.org").
fn url_host(link: &str) -> String {
    link.split("://")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .unwrap_or(link)
        .to_string()
}

/// A button the person pressed on one download.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Control {
    Pause(DownloadId),
    Resume(DownloadId),
    /// "Check files": check its files on disk again.
    Check(DownloadId),
    /// Up or down the list (the queue order).
    Move {
        id: DownloadId,
        to: Move,
    },
    /// "Use the files already there" (after a refusal because a file with the same name exists).
    UseExistingFiles(DownloadId),
    /// "Files" was pressed: show or hide its file list.
    ToggleFiles(DownloadId),
    /// A tick box was changed: download only these files (their numbers).
    ChooseFiles {
        id: DownloadId,
        chosen: BTreeSet<usize>,
    },
    /// "Remove" was pressed: show the question.
    AskRemove(DownloadId),
    CancelRemove,
    /// The question was answered.
    Remove {
        id: DownloadId,
        delete_files: bool,
    },
}

/// What to say after a download was removed: the message, and whether it is an error.
fn removed_message(
    name: &str,
    folder: &str,
    delete_files: bool,
    deleted: &dinghy_core::owned_files::Deleted,
) -> (String, bool) {
    if !delete_files {
        return (
            format!("Removed \"{name}\". Its files are still in {folder}."),
            false,
        );
    }
    let count = deleted.files;
    let mut message = format!(
        "Removed \"{name}\" and deleted {count} file{} it downloaded.",
        if count == 1 { "" } else { "s" }
    );
    if deleted.problems.is_empty() {
        return (message, false);
    }
    message.push_str(" These couldn't be deleted, so you can delete them yourself: ");
    message.push_str(&deleted.problems.join("; "));
    (message, true)
}

/// The button a download shows: Resume when paused, "Try again" when it failed, Pause otherwise.
fn control_for(download: &DownloadSnapshot) -> (&'static str, Control) {
    match download.status {
        DownloadStatus::Paused => ("Resume", Control::Resume(download.id)),
        DownloadStatus::Finished if download.sharing_stopped => {
            ("Share again", Control::Resume(download.id))
        }
        DownloadStatus::Failed => ("Try again", Control::Resume(download.id)),
        _ => ("Pause", Control::Pause(download.id)),
    }
}

/// The line under a paused download's progress bar.
fn paused_text(download: &DownloadSnapshot) -> String {
    if download.total_bytes == 0 {
        "Paused. BitDinghy will look for its file list when you resume it.".to_string()
    } else {
        format!(
            "Paused at {} of {}. Nothing is downloaded or shared until you resume it.",
            format_bytes(download.progress_bytes),
            format_bytes(download.total_bytes)
        )
    }
}

/// The "Files" button's label: says when only some files are chosen.
fn files_button_text(download: &DownloadSnapshot, showing: bool) -> String {
    if showing {
        "Hide files".to_string()
    } else if download.files_chosen < download.files_total {
        format!(
            "Files ({} of {})",
            download.files_chosen, download.files_total
        )
    } else {
        "Files".to_string()
    }
}

/// The files the person has ticked after clicking the tick box of file `clicked`.
fn chosen_after_click(files: &[FileChoice], clicked: usize) -> BTreeSet<usize> {
    files
        .iter()
        .filter(|file| file.chosen != (file.number == clicked))
        .map(|file| file.number)
        .collect()
}

/// A download's files with tick boxes. Changing a tick applies at once. The last ticked file
/// can't be unticked: to stop the whole download, pause or remove it.
fn draw_files(ui: &mut egui::Ui, id: DownloadId, files: &[FileChoice]) -> Option<Control> {
    let mut clicked = None;
    banner(ui, Tone::Busy, |ui| {
        let chosen_count = files.iter().filter(|file| file.chosen).count();
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} of {} files chosen. Unticked files are not downloaded.",
                chosen_count,
                files.len()
            ));
            if chosen_count < files.len() && ui.button("Choose all").clicked() {
                clicked = Some(Control::ChooseFiles {
                    id,
                    chosen: files.iter().map(|file| file.number).collect(),
                });
            }
        });
        ui.small(
            "What is left of unticked files in the folder is hidden (the engine needs it to \
             check the files next to them).",
        );
        let row_height = ui.spacing().interact_size.y;
        // Only the rows on screen are drawn, so a torrent with thousands of files stays quick.
        egui::ScrollArea::vertical()
            .id_salt(("files", id.0))
            .max_height(row_height * 10.0)
            .auto_shrink([false, true])
            .show_rows(ui, row_height, files.len(), |ui, rows| {
                for file in &files[rows] {
                    ui.horizontal(|ui| {
                        let mut ticked = file.chosen;
                        let only_one_left = file.chosen && chosen_count == 1;
                        let tick = ui
                            .add_enabled(
                                !only_one_left,
                                egui::Checkbox::new(&mut ticked, file.path.display().to_string()),
                            )
                            .on_disabled_hover_text(
                                "At least one file stays chosen. To stop the whole download, \
                                 pause or remove it.",
                            );
                        if tick.changed() {
                            clicked = Some(Control::ChooseFiles {
                                id,
                                chosen: chosen_after_click(files, file.number),
                            });
                        }
                        ui.weak(file_details(file));
                    });
                }
            });
    });
    clicked
}

/// A file's size, and how far it got when known: "1.5 GiB, 42%".
fn file_details(file: &FileChoice) -> String {
    match file.done_bytes {
        Some(done) => format!(
            "{}, {:.0}%",
            format_bytes(file.size),
            fraction(done, file.size) * 100.0
        ),
        None => format_bytes(file.size),
    }
}

/// The line under a finished download: what was saved, and where.
fn finished_text(download: &DownloadSnapshot, folder: &str) -> String {
    let size = format_bytes(download.total_bytes);
    if download.files_chosen < download.files_total {
        format!(
            "Finished: {} of {} files ({size}) saved in {folder}",
            download.files_chosen, download.files_total
        )
    } else {
        format!("Finished: {size} saved in {folder}")
    }
}

/// The numbers under a running download's progress bar.
fn progress_details(download: &DownloadSnapshot) -> String {
    format!(
        "{} of {}  ·  {}  ·  {} left  ·  {} people connected ({} known)",
        format_bytes(download.progress_bytes),
        format_bytes(download.total_bytes),
        format_speed(download.download_bytes_per_second),
        format_eta(download.eta),
        download.peers_connected,
        download.peers_known,
    )
}

/// Draws one download's card: its name and status badge, a progress bar (or what is happening
/// instead), then the details on the left and its buttons on the right. `confirming` shows the
/// "Remove?" question under it; `files` is its file list when that is open. Returns the button
/// press, if any.
fn draw_download(
    ui: &mut egui::Ui,
    download: &DownloadSnapshot,
    confirming: bool,
    files: Option<Result<Vec<FileChoice>, String>>,
    movable: bool,
) -> Option<Control> {
    let mut clicked = None;
    let palette = brand::palette(ui.ctx().theme());

    // The name, with the "move in the list" buttons before it and the status badge after it.
    egui::Sides::new().shrink_left().truncate().show(
        ui,
        |ui| {
            // Up and down the list: the queue starts downloads in this order.
            if movable {
                if ui
                    .small_button(UP)
                    .on_hover_text("Move up the list (downloads start in list order)")
                    .clicked()
                {
                    clicked = Some(Control::Move {
                        id: download.id,
                        to: Move::Up,
                    });
                }
                if ui
                    .small_button(DOWN)
                    .on_hover_text("Move down the list")
                    .clicked()
                {
                    clicked = Some(Control::Move {
                        id: download.id,
                        to: Move::Down,
                    });
                }
            }
            // Not `.strong()`: egui paints that in the pressed-button color, which the brand
            // makes dark navy (for text on the green button), so the name vanished.
            ui.label(
                egui::RichText::new(&download.name)
                    .font(brand::semibold(14.5))
                    .color(palette.text),
            )
            .on_hover_text(&download.name);
        },
        |ui| {
            let (word, tone) = status_badge(download);
            widgets::badge(ui, word, tone);
        },
    );

    // The progress bar, or what is happening instead.
    match download.status {
        DownloadStatus::Downloading | DownloadStatus::Paused | DownloadStatus::Finished => {
            let done = fraction(download.progress_bytes, download.total_bytes);
            ui.horizontal(|ui| {
                // The percentage sits beside the bar, where it is always readable.
                let bar_width = (ui.available_width() - 56.0).max(80.0);
                let fill = if download.status == DownloadStatus::Paused {
                    palette.accent.gamma_multiply(0.45)
                } else {
                    palette.accent
                };
                ui.add(
                    egui::ProgressBar::new(done)
                        .fill(fill)
                        .desired_height(8.0)
                        .corner_radius(4)
                        .desired_width(bar_width),
                );
                ui.label(format!("{:.1}%", done * 100.0));
            });
        }
        DownloadStatus::GettingInfo => {
            ui.horizontal(|ui| {
                spinner(ui);
                ui.colored_label(
                    info_color(ui),
                    waiting_text(download.waiting_for.unwrap_or_default()),
                );
            });
        }
        DownloadStatus::Checking => {
            ui.horizontal(|ui| {
                spinner(ui);
                ui.colored_label(info_color(ui), checking_text(download));
            });
        }
        DownloadStatus::Queued | DownloadStatus::Failed => {}
    }

    // The details on the left, the buttons on the right.
    let details: Vec<(String, egui::Color32)> = match download.status {
        DownloadStatus::Queued => vec![(
            "Queued: it starts when another download finishes (Settings: downloads at the same \
             time)."
                .to_string(),
            palette.info,
        )],
        DownloadStatus::Failed => vec![(
            download
                .error
                .clone()
                .unwrap_or_else(|| "The download failed.".to_string()),
            palette.danger,
        )],
        DownloadStatus::Paused => vec![(paused_text(download), palette.text_muted)],
        DownloadStatus::Finished => {
            let folder = download
                .save_folder
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            vec![
                (finished_text(download, &folder), palette.success),
                (sharing_text(download), palette.text_muted),
            ]
        }
        DownloadStatus::Downloading => vec![(progress_details(download), palette.text_muted)],
        DownloadStatus::GettingInfo | DownloadStatus::Checking => Vec::new(),
    };
    egui::Sides::new()
        .shrink_left()
        .wrap_mode(egui::TextWrapMode::Wrap)
        .show(
            ui,
            |ui| {
                ui.vertical(|ui| {
                    for (text, color) in details {
                        ui.colored_label(color, text);
                    }
                });
            },
            |ui| {
                // Right to left: the first button added sits at the far right.
                if quiet_button(ui, "Remove").clicked() {
                    clicked = Some(Control::AskRemove(download.id));
                }
                let (label, control) = control_for(download);
                if button(ui, label).clicked() {
                    clicked = Some(control);
                }
                if can_check(download)
                    && quiet_button(ui, "Check files")
                        .on_hover_text(
                            "Reads its files on disk again and compares them with the torrent. \
                             Anything missing or damaged is downloaded again.",
                        )
                        .clicked()
                {
                    clicked = Some(Control::Check(download.id));
                }
                // Only a torrent with several files has anything to choose.
                if download.files_total > 1
                    && quiet_button(ui, &files_button_text(download, files.is_some())).clicked()
                {
                    clicked = Some(Control::ToggleFiles(download.id));
                }
                if download.status == DownloadStatus::Failed
                    && download.can_use_existing_files
                    && button(ui, "Use the files already there")
                        .on_hover_text(
                            "BitDinghy checks the files that are already there and completes \
                             them; parts that don't match the torrent are replaced. From then on \
                             they count as this download's files (\"Delete the files too\" would \
                             delete them).",
                        )
                        .clicked()
                {
                    clicked = Some(Control::UseExistingFiles(download.id));
                }
            },
        );

    if confirming {
        banner(ui, Tone::Waiting, |ui| {
            ui.colored_label(
                warning_color(ui),
                format!("Remove \"{}\" from the list?", download.name),
            );
            ui.small(
                "\"Delete the files too\" deletes only the files BitDinghy downloaded for it. \
                 Nothing else in the folder is touched. \"Keep the files\" also shows any \
                 hidden leftovers of unticked files again.",
            );
            ui.horizontal(|ui| {
                if button(ui, "Keep the files").clicked() {
                    clicked = Some(Control::Remove {
                        id: download.id,
                        delete_files: false,
                    });
                }
                if button(ui, "Delete the files too").clicked() {
                    clicked = Some(Control::Remove {
                        id: download.id,
                        delete_files: true,
                    });
                }
                if quiet_button(ui, "Cancel").clicked() {
                    clicked = Some(Control::CancelRemove);
                }
            });
        });
    }

    match &files {
        Some(Ok(files)) => clicked = draw_files(ui, download.id, files).or(clicked),
        Some(Err(problem)) => {
            ui.colored_label(error_color(ui), problem);
        }
        None => {}
    }

    // Something to know that isn't a failure (for example: the drive may be too full).
    if let Some(warning) = &download.warning {
        ui.colored_label(warning_color(ui), warning);
    }
    // What the last "Check files" found: orange when something is missing or damaged.
    if let Some(note) = &download.note
        && download.status != DownloadStatus::Checking
    {
        let color = if download.note_is_warning {
            warning_color(ui)
        } else {
            info_color(ui)
        };
        ui.colored_label(color, note);
    }
    clicked
}

/// "Check files" makes sense once the file list is known, and not while it is starting or
/// already being checked.
fn can_check(download: &DownloadSnapshot) -> bool {
    download.files_total > 0
        && !matches!(
            download.status,
            DownloadStatus::Checking | DownloadStatus::GettingInfo
        )
}

/// The line while files on disk are being checked, with how far it got.
fn checking_text(download: &DownloadSnapshot) -> String {
    if download.total_bytes == 0 {
        "Checking files already on disk...".to_string()
    } else {
        format!(
            "Checking files already on disk... {:.0}%",
            fraction(download.progress_bytes, download.total_bytes) * 100.0
        )
    }
}

fn main() -> eframe::Result {
    // If the window crashes, it saves a report first (it stays on this computer); the next start
    // says so.
    let data_dir = dinghy_core::DataDir::locate()
        .ok()
        .map(|dir| dir.path().to_path_buf());
    dinghy_core::crash::install("BitDinghy", data_dir.clone());

    // Started by Windows with a magnet link or a .torrent file (M16): if a BitDinghy window is
    // already open, hand the link to it (it asks the person) and stop here.
    let opened_with = std::env::args().nth(1).filter(|arg| !arg.trim().is_empty());
    if let (Some(link), Some(data_dir)) = (&opened_with, &data_dir)
        && hand_to_open_window(data_dir, link)
    {
        return Ok(());
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([960.0, 660.0])
        .with_min_inner_size([640.0, 420.0]);
    if let Some(icon) = brand::window_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        dinghy_core::APP_NAME,
        options,
        Box::new(move |creation_context| {
            Ok(Box::new(BitDinghyApp::new(creation_context, opened_with)))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(status: DownloadStatus, progress: u64, total: u64) -> DownloadSnapshot {
        DownloadSnapshot {
            id: DownloadId(7),
            info_hash: "c9e15763f722f23e98a29decdfae341b98d53056".to_string(),
            name: "Example".to_string(),
            status,
            progress_bytes: progress,
            total_bytes: total,
            download_bytes_per_second: 0,
            upload_bytes_per_second: 0,
            eta: None,
            peers_connected: 0,
            peers_known: 0,
            uploaded_bytes: 0,
            waiting_for: None,
            save_folder: None,
            error: None,
            files_total: 0,
            files_chosen: 0,
            note: None,
            note_is_warning: false,
            can_use_existing_files: false,
            shared_seconds: 0,
            sharing_stopped: false,
            warning: None,
        }
    }

    #[test]
    fn the_history_says_today_and_the_last_30_days() {
        let day = |day, down, up| DayTotal {
            day,
            downloaded_bytes: down,
            uploaded_bytes: up,
        };
        let totals = [day(99, 1024, 0), day(100, 2048, 512)];
        assert_eq!(
            day_sums(&totals, 100),
            DaySums {
                today_down: 2048,
                today_up: 512,
                month_down: 3072,
                month_up: 512,
            }
        );
        let entry = dinghy_core::HistoryEntry {
            info_hash: "ab".repeat(20),
            name: "Some Linux".into(),
            size_bytes: 1024 * 1024,
            added_at: 0,
            finished_at: None,
            removed_at: 20_737 * 86_400 + 43_200,
            uploaded_bytes: 0,
            folder: "C:\\D".into(),
        };
        let line = history_line(&entry);
        assert!(
            line.starts_with("Some Linux (1.0 MiB), not finished, removed 2026-10-1"),
            "{line}"
        );
    }

    #[test]
    fn the_crash_notice_says_where_and_that_it_stays_here() {
        let text = crash_notice_text(Path::new(r"C:\data\crash-reports"));
        assert!(text.contains(r"saved in C:\data\crash-reports"), "{text}");
        assert!(text.contains("stays on this computer"), "{text}");
    }

    #[test]
    fn an_offered_link_is_described_before_asking() {
        assert_eq!(
            offer_text(
                "magnet:?xt=urn:btih:5a8b2f0c9e1d4b7a6c3e8f0a1b2c3d4e5f6a7b8c&dn=Some%20Linux"
            ),
            "A magnet link: \"Some Linux\""
        );
        assert_eq!(
            offer_text("magnet:?xt=urn:btih:5a8b2f0c9e1d4b7a6c3e8f0a1b2c3d4e5f6a7b8c"),
            "A magnet link (fingerprint 5a8b2f0c)"
        );
        assert_eq!(
            offer_text("https://example.org/x.torrent"),
            "A .torrent file from example.org"
        );
        assert!(
            offer_text("magnet:?broken").contains("magnet"),
            "{}",
            offer_text("magnet:?broken")
        );
    }

    #[test]
    fn the_default_app_line_says_which_copy_is_registered() {
        use dinghy_core::default_app::Status;
        let here = Path::new(r"C:\Apps\BitDinghy.exe");
        assert!(default_app_text(&Status::NotRegistered, Some(here)).contains("isn't set up"));
        let mine = Status::Registered {
            exe: here.to_path_buf(),
        };
        assert!(default_app_text(&mine, Some(here)).contains("choose it in Windows' Default apps"));
        let other = Status::Registered {
            exe: r"D:\Old\BitDinghy.exe".into(),
        };
        assert!(default_app_text(&other, Some(here)).contains(
            r"Another copy of BitDinghy is registered to open them: D:\Old\BitDinghy.exe"
        ));
    }

    #[test]
    fn the_recommendation_is_said_in_one_line() {
        let recommended = dinghy_core::recommend::Recommended {
            download_limit_kib: 4608,
            upload_limit_kib: 0,
            max_active_downloads: 3,
        };
        assert_eq!(
            recommended_text(&recommended),
            "Download limit 4.5 MiB/s, upload limit none, 3 downloads at the same time."
        );
    }

    #[test]
    fn the_listening_line_says_what_is_happening_now() {
        assert_eq!(
            listening_text(Some(51413)),
            "Right now: accepting connections on port 51413."
        );
        assert!(listening_text(None).contains("not accepting"));
    }

    #[test]
    fn a_download_stopped_by_a_sharing_limit_offers_share_again() {
        let mut download = snapshot(DownloadStatus::Finished, 100, 100);
        assert_eq!(control_for(&download).0, "Pause");
        download.sharing_stopped = true;
        assert_eq!(
            control_for(&download),
            ("Share again", Control::Resume(download.id))
        );
        assert!(sharing_text(&download).starts_with("Stopped sharing"));
    }

    #[test]
    fn the_sharing_limits_are_said_in_words() {
        assert!(sharing_limits_text(0, 0).contains("for as long as BitDinghy runs"));
        assert_eq!(
            sharing_limits_text(150, 0),
            "A finished download stops sharing once it has shared 1.50 times its size, from now on."
        );
        assert!(sharing_limits_text(100, 60).contains("or after 60 minutes of sharing"));
    }

    #[test]
    fn the_sidebar_adds_up_the_speeds_and_says_the_limits() {
        let mut one = snapshot(DownloadStatus::Downloading, 0, 0);
        one.download_bytes_per_second = 1024 * 1024;
        one.upload_bytes_per_second = 2048;
        let mut two = one.clone();
        two.download_bytes_per_second = 512 * 1024;
        assert_eq!(total_speeds(&[one, two]), (1536 * 1024, 4096));
        assert_eq!(limits_text((0, 0)), "No speed limits");
        assert_eq!(limits_text((2048, 0)), "Limits: 2.0 MiB/s down, none up");
        assert_eq!(limit_words(0), "none");
    }

    #[test]
    fn the_numbers_under_a_download_read_well() {
        let mut download = snapshot(DownloadStatus::Downloading, 1024, 4096);
        download.peers_connected = 3;
        download.peers_known = 40;
        let text = progress_details(&download);
        assert!(text.starts_with("1.0 KiB of 4.0 KiB  ·  0 B/s"), "{text}");
        assert!(text.ends_with("3 people connected (40 known)"), "{text}");
    }

    /// The brand fonts are installed as in the real window, so a font change can't turn the
    /// symbols the window uses into empty boxes.
    #[test]
    fn every_symbol_the_window_uses_can_be_drawn() {
        let ctx = egui::Context::default();
        brand::apply(&ctx, Look::Dark);
        let mut first = ctx.run_ui(Default::default(), |_| {});
        first.textures_delta.clear();
        let symbols = [
            UP,
            DOWN,
            ARROW_UP,
            ARROW_DOWN,
            DOWNLOADS_SYMBOL,
            HISTORY_SYMBOL,
            SETTINGS_SYMBOL,
            "·",
        ];
        for symbol in symbols {
            for font in [egui::FontId::proportional(14.0), brand::semibold(14.0)] {
                assert!(
                    ctx.fonts_mut(|fonts| fonts.has_glyphs(&font, symbol)),
                    "{symbol} in {font:?}"
                );
            }
        }
    }

    #[test]
    fn each_status_has_its_own_badge() {
        let words = |status| status_badge(&snapshot(status, 0, 0));
        assert_eq!(
            words(DownloadStatus::Downloading),
            ("Downloading", Tone::Busy)
        );
        assert_eq!(words(DownloadStatus::Queued), ("Queued", Tone::Waiting));
        assert_eq!(words(DownloadStatus::Paused), ("Paused", Tone::Quiet));
        assert_eq!(words(DownloadStatus::Failed), ("Problem", Tone::Problem));
        assert_eq!(
            words(DownloadStatus::Finished),
            ("Finished, sharing", Tone::Done)
        );
        let mut stopped = snapshot(DownloadStatus::Finished, 1, 1);
        stopped.sharing_stopped = true;
        assert_eq!(status_badge(&stopped), ("Finished", Tone::Done));
    }

    #[test]
    fn the_settings_say_which_program_opens_links() {
        let here = Path::new(r"C:\Apps\BitDinghy\BitDinghy.exe");
        let this_copy = Opener::BitDinghy {
            exe: Some(r"c:\apps\bitdinghy\BITDINGHY.EXE".into()),
        };
        assert_eq!(
            opener_text(Kind::MagnetLinks, &this_copy, Some(here)),
            (
                "Magnet links open in this BitDinghy.".to_string(),
                Tone::Done
            )
        );
        let other_copy = Opener::BitDinghy {
            exe: Some(r"C:\cargo-target\debug\BitDinghy.exe".into()),
        };
        let (text, tone) = opener_text(Kind::TorrentFiles, &other_copy, Some(here));
        assert_eq!(
            text,
            r".torrent files open in another copy of BitDinghy: C:\cargo-target\debug\BitDinghy.exe"
        );
        assert_eq!(tone, Tone::Waiting);
        let other = Opener::Other {
            name: "qbittorrent.exe".into(),
        };
        assert_eq!(
            opener_text(Kind::MagnetLinks, &other, Some(here)).0,
            "Magnet links open in qbittorrent.exe."
        );
        assert!(
            opener_text(Kind::MagnetLinks, &Opener::NotChosen, Some(here))
                .0
                .contains("no program chosen")
        );
    }

    #[test]
    fn the_default_app_question_is_asked_once_and_only_when_useful() {
        let here = Path::new(r"C:\Apps\BitDinghy\BitDinghy.exe");
        let this_copy = Opener::BitDinghy {
            exe: Some(here.to_path_buf()),
        };
        let other = Opener::Other {
            name: "qbittorrent.exe".into(),
        };
        // Never asked, and something else opens .torrent files: ask (on Windows).
        assert_eq!(
            should_ask_about_default_app(false, &this_copy, &other, Some(here)),
            cfg!(windows)
        );
        // Already answered: never again.
        assert!(!should_ask_about_default_app(
            true,
            &other,
            &other,
            Some(here)
        ));
        // This copy already opens both: nothing to ask.
        assert!(!should_ask_about_default_app(
            false,
            &this_copy,
            &this_copy,
            Some(here)
        ));
    }

    #[test]
    fn the_when_finished_choices_are_all_offered_and_explained() {
        let offered: Vec<WhenFinished> = WHEN_FINISHED_CHOICES
            .iter()
            .map(|(choice, _)| *choice)
            .collect();
        assert_eq!(
            offered,
            [
                WhenFinished::KeepSharing,
                WhenFinished::StopRightAway,
                WhenFinished::StopAfterCheck
            ]
        );
        assert!(WHEN_FINISHED_CHOICES[0].1.contains("(the standard)"));
        assert!(when_finished_text(WhenFinished::StopRightAway).contains("Share again"));
        assert!(when_finished_text(WhenFinished::StopAfterCheck).contains("Check files"));
    }

    #[test]
    fn check_files_is_offered_once_the_file_list_is_known() {
        let mut download = snapshot(DownloadStatus::Downloading, 0, 100);
        assert!(!can_check(&download), "file list not known yet");
        download.files_total = 1;
        assert!(can_check(&download));
        for busy in [DownloadStatus::Checking, DownloadStatus::GettingInfo] {
            download.status = busy;
            assert!(!can_check(&download), "{busy:?}");
        }
        download.status = DownloadStatus::Paused;
        assert!(can_check(&download));
    }

    #[test]
    fn the_checking_line_shows_how_far_it_got() {
        let download = snapshot(DownloadStatus::Checking, 45, 100);
        assert_eq!(
            checking_text(&download),
            "Checking files already on disk... 45%"
        );
        let download = snapshot(DownloadStatus::Checking, 0, 0);
        assert_eq!(
            checking_text(&download),
            "Checking files already on disk..."
        );
    }

    fn file(number: usize, chosen: bool) -> FileChoice {
        FileChoice {
            number,
            path: format!("file{number}.bin").into(),
            size: 1000,
            done_bytes: None,
            chosen,
        }
    }

    #[test]
    fn clicking_a_tick_box_flips_only_that_file() {
        let files = [file(1, true), file(2, false), file(3, true)];
        assert_eq!(chosen_after_click(&files, 2), BTreeSet::from([1, 2, 3]));
        assert_eq!(chosen_after_click(&files, 1), BTreeSet::from([3]));
    }

    #[test]
    fn the_files_button_says_when_only_some_are_chosen() {
        let mut download = snapshot(DownloadStatus::Downloading, 0, 0);
        download.files_total = 5;
        download.files_chosen = 5;
        assert_eq!(files_button_text(&download, false), "Files");
        download.files_chosen = 2;
        assert_eq!(files_button_text(&download, false), "Files (2 of 5)");
        assert_eq!(files_button_text(&download, true), "Hide files");
    }

    #[test]
    fn a_finished_download_says_how_many_files_were_chosen() {
        let mut download = snapshot(DownloadStatus::Finished, 2048, 2048);
        download.files_total = 3;
        download.files_chosen = 3;
        assert_eq!(
            finished_text(&download, r"C:\D"),
            r"Finished: 2.0 KiB saved in C:\D"
        );
        download.files_chosen = 1;
        assert!(
            finished_text(&download, r"C:\D").contains("Finished: 1 of 3 files (2.0 KiB)"),
            "{}",
            finished_text(&download, r"C:\D")
        );
    }

    #[test]
    fn each_download_shows_the_button_that_fits_its_state() {
        let id = DownloadId(7);
        for status in [
            DownloadStatus::GettingInfo,
            DownloadStatus::Checking,
            DownloadStatus::Downloading,
            DownloadStatus::Finished,
        ] {
            assert_eq!(
                control_for(&snapshot(status, 0, 0)),
                ("Pause", Control::Pause(id))
            );
        }
        assert_eq!(
            control_for(&snapshot(DownloadStatus::Paused, 0, 0)),
            ("Resume", Control::Resume(id))
        );
        assert_eq!(
            control_for(&snapshot(DownloadStatus::Failed, 0, 0)),
            ("Try again", Control::Resume(id))
        );
    }

    #[test]
    fn the_removed_message_says_what_happened_to_the_files() {
        use dinghy_core::owned_files::Deleted;
        let (kept, error) = removed_message("Ubuntu", r"C:\Downloads", false, &Deleted::default());
        assert!(!error);
        assert!(kept.contains(r"still in C:\Downloads"), "{kept}");

        let all_gone = Deleted {
            files: 3,
            problems: vec![],
        };
        let (deleted, error) = removed_message("Ubuntu", r"C:\Downloads", true, &all_gone);
        assert!(!error);
        assert!(deleted.contains("deleted 3 files"), "{deleted}");

        let stuck = Deleted {
            files: 1,
            problems: vec![r"C:\Downloads\x.iso: in use".to_string()],
        };
        let (partly, error) = removed_message("Ubuntu", r"C:\Downloads", true, &stuck);
        assert!(error);
        assert!(partly.contains("deleted 1 file it downloaded"), "{partly}");
        assert!(partly.contains(r"x.iso: in use"), "{partly}");
    }

    #[test]
    fn a_paused_download_says_how_far_it_got() {
        let text = paused_text(&snapshot(DownloadStatus::Paused, 512 * 1024, 2048 * 1024));
        assert!(text.contains("Paused at 512.0 KiB of 2.0 MiB"), "{text}");
        let text = paused_text(&snapshot(DownloadStatus::Paused, 0, 0));
        assert!(text.contains("file list"), "{text}");
    }

    /// This test reproduces the real situation: the window calls `shut_down` from its ordinary
    /// thread, outside any async code. It used to panic.
    #[test]
    fn closing_the_window_shuts_the_engine_down_without_crashing() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let work = tempfile::tempdir().unwrap();
        let mut config = dinghy_core::EngineConfig::new(work.path().join("downloads"));
        config.use_dht = false;
        config.accept_incoming = false;
        config.local_discovery = false;
        let engine = runtime.block_on(Engine::start(config)).unwrap();

        shut_down(&runtime, &engine);
    }
}

#[cfg(test)]
mod web_link_tests {
    use super::*;

    #[test]
    fn the_fetching_line_names_the_web_site() {
        assert_eq!(
            url_host("https://archlinux.org/releng/releases/2026.10.01/torrent/"),
            "archlinux.org"
        );
        assert_eq!(url_host("https://example.org:8443?x=1"), "example.org:8443");
    }
}
