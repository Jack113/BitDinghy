//! BitDinghy's shared engine.
//!
//! Both programs (`dinghy`, the command line, and `BitDinghy`, the window) are thin front-ends
//! over this crate. Anything that matters, such as the download list, the queue, settings and
//! the safety checks, belongs here so it works identically in both.
//!
//! This crate is also the ONLY place allowed to talk to the BitTorrent engine (`librqbit`).
//! If we ever need to upgrade or swap that engine, only this crate changes.

use std::path::PathBuf;

pub mod crash;
pub mod data_dir;
pub mod days;
pub mod default_app;
pub mod display;
pub mod drive;
pub mod engine;
pub mod error;
pub mod file_choice;
pub mod input;
pub mod magnet;
pub mod owned_files;
pub mod plain;
pub mod profile;
pub mod recommend;
pub mod remote;
pub mod safety;
pub mod saved_list;
pub mod settings;
pub mod torrent_file;
pub mod web_link;

pub use data_dir::{DataDir, DataKind};
pub use engine::{
    AddOptions, DownloadId, DownloadSnapshot, DownloadStatus, Engine, EngineConfig, Move,
};
pub use error::{Error, Result};
pub use file_choice::FileChoice;
pub use profile::Profile;
pub use saved_list::{DayTotal, HistoryEntry, SavedDownload, SavedList, Sharing};
pub use settings::Settings;

/// What BitDinghy says about encryption (M17). librqbit has no BitTorrent encryption yet (its
/// pull request ikatson/rqbit#633 isn't merged), so connections are plain, and the person is told.
pub const ENCRYPTION_NOTE: &str = "Connections to other people aren't encrypted yet: the engine BitDinghy is built on doesn't support it. Your internet provider can see BitTorrent traffic, including which torrents. Encryption comes when the engine supports it.";

/// The program's display name.
pub const APP_NAME: &str = "BitDinghy";

/// This build's version. Cargo fills it in at compile time from `Cargo.toml`
/// (like `AssemblyVersion` in .NET, but with no extra step).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The one-line identity shown by `dinghy --version` and at the top of the window.
pub fn about() -> String {
    format!("{APP_NAME} {VERSION}")
}

/// Where downloads go unless the user picks another folder: a `BitDinghy` folder inside the
/// user's normal Downloads folder.
pub fn default_download_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(|p| p.join(APP_NAME)))
        .unwrap_or_else(|| PathBuf::from(APP_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_contains_name_and_version() {
        let text = about();
        assert!(text.contains(APP_NAME));
        assert!(text.contains(VERSION));
    }

    #[test]
    fn default_download_folder_is_named_after_the_app() {
        assert!(default_download_dir().ends_with(APP_NAME));
    }
}
