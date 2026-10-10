//! The errors the engine can report. Every message is written for a person, not a programmer.

use std::path::PathBuf;

use crate::safety::UnsafePath;

/// Anything that can go wrong in `dinghy-core`.
///
/// In Rust, errors are ordinary values that functions return (`Result<T, Error>`) instead of
/// exceptions that get thrown. `thiserror` writes the boilerplate that makes this enum behave
/// like a proper error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The pasted text isn't a usable magnet link. The message says what to do about it.
    #[error("{0}")]
    InvalidMagnet(String),

    /// A file name inside the torrent could be used to write outside the download folder.
    #[error(
        "BitDinghy refused this torrent because it contains a file name that isn't safe to save: {0}"
    )]
    UnsafeFileName(#[from] UnsafePath),

    #[error("Couldn't start the download engine: {0}")]
    EngineStart(String),

    #[error("Couldn't start this download: {0}")]
    Add(String),

    #[error("The download folder can't be used ({}): {reason}", .path.display())]
    DownloadDir { path: PathBuf, reason: String },

    /// BitDinghy's own data folder (where settings are kept) can't be created or written to.
    #[error("BitDinghy can't keep its files in {}: {reason}", .path.display())]
    DataDir { path: PathBuf, reason: String },

    /// Settings couldn't be saved (or must not be, for example when a newer version wrote them).
    #[error("{0}")]
    Settings(String),

    /// Another BitDinghy (the window, or `dinghy` in another terminal) is already using this data
    /// folder. Two engines would download into the same files and fight over the saved list.
    #[error(
        "BitDinghy is already running (in its window or in another terminal). Use that one, or \
         close it first."
    )]
    AlreadyRunning,

    /// The saved download list couldn't be read or changed.
    #[error("{0}")]
    SavedList(String),

    /// Pausing, resuming or removing a download didn't work. The message says why.
    #[error("{0}")]
    Control(String),

    /// The command line couldn't talk to the open window. The message says why.
    #[error("{0}")]
    Remote(String),

    /// A new download would have overwritten a file that BitDinghy didn't download.
    #[error(
        "BitDinghy didn't start this download because a file with the same name is already in \
         the download folder: {}. BitDinghy never overwrites files it didn't download. Move or \
         rename that file, then try again.",
        .0.display()
    )]
    NameClash(PathBuf),
}

/// Shorthand used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
