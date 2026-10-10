//! The `BitDinghy` window. A thin front-end over `dinghy-core`.

// In release builds on Windows, don't pop up a black console window behind the app.
// (Debug builds keep the console so log messages are visible while developing.)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod brand;

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use dinghy_core::display::{format_bytes, format_eta, format_speed, fraction, waiting_text};
use dinghy_core::input::{self, Input};
use dinghy_core::{
    DownloadId, DownloadSnapshot, DownloadStatus, Engine, FileChoice, Profile, remote,
};
use eframe::egui;

/// The brand's error color for the current theme (light or dark), so it stays readable on both
/// backgrounds.
fn error_color(ui: &egui::Ui) -> egui::Color32 {
    brand::palette(ui.ctx().theme()).danger
}

/// The brand's warning color for the current theme.
fn warning_color(ui: &egui::Ui) -> egui::Color32 {
    brand::palette(ui.ctx().theme()).warning
}

/// Green: something finished successfully.
fn success_color(ui: &egui::Ui) -> egui::Color32 {
    brand::palette(ui.ctx().theme()).success
}

/// Teal: something is in progress.
fn info_color(ui: &egui::Ui) -> egui::Color32 {
    brand::palette(ui.ctx().theme()).info
}

/// The "busy" spinner, in the teal of the line beside it. Not `ui.spinner()`: egui paints that in
/// the pressed-button color, which the brand makes dark navy, so it vanished on the background.
fn spinner(ui: &mut egui::Ui) {
    ui.add(egui::Spinner::new().color(info_color(ui)));
}

/// How tall the logo is drawn at the top of the window, in points.
const LOGO_HEIGHT: f32 = 48.0;

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
    /// The logo pictures. `None` if they couldn't be read, in which case plain text is shown.
    logos: Option<brand::Logos>,
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
    /// Why the command line can't talk to this window, if it can't.
    channel_problem: Option<String>,
    /// A web link being fetched in the background, if any.
    fetching: Option<Fetching>,
}

/// A web link to a `.torrent` file being fetched in the background, so the window never freezes.
struct Fetching {
    /// The web site's name, for the "Fetching..." line.
    host: String,
    task: tokio::task::JoinHandle<dinghy_core::Result<DownloadId>>,
}

impl BitDinghyApp {
    fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        brand::apply(&creation_context.egui_ctx);
        let logos = brand::Logos::load(&creation_context.egui_ctx);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("couldn't start BitDinghy's background threads");
        let profile = Profile::load();
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
        let folder_text = profile
            .settings()
            .download_dir_or_default()
            .display()
            .to_string();
        Self {
            runtime,
            profile,
            engine,
            logos,
            link: String::new(),
            link_problem: None,
            folder_text,
            settings_message: None,
            control_message: None,
            confirming_remove: None,
            showing_files: HashSet::new(),
            channel_problem,
            fetching: None,
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

    /// Carries out a button press on one download.
    fn act(&mut self, engine: &Engine, downloads: &[DownloadSnapshot], control: Control) {
        let name_of = |id| {
            downloads
                .iter()
                .find(|download| download.id == id)
                .map(|download| download.name.clone())
                .unwrap_or_default()
        };
        let outcome = match control {
            Control::Pause(id) => self.runtime.block_on(engine.pause(id)).map(|()| None),
            Control::Resume(id) => self.runtime.block_on(engine.resume(id)).map(|()| None),
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
            Control::ChooseFiles { id, chosen } => self
                .runtime
                .block_on(engine.choose_files(id, &chosen))
                .map(|()| None),
            Control::Remove { id, delete_files } => {
                self.confirming_remove = None;
                self.showing_files.remove(&id);
                let name = name_of(id);
                let folder = engine
                    .download_dir_of(id)
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_default();
                self.runtime
                    .block_on(engine.remove(id, delete_files))
                    .map(|deleted| Some(removed_message(&name, &folder, delete_files, &deleted)))
            }
        };
        self.control_message = match outcome {
            Ok(None) => None,
            Ok(Some(message)) => Some(message),
            Err(error) => Some((error.to_string(), true)),
        };
    }

    /// The logo (or, if it couldn't be loaded, the name) with the version beside it.
    fn draw_header(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.logos {
                Some(logos) => {
                    let texture = logos.for_theme(ui.ctx().theme());
                    let size = egui::vec2(LOGO_HEIGHT * texture.aspect_ratio(), LOGO_HEIGHT);
                    ui.add(
                        egui::Image::from_texture(egui::load::SizedTexture::from_handle(texture))
                            .fit_to_exact_size(size),
                    );
                }
                None => {
                    ui.heading(dinghy_core::APP_NAME);
                }
            }
            ui.weak(format!("version {}", dinghy_core::VERSION));
        });
    }

    /// The settings section: where BitDinghy keeps its files, and the download folder.
    fn draw_settings(&mut self, ui: &mut egui::Ui) {
        let mut save = false;
        let mut use_standard = false;
        let mut change_http = None;
        // Open it by default when the engine couldn't start, because the fix is probably here.
        egui::CollapsingHeader::new("Settings")
            .default_open(self.engine.is_err())
            .show(ui, |ui| {
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
                            "There is nowhere to keep BitDinghy's files, so settings can't be \
                             remembered.",
                        );
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Download folder:");
                    let width = (ui.available_width() - 230.0).max(120.0);
                    ui.add(egui::TextEdit::singleline(&mut self.folder_text).desired_width(width));
                    save |= ui.button("Save").clicked();
                    use_standard |= ui.button("Use the standard folder").clicked();
                });
                ui.small("A new download folder is used the next time you start BitDinghy.");
                let mut allow_http = self.profile.settings().allow_http_links;
                if ui
                    .checkbox(
                        &mut allow_http,
                        "Also open plain http:// links to .torrent files (less safe: they can be \
                         changed on the way). Secure https:// links always work.",
                    )
                    .changed()
                {
                    change_http = Some(allow_http);
                }
                if let Some((message, is_error)) = &self.settings_message {
                    let color = if *is_error {
                        error_color(ui)
                    } else {
                        ui.visuals().text_color()
                    };
                    ui.colored_label(color, message);
                }
            });
        if save || use_standard {
            self.save_download_folder(use_standard);
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

impl eframe::App for BitDinghyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw_header(ui);
        for notice in self.profile.notices() {
            ui.colored_label(warning_color(ui), notice);
        }
        self.draw_settings(ui);
        ui.separator();

        // If the engine couldn't start there is nothing else to show.
        let engine = match &self.engine {
            Ok(engine) => engine.clone(),
            Err(message) => {
                ui.add_space(8.0);
                ui.colored_label(error_color(ui), message);
                return;
            }
        };

        // Problems with the saved download list, for example.
        for notice in engine.notices() {
            ui.colored_label(warning_color(ui), notice);
        }
        if let Some(problem) = &self.channel_problem {
            ui.colored_label(warning_color(ui), problem);
        }

        // Redraw a couple of times a second so progress keeps moving.
        ui.ctx().request_repaint_after(Duration::from_millis(500));

        ui.label(format!("Saving to: {}", engine.download_dir().display()));
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let box_width = (ui.available_width() - 250.0).max(120.0);
            let link_box = ui.add(
                egui::TextEdit::singleline(&mut self.link)
                    .hint_text("Paste a magnet link, a .torrent file's path, or a web link to one")
                    .desired_width(box_width),
            );
            let enter_pressed =
                link_box.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if ui.button("Download").clicked() || enter_pressed {
                self.submit_link(&engine);
            }
            if ui.button("Open .torrent file...").clicked() {
                // Windows' own file dialog. The window waits while it is open.
                let chosen = rfd::FileDialog::new()
                    .set_title("Open .torrent files")
                    .add_filter(".torrent files", &["torrent"])
                    .pick_files()
                    .unwrap_or_default();
                self.add_files(&engine, &chosen);
            }
        });
        ui.small("You can also drag .torrent files onto this window.");
        self.collect_fetch();
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

        // Files dragged over the window: say what dropping them does; add them when dropped.
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
            ui.colored_label(info_color(ui), "Drop the .torrent files here to add them.");
        }
        if !dropped.is_empty() {
            self.add_files(&engine, &dropped);
        }

        ui.add_space(4.0);
        ui.separator();

        if let Some((message, is_error)) = &self.control_message {
            let color = if *is_error {
                error_color(ui)
            } else {
                ui.visuals().text_color()
            };
            ui.colored_label(color, message);
        }
        let downloads = engine.snapshot();
        if downloads.is_empty() {
            ui.add_space(8.0);
            ui.label("Nothing downloading yet. Paste a magnet link above and press Download.");
            return;
        }
        let mut clicked = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for download in &downloads {
                let confirming = self.confirming_remove == Some(download.id);
                let files = self
                    .showing_files
                    .contains(&download.id)
                    .then(|| engine.files(download.id).map_err(|error| error.to_string()));
                clicked = draw_download(ui, download, confirming, files).or(clicked.take());
                ui.separator();
            }
        });
        // Act on a button only after drawing, so the list isn't changed halfway through.
        if let Some(control) = clicked {
            self.act(&engine, &downloads, control);
        }
    }

    /// Called when the window closes: stop sharing and close connections tidily.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Ok(engine) = &self.engine {
            shut_down(&self.runtime, engine);
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
    ui.group(|ui| {
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

/// Draws one download: its name and buttons, then a progress bar and numbers (or a message).
/// `confirming` shows the "Remove?" question under it; `files` is its file list when that is
/// open. Returns the button press, if any.
fn draw_download(
    ui: &mut egui::Ui,
    download: &DownloadSnapshot,
    confirming: bool,
    files: Option<Result<Vec<FileChoice>, String>>,
) -> Option<Control> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        // Not `.strong()`: egui paints that in the pressed-button color, which the brand makes
        // dark navy (for text on the green button), so the name vanished on the navy background.
        let name_color = brand::palette(ui.ctx().theme()).text;
        ui.label(
            egui::RichText::new(&download.name)
                .size(15.0)
                .color(name_color),
        );
        // Right to left: the first button added sits at the far right.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Remove").clicked() {
                clicked = Some(Control::AskRemove(download.id));
            }
            let (label, control) = control_for(download);
            if ui.button(label).clicked() {
                clicked = Some(control);
            }
            // Only a torrent with several files has anything to choose.
            if download.files_total > 1
                && ui
                    .button(files_button_text(download, files.is_some()))
                    .clicked()
            {
                clicked = Some(Control::ToggleFiles(download.id));
            }
        });
    });
    if confirming {
        ui.group(|ui| {
            ui.colored_label(
                warning_color(ui),
                format!("Remove \"{}\" from the list?", download.name),
            );
            ui.small(
                "\"Delete the files too\" deletes only the files BitDinghy downloaded for it. \
                 Nothing else in the folder is touched.",
            );
            ui.horizontal(|ui| {
                if ui.button("Keep the files").clicked() {
                    clicked = Some(Control::Remove {
                        id: download.id,
                        delete_files: false,
                    });
                }
                if ui.button("Delete the files too").clicked() {
                    clicked = Some(Control::Remove {
                        id: download.id,
                        delete_files: true,
                    });
                }
                if ui.button("Cancel").clicked() {
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

    match download.status {
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
                ui.colored_label(info_color(ui), "Checking files already on disk...");
            });
        }
        DownloadStatus::Failed => {
            let message = download.error.as_deref().unwrap_or("The download failed.");
            ui.colored_label(error_color(ui), message);
        }
        DownloadStatus::Downloading | DownloadStatus::Paused | DownloadStatus::Finished => {
            let done = fraction(download.progress_bytes, download.total_bytes);
            ui.horizontal(|ui| {
                // The percentage sits beside the bar, where it is always readable.
                let bar_width = (ui.available_width() - 64.0).max(80.0);
                ui.add(
                    egui::ProgressBar::new(done)
                        .fill(brand::palette(ui.ctx().theme()).accent)
                        .desired_width(bar_width),
                );
                ui.label(format!("{:.1}%", done * 100.0));
            });
            if download.status == DownloadStatus::Paused {
                ui.label(paused_text(download));
            } else if download.status == DownloadStatus::Finished {
                let folder = download
                    .save_folder
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                ui.colored_label(success_color(ui), finished_text(download, &folder));
                ui.label(format!(
                    "Sharing back to {} people ({} uploaded so far). Closing BitDinghy stops sharing.",
                    download.peers_connected,
                    format_bytes(download.uploaded_bytes)
                ));
            } else {
                ui.label(format!(
                    "{} of {}   |   {}   |   {} left   |   {} people connected ({} known)",
                    format_bytes(download.progress_bytes),
                    format_bytes(download.total_bytes),
                    format_speed(download.download_bytes_per_second),
                    format_eta(download.eta),
                    download.peers_connected,
                    download.peers_known,
                ));
            }
        }
    }
    clicked
}

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([820.0, 600.0])
        .with_min_inner_size([480.0, 340.0]);
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
        Box::new(|creation_context| Ok(Box::new(BitDinghyApp::new(creation_context)))),
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
        }
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
