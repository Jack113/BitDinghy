//! What the person typed, pasted or dropped: a magnet link, a `.torrent` file, or a web link to one.
//!
//! This only decides WHICH kind it is (and checks a magnet link). Reading a `.torrent` file
//! (`torrent_file.rs`) and fetching a web link happen when it is added.

use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::magnet::{self, ParsedMagnet};

/// One thing to download, before it is added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Magnet(ParsedMagnet),
    /// A `.torrent` file on this computer.
    TorrentFile(PathBuf),
    /// A web link (http or https) to a `.torrent` file.
    WebLink(String),
}

/// Works out what kind of input this is. Surrounding spaces and quotation marks (Windows'
/// "Copy as path" adds them) are ignored.
pub fn classify(text: &str) -> Result<Input> {
    let text = text.trim().trim_matches('"').trim();
    if text.is_empty() {
        return Err(Error::InvalidMagnet(
            "Paste a magnet link, or the path of a .torrent file, first.".to_string(),
        ));
    }
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("magnet:") {
        return magnet::parse(text).map(Input::Magnet);
    }
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Ok(Input::WebLink(text.to_string()));
    }
    // A network location (`\\server\share\x.torrent`): even looking at it makes Windows contact that
    // computer and sign in to it with this person's account. Copy the file to this PC first.
    if text.starts_with(r"\\") || text.starts_with("//") {
        return Err(Error::InvalidTorrent(
            "BitDinghy doesn't open .torrent files from network locations (paths starting with \
             \\\\). Copy the file to this PC first."
                .to_string(),
        ));
    }
    let path = PathBuf::from(text);
    let named_like_a_torrent = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("torrent"));
    if named_like_a_torrent || path.is_file() {
        return Ok(Input::TorrentFile(path));
    }
    Err(Error::InvalidMagnet(
        "That isn't a magnet link or a .torrent file. Magnet links start with magnet:?xt=urn:btih:, \
         and .torrent files end in .torrent."
            .to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "c9e15763f722f23e98a29decdfae341b98d53056";

    #[test]
    fn magnet_links_are_checked_straight_away() {
        let input = classify(&format!("  magnet:?xt=urn:btih:{HASH}  ")).unwrap();
        assert!(matches!(input, Input::Magnet(parsed) if parsed.info_hash == HASH));
        assert!(classify("magnet:?xt=urn:btih:nothex").is_err());
        assert!(matches!(
            classify(&format!("MAGNET:?xt=urn:btih:{HASH}")),
            Ok(Input::Magnet(_))
        ));
    }

    #[test]
    fn torrent_files_are_recognised_with_or_without_quotes() {
        for text in [
            r"C:\Downloads\ubuntu.torrent",
            r#""C:\Downloads\My Files\ubuntu.TORRENT""#,
            "relative/arch.torrent",
        ] {
            assert!(
                matches!(classify(text), Ok(Input::TorrentFile(_))),
                "{text}"
            );
        }
        // An existing file without the usual ending is tried as a .torrent too.
        let work = tempfile::tempdir().unwrap();
        let odd = work.path().join("download");
        std::fs::write(&odd, b"x").unwrap();
        assert_eq!(
            classify(&odd.display().to_string()).unwrap(),
            Input::TorrentFile(odd)
        );
    }

    #[test]
    fn network_locations_are_refused_without_being_touched() {
        for text in [
            r"\\203.0.113.5\share\x.torrent",
            r#""\\nas\downloads\x.torrent""#,
            r"\\?\UNC\server\share\x.torrent",
            "//server/share/x.torrent",
        ] {
            let error = classify(text).unwrap_err().to_string();
            assert!(error.contains("network locations"), "{text}: {error}");
        }
    }

    #[test]
    fn web_links_are_recognised() {
        assert!(matches!(
            classify("https://example.org/ubuntu.iso.torrent"),
            Ok(Input::WebLink(_))
        ));
        assert!(matches!(
            classify("HTTP://example.org/x.torrent"),
            Ok(Input::WebLink(_))
        ));
    }

    #[test]
    fn anything_else_is_explained() {
        for text in ["", "   ", "hello", "C:\\no-such-folder\\notes.txt"] {
            let error = classify(text).unwrap_err().to_string();
            assert!(error.contains("magnet link"), "{text:?} gave: {error}");
        }
    }
}
