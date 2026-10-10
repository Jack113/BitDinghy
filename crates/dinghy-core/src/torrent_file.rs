//! Reading `.torrent` files safely.
//!
//! A `.torrent` file comes from a web site or a message, so like a magnet link it is untrusted.
//! It holds the torrent's file list (the "info" part, which the fingerprint is calculated from)
//! and, around it, extra fields: trackers, comments, web addresses... The engine would use the
//! trackers in it as they are, including ones pointing at this computer or the local network.
//!
//! So, exactly as for magnet links (`magnet.rs`), we never hand the original file to the engine.
//! We read out the parts we understand and check, and build a fresh, minimal `.torrent` from them:
//!
//! * the **info part, byte for byte** (changing it would change the fingerprint), after checking
//!   that it is a valid file list;
//! * the **trackers that pass the same filter as a magnet link's** (ordinary internet addresses
//!   only, at most 20).
//!
//! Everything else in the file is left out on purpose. The file list then goes through the same
//! safety guard as every other download when it is added.

use std::io::Read;
use std::path::Path;

use crate::error::{Error, Result};
use crate::magnet;

/// A `.torrent` file bigger than this is certainly not one (real ones are kilobytes, a few MB for
/// torrents with very many files).
pub const MAX_TORRENT_BYTES: u64 = 16 * 1024 * 1024;

/// A `.torrent` file that has been checked and rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedTorrent {
    /// The torrent's fingerprint as 40 hexadecimal characters.
    pub info_hash: String,
    /// The torrent's name, cleaned up for showing.
    pub display_name: Option<String>,
    /// The rebuilt, minimal `.torrent` file. This is the only version ever given to the engine.
    pub bytes: Vec<u8>,
    /// A clean magnet link for the same torrent (kept in the saved list as a fall-back).
    pub magnet: String,
}

fn invalid(message: String) -> Error {
    Error::InvalidTorrent(message)
}

/// Reads and checks a `.torrent` file from disk.
pub fn read(path: &Path) -> Result<CheckedTorrent> {
    let shown = path.display();
    let metadata = std::fs::metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            invalid(format!("There is no file at {shown}."))
        } else {
            invalid(format!("BitDinghy couldn't open {shown}: {e}"))
        }
    })?;
    if !metadata.is_file() {
        return Err(invalid(format!(
            "{shown} is a folder, not a .torrent file."
        )));
    }
    if metadata.len() > MAX_TORRENT_BYTES {
        return Err(too_big(path));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_TORRENT_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| invalid(format!("BitDinghy couldn't read {shown}: {e}")))?;
    if bytes.len() as u64 > MAX_TORRENT_BYTES {
        return Err(too_big(path));
    }
    check(&bytes).map_err(|_| invalid(format!("{shown} isn't a valid .torrent file.")))
}

fn too_big(path: &Path) -> Error {
    invalid(format!(
        "{} is far too big to be a .torrent file.",
        path.display()
    ))
}

/// Checks the contents of a `.torrent` file and rebuilds it from checked parts only.
pub fn check(bytes: &[u8]) -> Result<CheckedTorrent> {
    let not_a_torrent = || invalid("That isn't a valid .torrent file.".to_string());
    if bytes.len() as u64 > MAX_TORRENT_BYTES {
        return Err(not_a_torrent());
    }
    let parsed = librqbit::torrent_from_bytes(bytes).map_err(|_| not_a_torrent())?;
    // A valid file list: pieces, lengths and file names that make sense.
    let info = parsed
        .info
        .data
        .clone()
        .validate()
        .map_err(|_| not_a_torrent())?;
    let info_hash = parsed.info_hash.as_string();
    let display_name = info
        .name()
        .and_then(|name| magnet::clean_display_name(&name));

    let mut trackers: Vec<String> = Vec::new();
    for tracker in parsed.iter_announce() {
        if trackers.len() >= magnet::MAX_TRACKERS {
            break;
        }
        if let Ok(text) = std::str::from_utf8(tracker.0)
            && let Some(clean) = magnet::clean_tracker(text)
            && !trackers.contains(&clean)
        {
            trackers.push(clean);
        }
    }

    let rebuilt = rebuild(parsed.info.raw_bytes.0, &trackers);
    // The rebuilt file must describe exactly the same torrent.
    let check_again = librqbit::torrent_from_bytes(&rebuilt).map_err(|_| not_a_torrent())?;
    if check_again.info_hash != parsed.info_hash {
        return Err(not_a_torrent());
    }
    Ok(CheckedTorrent {
        magnet: magnet::build_link(&info_hash, display_name.as_deref(), &trackers),
        info_hash,
        display_name,
        bytes: rebuilt,
    })
}

/// Writes a minimal `.torrent` file: the trackers (if any) and the info part, byte for byte.
/// The format is "bencode": `d` ... `e` is a dictionary with its keys in sorted order, `l` ...
/// `e` a list, and `8:announce` the 8-byte text "announce".
fn rebuild(info: &[u8], trackers: &[String]) -> Vec<u8> {
    fn text(out: &mut Vec<u8>, value: &[u8]) {
        out.extend_from_slice(format!("{}:", value.len()).as_bytes());
        out.extend_from_slice(value);
    }
    let mut out = vec![b'd'];
    if let Some(first) = trackers.first() {
        text(&mut out, b"announce");
        text(&mut out, first.as_bytes());
        text(&mut out, b"announce-list");
        out.extend_from_slice(b"ll"); // One group holding all the trackers.
        for tracker in trackers {
            text(&mut out, tracker.as_bytes());
        }
        out.extend_from_slice(b"ee");
    }
    text(&mut out, b"info");
    out.extend_from_slice(info);
    out.push(b'e');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The info part of a tiny one-file torrent, written by hand.
    const INFO: &str =
        "d6:lengthi5e4:name9:hello.txt12:piece lengthi16384e6:pieces20:AAAAAAAAAAAAAAAAAAAAe";

    /// A `.torrent` file with extra (outer) fields before and after the info part. (The format
    /// wants the keys in sorted order: "announce" and "comment" come before "info", "url-list"
    /// after it.)
    fn torrent_with(before: &str, after: &str) -> Vec<u8> {
        format!("d{before}4:info{INFO}{after}e").into_bytes()
    }

    /// One text in the format: its length in bytes, a colon, then the text.
    fn t(text: &str) -> String {
        format!("{}:{text}", text.len())
    }

    fn announce_of(bytes: &[u8]) -> Vec<String> {
        let parsed = librqbit::torrent_from_bytes(bytes).unwrap();
        parsed
            .iter_announce()
            .map(|t| String::from_utf8(t.0.to_vec()).unwrap())
            .collect()
    }

    #[test]
    fn a_plain_torrent_keeps_its_fingerprint_and_name() {
        let original = torrent_with("", "");
        let checked = check(&original).unwrap();
        let expected = librqbit::torrent_from_bytes(&original).unwrap().info_hash;
        assert_eq!(checked.info_hash, expected.as_string());
        assert_eq!(checked.display_name.as_deref(), Some("hello.txt"));
        assert!(checked.magnet.starts_with("magnet:?xt=urn%3Abtih%3A"));
        assert!(announce_of(&checked.bytes).is_empty());
    }

    #[test]
    fn only_ordinary_internet_trackers_survive() {
        let home = "http://192.168.1.1:80/announce";
        let public = "udp://tracker.example.org:1337/abc";
        let this_pc = "http://localhost:6969/announce";
        let original = torrent_with(
            &format!(
                "{}{}{}ll{}{}{}{}ee",
                t("announce"),
                t(home),
                t("announce-list"),
                t(home),
                t(public),
                t(this_pc),
                t(public)
            ),
            "",
        );
        let checked = check(&original).unwrap();
        assert_eq!(
            announce_of(&checked.bytes),
            vec!["udp://tracker.example.org:1337/abc".to_string()]
        );
        assert!(checked.magnet.contains("tracker.example.org"));
        assert!(!checked.magnet.contains("192.168"));
        // Still exactly the same torrent.
        assert_eq!(check(&checked.bytes).unwrap().info_hash, checked.info_hash);
    }

    #[test]
    fn everything_else_in_the_file_is_left_out() {
        let original = torrent_with(
            &format!(
                "{}{}{}{}",
                t("comment"),
                t("hello there"),
                t("created by"),
                t("somebody")
            ),
            &format!("{}l{}e", t("url-list"), t("http://10.0.0.1/file.txt")),
        );
        let rebuilt = check(&original).unwrap().bytes;
        let text = String::from_utf8_lossy(&rebuilt);
        assert!(!text.contains("comment"));
        assert!(!text.contains("url-list"));
        assert!(!text.contains("10.0.0.1"));
        assert_eq!(rebuilt, format!("d4:info{INFO}e").into_bytes());
    }

    #[test]
    fn garbage_is_refused_with_a_plain_message() {
        for bad in [
            &b"not a torrent at all"[..],
            b"d4:infod4:name1:xee", // no pieces or length
            b"",
        ] {
            let error = check(bad).unwrap_err().to_string();
            assert!(error.contains("isn't a valid .torrent file"), "{error}");
        }
    }

    #[test]
    fn reading_from_disk_explains_every_problem() {
        let work = tempfile::tempdir().unwrap();
        let missing = work.path().join("missing.torrent");
        assert!(
            read(&missing)
                .unwrap_err()
                .to_string()
                .contains("There is no file")
        );
        assert!(
            read(work.path())
                .unwrap_err()
                .to_string()
                .contains("is a folder")
        );

        let not_torrent = work.path().join("notes.torrent");
        std::fs::write(&not_torrent, b"just some text").unwrap();
        assert!(
            read(&not_torrent)
                .unwrap_err()
                .to_string()
                .contains("isn't a valid .torrent file")
        );

        let huge = work.path().join("huge.torrent");
        let file = std::fs::File::create(&huge).unwrap();
        file.set_len(MAX_TORRENT_BYTES + 1).unwrap();
        assert!(read(&huge).unwrap_err().to_string().contains("far too big"));

        let good = work.path().join("good.torrent");
        std::fs::write(&good, torrent_with("", "")).unwrap();
        assert_eq!(
            read(&good).unwrap().display_name.as_deref(),
            Some("hello.txt")
        );
    }
}
