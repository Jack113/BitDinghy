//! The file-name guard: the rule that BitDinghy never writes outside the download folder.
//!
//! A torrent is written by a stranger, and so are the file names inside it. The BitTorrent
//! engine (librqbit) already rejects a few things (`..` and slashes), but it does NOT reject
//! everything that is dangerous on Windows: a name like `C:evil` makes Windows treat the path as
//! living on a different drive, and names like `NUL` or `CON` are devices rather than files.
//! So we check every name ourselves, strictly, BEFORE any file is created.
//!
//! The rules are written for both Windows and Unix and take the platform as a parameter, so the
//! tests can check the Windows rules on any computer (including the Linux CI machine).

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

use librqbit::ValidatedTorrentMetaV1Info;

/// Which operating system's file-name rules to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Unix,
}

impl Platform {
    /// The rules for the computer we are running on.
    pub const fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Unix
        }
    }
}

/// Why a name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Empty,
    CurrentOrParentDirectory,
    PathSeparator,
    ControlCharacter,
    TooLong,
    TooDeep,
    NotARelativePath,
    WindowsReservedCharacter,
    WindowsDeviceName,
    WindowsTrailingDotOrSpace,
    /// Several files but no name for their folder. The BitTorrent standard requires a name;
    /// without one the engine would name the folder after the magnet link's `dn=` (chosen by
    /// whoever wrote the link) or after a file, and could choose differently on the next start.
    MissingFolderName,
}

impl Reason {
    fn describe(self) -> &'static str {
        match self {
            Reason::Empty => "a file name is empty",
            Reason::CurrentOrParentDirectory => "a name tries to point at another folder",
            Reason::PathSeparator => "a name contains a slash",
            Reason::ControlCharacter => "a name contains an invisible control character",
            Reason::TooLong => "a name is far too long",
            Reason::TooDeep => "files are nested far too deeply",
            Reason::NotARelativePath => "a name is not a plain path inside the folder",
            Reason::WindowsReservedCharacter => {
                "a name contains a character Windows does not allow"
            }
            Reason::WindowsDeviceName => "a name is a reserved Windows device name",
            Reason::WindowsTrailingDotOrSpace => "a name ends with a dot or a space",
            Reason::MissingFolderName => "it has several files but no name for their folder",
        }
    }
}

/// `: "name"` after the reason, or nothing when there is no name to show.
fn quoted_suffix(name: &str) -> String {
    if name.is_empty() {
        String::new()
    } else {
        format!(": \"{name}\"")
    }
}

/// A name that was refused, with the reason. The name is shown safely escaped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}{}", .reason.describe(), quoted_suffix(.name))]
pub struct UnsafePath {
    pub reason: Reason,
    pub name: String,
}

impl UnsafePath {
    fn new(reason: Reason, name: &str) -> Self {
        Self {
            reason,
            name: escape_for_display(name),
        }
    }

    /// For the engine's second check: a folder that is not inside the download folder.
    pub(crate) fn outside_download_folder(folder: &Path) -> Self {
        Self::new(Reason::NotARelativePath, &folder.to_string_lossy())
    }
}

/// A download's folder must be the download folder itself (a torrent with one file) or ONE
/// plain-named folder directly inside it (a torrent with several files). That is the only shape
/// BitDinghy ever creates, so anything else (deeper, `..`, elsewhere) is refused.
pub fn check_download_folder(download_dir: &Path, folder: &Path) -> Result<(), UnsafePath> {
    let fits = folder.strip_prefix(download_dir).is_ok_and(|inside| {
        let parts: Vec<Component<'_>> = inside.components().collect();
        parts.len() <= 1
            && parts
                .iter()
                .all(|part| matches!(part, Component::Normal(_)))
    });
    if fits {
        Ok(())
    } else {
        Err(UnsafePath::outside_download_folder(folder))
    }
}

/// Longest single name we accept, in bytes (the limit of most file systems).
const MAX_COMPONENT_BYTES: usize = 255;
/// Deepest folder nesting we accept. Real torrents are nowhere near this.
const MAX_DEPTH: usize = 64;

/// Makes a hostile name safe to print: control characters become `\u{..}`, and very long names
/// are cut short.
fn escape_for_display(name: &str) -> String {
    let mut out = String::new();
    for (count, ch) in name.chars().enumerate() {
        if count >= 80 {
            out.push('…');
            break;
        }
        if ch.is_control() {
            out.push_str(&format!("\\u{{{:x}}}", ch as u32));
        } else {
            out.push(ch);
        }
    }
    out
}

/// Windows treats these as devices, even with an extension (`nul.txt`) or trailing spaces.
fn is_windows_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end_matches(' ');
    let upper = stem.to_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    // COM0-COM9 and LPT0-LPT9, including the superscript digits Windows also reserves.
    let rest = upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"));
    match rest {
        Some(digit) => {
            let mut chars = digit.chars();
            matches!(
                (chars.next(), chars.next()),
                (Some('0'..='9' | '¹' | '²' | '³'), None)
            )
        }
        None => false,
    }
}

/// Checks ONE name (one folder or file name, never a whole path).
pub fn check_component(name: &str, platform: Platform) -> Result<(), UnsafePath> {
    let refuse = |reason| Err(UnsafePath::new(reason, name));

    if name.is_empty() {
        return refuse(Reason::Empty);
    }
    if name == "." || name == ".." {
        return refuse(Reason::CurrentOrParentDirectory);
    }
    if name.contains('/') || name.contains('\\') {
        return refuse(Reason::PathSeparator);
    }
    if name.chars().any(|c| c.is_control()) {
        return refuse(Reason::ControlCharacter);
    }
    if name.len() > MAX_COMPONENT_BYTES {
        return refuse(Reason::TooLong);
    }

    if platform == Platform::Windows {
        // `:` also covers drive letters (`C:evil`) and hidden data streams (`file.txt:secret`).
        if name
            .chars()
            .any(|c| matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return refuse(Reason::WindowsReservedCharacter);
        }
        if name.ends_with('.') || name.ends_with(' ') {
            return refuse(Reason::WindowsTrailingDotOrSpace);
        }
        if is_windows_device_name(name) {
            return refuse(Reason::WindowsDeviceName);
        }
    }
    Ok(())
}

/// Checks a whole file path, given as its folder and file names in order.
pub fn check_file_path<'a>(
    components: impl IntoIterator<Item = &'a str>,
    platform: Platform,
) -> Result<(), UnsafePath> {
    let mut count = 0;
    for name in components {
        count += 1;
        if count > MAX_DEPTH {
            return Err(UnsafePath::new(Reason::TooDeep, name));
        }
        check_component(name, platform)?;
    }
    if count == 0 {
        return Err(UnsafePath::new(Reason::Empty, ""));
    }
    Ok(())
}

/// Checks everything a torrent wants to create: its top-level name and every file path.
pub fn check_torrent<B: AsRef<[u8]>>(
    info: &ValidatedTorrentMetaV1Info<B>,
    platform: Platform,
) -> Result<(), UnsafePath> {
    match info.name() {
        Some(name) => check_component(&name, platform)?,
        // The engine puts two or more files in a folder named after the torrent. (One file goes
        // straight into the download folder and needs no folder name.)
        None if info.iter_file_details().nth(1).is_some() => {
            return Err(UnsafePath::new(Reason::MissingFolderName, ""));
        }
        None => {}
    }
    for file in info.iter_file_details() {
        let parts: Vec<Cow<'_, str>> = file.filename.iter_components().collect();
        check_file_path(parts.iter().map(|part| part.as_ref()), platform)?;
    }
    Ok(())
}

/// Last line of defence: joins `relative` onto `root`, but only if `relative` is made of plain
/// names. Anything that could jump elsewhere (a drive prefix, a leading slash, `..`) is refused,
/// so the result is always inside `root`.
pub fn join_inside(root: &Path, relative: &Path) -> Result<PathBuf, UnsafePath> {
    if relative
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(UnsafePath::new(
            Reason::NotARelativePath,
            &relative.to_string_lossy(),
        ));
    }
    Ok(root.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: Platform = Platform::Windows;
    const NIX: Platform = Platform::Unix;

    fn reason(name: &str, platform: Platform) -> Option<Reason> {
        check_component(name, platform).err().map(|e| e.reason)
    }

    #[test]
    fn ordinary_names_are_fine_everywhere() {
        for name in [
            "ubuntu-24.04-desktop-amd64.iso",
            "My Holiday Photos",
            "résumé.pdf",
            "日本語.txt",
            ".hidden",
            "archive.tar.gz",
        ] {
            assert_eq!(reason(name, WIN), None, "{name}");
            assert_eq!(reason(name, NIX), None, "{name}");
        }
    }

    #[test]
    fn parent_and_current_directory_are_refused_everywhere() {
        assert_eq!(reason("..", NIX), Some(Reason::CurrentOrParentDirectory));
        assert_eq!(reason(".", WIN), Some(Reason::CurrentOrParentDirectory));
    }

    #[test]
    fn slashes_are_refused_everywhere() {
        assert_eq!(reason("a/b", NIX), Some(Reason::PathSeparator));
        assert_eq!(reason("..\\..\\evil", NIX), Some(Reason::PathSeparator));
        assert_eq!(reason("/etc/passwd", WIN), Some(Reason::PathSeparator));
    }

    #[test]
    fn empty_and_control_characters_are_refused() {
        assert_eq!(reason("", NIX), Some(Reason::Empty));
        assert_eq!(reason("bad\u{7}name", NIX), Some(Reason::ControlCharacter));
        assert_eq!(reason("nul\0byte", WIN), Some(Reason::ControlCharacter));
        assert_eq!(reason("tab\there", NIX), Some(Reason::ControlCharacter));
    }

    #[test]
    fn overlong_names_are_refused() {
        assert_eq!(reason(&"a".repeat(256), NIX), Some(Reason::TooLong));
        assert_eq!(reason(&"a".repeat(255), NIX), None);
    }

    #[test]
    fn windows_drive_letters_and_streams_are_refused() {
        assert_eq!(
            reason("C:evil", WIN),
            Some(Reason::WindowsReservedCharacter)
        );
        assert_eq!(reason("C:", WIN), Some(Reason::WindowsReservedCharacter));
        assert_eq!(
            reason("notes.txt:hidden", WIN),
            Some(Reason::WindowsReservedCharacter)
        );
        // A colon is a normal character on Linux, so it's fine there.
        assert_eq!(reason("Album: Title.mp3", NIX), None);
    }

    #[test]
    fn windows_reserved_characters_are_refused_only_on_windows() {
        for name in ["a<b", "a>b", "a\"b", "a|b", "a?b", "a*b"] {
            assert_eq!(reason(name, WIN), Some(Reason::WindowsReservedCharacter));
            assert_eq!(reason(name, NIX), None, "{name}");
        }
    }

    #[test]
    fn windows_device_names_are_refused_in_any_case_and_with_extensions() {
        for name in [
            "NUL",
            "nul",
            "Con",
            "PRN",
            "aux",
            "COM1",
            "com9",
            "LPT1",
            "lpt0",
            "nul.txt",
            "CON.tar.gz",
            "COM¹",
            "nul ",
        ] {
            assert!(
                reason(name, WIN).is_some(),
                "{name} should be refused on Windows"
            );
        }
        // Names that merely start the same way are fine.
        for name in [
            "console",
            "COM10",
            "com",
            "lpt",
            "nullable",
            "aux-notes.txt",
        ] {
            assert_eq!(reason(name, WIN), None, "{name}");
        }
        assert_eq!(reason("NUL", NIX), None);
    }

    #[test]
    fn windows_trailing_dots_and_spaces_are_refused() {
        assert_eq!(
            reason("file.", WIN),
            Some(Reason::WindowsTrailingDotOrSpace)
        );
        assert_eq!(
            reason("file ", WIN),
            Some(Reason::WindowsTrailingDotOrSpace)
        );
        assert_eq!(reason("...", WIN), Some(Reason::WindowsTrailingDotOrSpace));
        assert_eq!(reason("file.", NIX), None);
    }

    #[test]
    fn a_whole_path_is_checked_piece_by_piece() {
        assert!(check_file_path(["Movies", "2024", "film.mkv"], WIN).is_ok());
        let err = check_file_path(["Movies", "..", "film.mkv"], NIX).unwrap_err();
        assert_eq!(err.reason, Reason::CurrentOrParentDirectory);
        assert_eq!(
            check_file_path(Vec::<&str>::new(), NIX).unwrap_err().reason,
            Reason::Empty
        );
    }

    #[test]
    fn absurdly_deep_paths_are_refused() {
        let deep = vec!["d"; 65];
        assert_eq!(
            check_file_path(deep, NIX).unwrap_err().reason,
            Reason::TooDeep
        );
        assert!(check_file_path(vec!["d"; 64], NIX).is_ok());
    }

    #[test]
    fn hostile_names_are_escaped_when_shown() {
        let err = check_component("evil\u{1b}[31m\n", NIX).unwrap_err();
        assert!(!err.name.contains('\u{1b}'));
        assert!(!err.name.contains('\n'));
        assert!(err.name.contains("\\u{1b}"));
        // A huge name is cut short instead of flooding the screen.
        let err = check_component(&"é".repeat(1000), NIX).unwrap_err();
        assert!(err.name.chars().count() <= 81);
        assert!(err.to_string().len() < 400);
    }

    /// Documents WHY this module exists: on Windows, joining a name that starts with a drive
    /// letter onto the download folder does not end up inside it. Without our guard, a torrent
    /// file called `C:evil` would be written somewhere else entirely.
    #[cfg(windows)]
    #[test]
    fn why_the_guard_exists_a_drive_letter_name_escapes_the_folder() {
        let root = Path::new(r"D:\downloads");
        let escaped = root.join("C:evil");
        assert!(!escaped.starts_with(root), "{escaped:?}");
        assert!(join_inside(root, Path::new("C:evil")).is_err());
    }

    #[test]
    fn join_inside_accepts_plain_relative_paths() {
        let root = Path::new("downloads");
        let joined = join_inside(root, Path::new("folder/file.txt")).unwrap();
        assert!(joined.starts_with(root));
    }

    #[test]
    fn a_download_folder_is_the_folder_itself_or_one_plain_folder_inside_it() {
        let downloads = Path::new("downloads");
        assert!(check_download_folder(downloads, downloads).is_ok());
        assert!(check_download_folder(downloads, &downloads.join("album")).is_ok());
        for wrong in [
            downloads.join("a").join("b"),
            // `Path::parent()` would call this "one folder inside downloads"; it is the folder
            // ABOVE it.
            downloads.join(".."),
            downloads.join("..").join("elsewhere"),
            PathBuf::from("elsewhere"),
            PathBuf::from("down"),
        ] {
            assert!(
                check_download_folder(downloads, &wrong).is_err(),
                "{wrong:?} must be refused"
            );
        }
    }

    #[test]
    fn join_inside_refuses_anything_that_could_escape() {
        let root = Path::new("downloads");
        assert!(join_inside(root, Path::new("../escape.txt")).is_err());
        assert!(join_inside(root, Path::new("a/../../escape.txt")).is_err());
        #[cfg(unix)]
        assert!(join_inside(root, Path::new("/etc/passwd")).is_err());
        #[cfg(windows)]
        {
            assert!(join_inside(root, Path::new("C:evil.txt")).is_err());
            assert!(join_inside(root, Path::new("C:\\Windows\\evil.txt")).is_err());
            assert!(join_inside(root, Path::new("\\\\server\\share\\evil.txt")).is_err());
            assert!(join_inside(root, Path::new("\\evil.txt")).is_err());
        }
    }
}
