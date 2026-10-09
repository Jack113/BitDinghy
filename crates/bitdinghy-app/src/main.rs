//! The `BitDinghy` window. A thin front-end over `dinghy-core`.

// In release builds on Windows, don't pop up a black console window behind the app.
// (Debug builds keep the console so log messages are visible while developing.)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::sync::Arc;
use std::time::Duration;

use dinghy_core::display::{format_bytes, format_eta, format_speed, fraction, waiting_text};
use dinghy_core::{DownloadSnapshot, DownloadStatus, Engine, EngineConfig};
use eframe::egui;

const ERROR_COLOR: egui::Color32 = egui::Color32::from_rgb(224, 92, 92);

/// A message shown under the link box.
struct Notice {
    text: String,
}

/// The whole window's state.
///
/// egui is an "immediate mode" toolkit: instead of building widgets once and updating them,
/// `ui()` below is called again for every frame drawn and describes the screen from scratch,
/// using the latest numbers from the engine.
struct BitDinghyApp {
    /// Owns the background threads the engine runs on. It must outlive the engine.
    runtime: tokio::runtime::Runtime,
    /// `Err` holds the reason the engine couldn't start, so the window can show it.
    engine: Result<Arc<Engine>, String>,
    /// The text in the link box.
    link: String,
    notice: Option<Notice>,
}

impl BitDinghyApp {
    fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("couldn't start BitDinghy's background threads");
        let config = EngineConfig::new(dinghy_core::default_download_dir());
        let engine = runtime
            .block_on(Engine::start(config))
            .map(Arc::new)
            .map_err(|error| error.to_string());
        Self {
            runtime,
            engine,
            link: String::new(),
            notice: None,
        }
    }

    fn submit_link(&mut self, engine: &Engine) {
        match engine.add_magnet(&self.link, Vec::new()) {
            Ok(_) => {
                self.link.clear();
                self.notice = None;
            }
            Err(error) => {
                self.notice = Some(Notice {
                    text: error.to_string(),
                });
            }
        }
    }
}

impl eframe::App for BitDinghyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading(dinghy_core::about());

        // If the engine couldn't start there is nothing else to show.
        let engine = match &self.engine {
            Ok(engine) => engine.clone(),
            Err(message) => {
                ui.add_space(8.0);
                ui.colored_label(ERROR_COLOR, message);
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
        if let Some(notice) = &self.notice {
            ui.colored_label(ERROR_COLOR, &notice.text);
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
            let _ = self.runtime.block_on(tokio::time::timeout(
                Duration::from_secs(3),
                engine.shutdown(),
            ));
        }
    }
}

/// Draws one download: its name, then a progress bar and numbers (or a message).
fn draw_download(ui: &mut egui::Ui, download: &DownloadSnapshot) {
    ui.label(egui::RichText::new(&download.name).strong());

    match download.status {
        DownloadStatus::GettingInfo => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(waiting_text(download.waiting_for.unwrap_or_default()));
            });
        }
        DownloadStatus::Checking => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Checking files already on disk...");
            });
        }
        DownloadStatus::Failed => {
            let message = download.error.as_deref().unwrap_or("The download failed.");
            ui.colored_label(ERROR_COLOR, message);
        }
        DownloadStatus::Downloading | DownloadStatus::Paused | DownloadStatus::Finished => {
            ui.add(
                egui::ProgressBar::new(fraction(download.progress_bytes, download.total_bytes))
                    .show_percentage(),
            );
            if download.status == DownloadStatus::Finished {
                let folder = download
                    .save_folder
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                ui.label(format!(
                    "Finished: {} saved in {folder}",
                    format_bytes(download.total_bytes)
                ));
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
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([820.0, 520.0])
            .with_min_inner_size([480.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        dinghy_core::APP_NAME,
        options,
        Box::new(|_creation_context| Ok(Box::new(BitDinghyApp::new()))),
    )
}
