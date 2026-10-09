//! The `BitDinghy` window. A thin front-end over `dinghy-core`.

// In release builds on Windows, don't pop up a black console window behind the app.
// (Debug builds keep the console so log messages are visible while developing.)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod brand;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use dinghy_core::display::{format_bytes, format_eta, format_speed, fraction, waiting_text};
use dinghy_core::{DownloadSnapshot, DownloadStatus, Engine, Profile};
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
        }
    }

    fn submit_link(&mut self, engine: &Engine) {
        match engine.add_magnet(&self.link, Vec::new()) {
            Ok(_) => {
                self.link.clear();
                self.link_problem = None;
            }
            Err(error) => self.link_problem = Some(error.to_string()),
        }
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

        // Redraw a couple of times a second so progress keeps moving.
        ui.ctx().request_repaint_after(Duration::from_millis(500));

        ui.label(format!("Saving to: {}", engine.download_dir().display()));
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let box_width = (ui.available_width() - 100.0).max(120.0);
            let link_box = ui.add(
                egui::TextEdit::singleline(&mut self.link)
                    .hint_text("Paste a magnet link here")
                    .desired_width(box_width),
            );
            let enter_pressed =
                link_box.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if ui.button("Download").clicked() || enter_pressed {
                self.submit_link(&engine);
            }
        });
        if let Some(problem) = &self.link_problem {
            ui.colored_label(error_color(ui), problem);
        }

        ui.add_space(4.0);
        ui.separator();

        let downloads = engine.snapshot();
        if downloads.is_empty() {
            ui.add_space(8.0);
            ui.label("Nothing downloading yet. Paste a magnet link above and press Download.");
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for download in &downloads {
                draw_download(ui, download);
                ui.separator();
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

/// Draws one download: its name, then a progress bar and numbers (or a message).
fn draw_download(ui: &mut egui::Ui, download: &DownloadSnapshot) {
    ui.label(egui::RichText::new(&download.name).strong());

    match download.status {
        DownloadStatus::GettingInfo => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.colored_label(
                    info_color(ui),
                    waiting_text(download.waiting_for.unwrap_or_default()),
                );
            });
        }
        DownloadStatus::Checking => {
            ui.horizontal(|ui| {
                ui.spinner();
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
            if download.status == DownloadStatus::Finished {
                let folder = download
                    .save_folder
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                ui.colored_label(
                    success_color(ui),
                    format!(
                        "Finished: {} saved in {folder}",
                        format_bytes(download.total_bytes)
                    ),
                );
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
