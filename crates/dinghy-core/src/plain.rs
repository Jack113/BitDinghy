//! Problems from the engine, said in plain words (M12).
//!
//! librqbit reports problems as technical text, often ending in an operating-system code such as
//! "(os error 112)". [`explain`] recognises the common ones (a full disk, no permission, a file
//! another program holds, a drive that went away...) and puts a plain sentence with what to do
//! first. The technical text stays at the end, after "Details:", because it helps when asking for
//! help.

/// A plain-language version of an engine problem. Unknown problems come back unchanged.
pub fn explain(technical: &str) -> String {
    let Some(plain) = recognise(technical) else {
        return technical.to_string();
    };
    format!("{plain} Details: {technical}")
}

/// The plain sentence for a problem we recognise.
fn recognise(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    let code = os_error_code(&lower);
    let has = |words: &str| lower.contains(words);
    // The same number means different things on Windows and on Linux, so each list of codes
    // only counts on its own system; the words the system uses count everywhere.
    let windows = |codes: &[i32]| cfg!(windows) && code.is_some_and(|c| codes.contains(&c));
    let unix = |codes: &[i32]| cfg!(unix) && code.is_some_and(|c| codes.contains(&c));
    if windows(&[112, 39])
        || unix(&[28])
        || has("no space left")
        || has("not enough space on the disk")
    {
        return Some(
            "The disk is full, so BitDinghy can't save more of this download. Free some space \
             (or choose a download folder on another drive in Settings), then try again; \
             what was already downloaded is kept.",
        );
    }
    if windows(&[223]) || unix(&[27]) || has("file too large") || has("exceeds the limit") {
        return Some(
            "A file in this download is too big for the drive's format (FAT32 drives can't hold \
             files of 4 GB or more). Choose a download folder on a drive formatted as NTFS in \
             Settings.",
        );
    }
    if windows(&[5])
        || unix(&[13, 1, 30])
        || has("access is denied")
        || has("permission denied")
        || has("read-only file system")
    {
        return Some(
            "Windows didn't let BitDinghy write in the download folder. Choose another folder in \
             Settings, or check that the folder isn't read-only and that you may write in it.",
        );
    }
    if windows(&[32, 33])
        || has("being used by another process")
        || has("locked a portion of the file")
    {
        return Some(
            "Another program has one of this download's files open (a video player or a virus \
             scanner, for example). Close it, then try again.",
        );
    }
    if windows(&[21, 1117, 1167, 55, 3])
        || unix(&[5, 6, 19])
        || has("device is not ready")
        || has("i/o device error")
        || has("device is not connected")
    {
        return Some(
            "The drive with the download folder isn't there or isn't working (was a USB drive \
             unplugged?). Plug it back in, then try again.",
        );
    }
    None
}

/// The number in "(os error 112)", if the text has one.
fn os_error_code(lower: &str) -> Option<i32> {
    let start = lower.rfind("os error ")? + "os error ".len();
    let digits: String = lower[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_disk_is_said_plainly_and_the_details_are_kept() {
        let technical = "error writing to file \"C:\\\\x\\\\a.iso\": There is not enough space \
                         on the disk. (os error 112)";
        let plain = explain(technical);
        assert!(plain.starts_with("The disk is full"), "{plain}");
        assert!(plain.ends_with(&format!("Details: {technical}")), "{plain}");
    }

    #[cfg(windows)]
    #[test]
    fn the_common_windows_problems_are_recognised() {
        for (technical, starts) in [
            (
                "Access is denied. (os error 5)",
                "Windows didn't let BitDinghy write",
            ),
            (
                "The process cannot access the file because it is being used by another \
                 process. (os error 32)",
                "Another program has one",
            ),
            (
                "The device is not ready. (os error 21)",
                "The drive with the download folder",
            ),
            (
                "The file size exceeds the limit allowed (os error 223)",
                "A file in this download",
            ),
            (
                "The system cannot find the path specified. (os error 3)",
                "The drive with",
            ),
        ] {
            let plain = explain(technical);
            assert!(plain.starts_with(starts), "{technical} -> {plain}");
        }
    }

    #[test]
    fn unknown_problems_come_back_unchanged() {
        assert_eq!(
            explain("torrent metadata invalid"),
            "torrent metadata invalid"
        );
        assert_eq!(explain("os error 99999"), "os error 99999");
    }

    #[test]
    fn the_code_is_read_from_the_end() {
        assert_eq!(os_error_code("bad (os error 5)"), Some(5));
        assert_eq!(os_error_code("no code here"), None);
    }
}
