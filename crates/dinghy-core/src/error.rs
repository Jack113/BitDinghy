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
}

/// Shorthand used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
