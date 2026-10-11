//! Which files on disk belong to a download, and deleting them safely.
//!
//! BitDinghy only ever deletes files it downloaded itself. That rests on two rules:
//!
//! * **A new download never takes over an existing file** ([`refuse_existing_files`]). If a file
//!   with the same name is already in the folder, the download is refused, so every file at one of
//!   a download's paths was written by that download.
//! * **The paths come from the download's own checked file list** ([`files_of`]), never from
//!   looking around in the folder. Each path goes through the same safety guard as when the
//!   download started, and must sit inside the download folder.
//!
//! Deleting then removes exactly those files, plus any folders they leave empty, but never the
//! download folder itself.
//!
//! **Leftovers of unticked files** (M6). The engine creates every file of a torrent, ticked or
//! not: an unticked file stays empty, or holds a little data where a piece crosses into it from a
//! ticked neighbour (that data is needed to check the ticked file, so it can't be deleted). So
//! BitDinghy marks unticked files **hidden** ([`leftover_marks`], [`apply_hidden`]), and folders
//! holding only unticked files too. Ticking a file again makes it visible. Removing a download
//! while keeping its files makes everything visible again, so nothing BitDinghy no longer looks
//! after stays hidden. (Linux has no "hidden" mark; there this does nothing.)

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use librqbit::ValidatedTorrentMetaV1Info;

use crate::error::{Error, Result};
use crate::file_choice;
use crate::safety::{self, Platform};

/// The full path of every file a download writes, in order. Padding entries (a BitTorrent detail
/// that is never written to disk) are skipped.
pub fn paths_in<B: AsRef<[u8]>>(
    info: &ValidatedTorrentMetaV1Info<B>,
    output_folder: &Path,
) -> Result<Vec<PathBuf>> {
    safety::check_torrent(info, Platform::current())?;
    let mut paths = Vec::new();
    for file in info.iter_file_details() {
        if file.attrs().padding {
            continue;
        }
        paths.push(safety::join_inside(
            output_folder,
            &file.filename.to_pathbuf(),
        )?);
    }
    Ok(paths)
}

/// The files a saved download owns, from its saved file list (`torrent`, a `.torrent` file) and
/// its folder. `output_folder` must be the download folder itself (a one-file torrent) or one
/// folder directly inside it (a torrent with several files, in a folder named after it), which is
/// the only shape BitDinghy ever creates.
pub fn files_of(torrent: &[u8], download_dir: &Path, output_folder: &Path) -> Result<Vec<PathBuf>> {
    Ok(real_files(torrent, download_dir, output_folder)?
        .into_iter()
        .map(|(_, path)| path)
        .collect())
}

/// Like [`files_of`], with each file's index in the torrent (what a choice of files uses).
fn real_files(
    torrent: &[u8],
    download_dir: &Path,
    output_folder: &Path,
) -> Result<Vec<(usize, PathBuf)>> {
    let unreadable = |reason: String| {
        Error::Control(format!(
            "BitDinghy couldn't read this download's saved file list ({reason}), so it won't \
             delete anything."
        ))
    };
    if safety::check_download_folder(download_dir, output_folder).is_err() {
        return Err(unreadable(format!(
            "its folder {} isn't where BitDinghy puts downloads",
            output_folder.display()
        )));
    }
    let parsed = librqbit::torrent_from_bytes(torrent).map_err(|e| unreadable(e.to_string()))?;
    let info = parsed
        .info
        .data
        .validate()
        .map_err(|e| unreadable(e.to_string()))?;
    let paths = paths_in(&info, output_folder)?;
    let indexes = file_choice::files_in(&info)
        .into_iter()
        .map(|file| file.index);
    Ok(indexes.zip(paths).collect())
}

/// Which of a download's files and folders should be hidden (`true`) or visible (`false`), from
/// its saved file list and the files the person unticked (`skipped`; empty: show everything).
/// The paths are checked exactly like the ones [`files_of`] deletes.
pub fn leftover_marks(
    torrent: &[u8],
    download_dir: &Path,
    output_folder: &Path,
    skipped: &BTreeSet<usize>,
) -> Result<Vec<(PathBuf, bool)>> {
    let files: Vec<(PathBuf, bool)> = real_files(torrent, download_dir, output_folder)?
        .into_iter()
        .map(|(index, path)| (path, skipped.contains(&index)))
        .collect();
    Ok(marks_with_folders(output_folder, &files))
}

/// Adds the folders inside `output_folder` to `files` (path, hide): a folder is hidden when every
/// file inside it is. `output_folder` itself is never included.
fn marks_with_folders(output_folder: &Path, files: &[(PathBuf, bool)]) -> Vec<(PathBuf, bool)> {
    // Each folder, and whether everything inside it is hidden.
    let mut folders: BTreeMap<&Path, bool> = BTreeMap::new();
    for (path, hide) in files {
        let mut folder = path.parent();
        while let Some(current) = folder
            && current != output_folder
            && current.starts_with(output_folder)
        {
            *folders.entry(current).or_insert(true) &= *hide;
            folder = current.parent();
        }
    }
    let mut marks = files.to_vec();
    marks.extend(
        folders
            .into_iter()
            .map(|(folder, hide)| (folder.to_path_buf(), hide)),
    );
    marks
}

/// Marks each path hidden or visible, as listed. Paths that don't exist, and links, are skipped.
/// Best effort: returns how many couldn't be changed (a file that can't be hidden is harmless).
pub fn apply_hidden(marks: &[(PathBuf, bool)]) -> usize {
    marks
        .iter()
        .filter(|(path, hide)| hidden::set(path, *hide).is_err())
        .count()
}

/// The "hidden" mark itself. Only Windows has one.
#[cfg(windows)]
mod hidden {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::path::Path;

    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_NOT_CONTENT_INDEXED, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_READONLY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM, FILE_ATTRIBUTE_TEMPORARY,
        SetFileAttributesW,
    };

    /// The marks `SetFileAttributesW` accepts. Others (folder, sparse...) are kept by Windows.
    const SETTABLE: u32 = FILE_ATTRIBUTE_READONLY
        | FILE_ATTRIBUTE_HIDDEN
        | FILE_ATTRIBUTE_SYSTEM
        | FILE_ATTRIBUTE_ARCHIVE
        | FILE_ATTRIBUTE_TEMPORARY
        | FILE_ATTRIBUTE_OFFLINE
        | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED;

    pub fn set(path: &Path, hide: bool) -> std::io::Result<()> {
        // `symlink_metadata` looks at the path itself, never where a link points.
        let metadata = match path.symlink_metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let attributes = metadata.file_attributes();
        if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Ok(()); // A link or junction: not something BitDinghy wrote.
        }
        if (attributes & FILE_ATTRIBUTE_HIDDEN != 0) == hide {
            return Ok(()); // Already right.
        }
        let mut wanted = attributes & SETTABLE & !FILE_ATTRIBUTE_HIDDEN;
        if hide {
            wanted |= FILE_ATTRIBUTE_HIDDEN;
        }
        if wanted == 0 {
            wanted = FILE_ATTRIBUTE_NORMAL; // "No marks at all" is spelled NORMAL.
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: `wide` is a valid, zero-terminated wide string that outlives the call.
        if unsafe { SetFileAttributesW(wide.as_ptr(), wanted) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// Whether a path is marked hidden (for tests and checks).
    pub fn is_hidden(path: &Path) -> bool {
        path.symlink_metadata()
            .is_ok_and(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
    }
}

#[cfg(not(windows))]
mod hidden {
    use std::path::Path;

    pub fn set(_path: &Path, _hide: bool) -> std::io::Result<()> {
        Ok(())
    }

    pub fn is_hidden(_path: &Path) -> bool {
        false
    }
}

pub use hidden::is_hidden;

/// Refuses a NEW download if any of its files already exists: it would overwrite (or, when it is
/// removed later, delete) a file BitDinghy didn't download.
pub fn refuse_existing_files(paths: &[PathBuf]) -> Result<()> {
    match paths.iter().find(|path| path.symlink_metadata().is_ok()) {
        Some(existing) => Err(Error::NameClash(existing.clone())),
        None => Ok(()),
    }
}

/// Refuses a download if any folder or file on the way to its files (inside the download folder)
/// is a link to another place: a symbolic link or a junction. Writing "into" one would really
/// write somewhere else, outside the download folder. (Cloud placeholders, such as OneDrive's,
/// are not links and are fine.) The download folder itself is the person's own choice.
pub fn refuse_links(download_dir: &Path, paths: &[PathBuf]) -> Result<()> {
    let mut checked = std::collections::HashSet::new();
    for path in paths {
        let mut current = Some(path.as_path());
        while let Some(place) = current
            && place != download_dir
            && place.starts_with(download_dir)
        {
            if !checked.insert(place) {
                break; // This folder and everything above it were checked already.
            }
            // `symlink_metadata` looks at the place itself; on Windows `is_symlink` is true for
            // symbolic links and junctions.
            if place
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
            {
                return Err(Error::LinkInTheWay(place.to_path_buf()));
            }
            current = place.parent();
        }
    }
    Ok(())
}

/// What deleting a download's files did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Deleted {
    /// How many files were deleted.
    pub files: usize,
    /// Files that couldn't be deleted, each with the reason. Files already gone are not problems.
    pub problems: Vec<String>,
}

/// Deletes these files, then any folders they leave empty (deepest first), up to `output_folder`.
/// The download folder itself is never removed, and a folder that still holds anything (for
/// example a file the person added) is left alone.
pub fn delete(paths: &[PathBuf], download_dir: &Path, output_folder: &Path) -> Deleted {
    let mut result = Deleted::default();
    for path in paths {
        match remove_file_patiently(path) {
            Ok(true) => result.files += 1,
            Ok(false) => {} // Already gone.
            Err(error) => result.problems.push(format!("{}: {error}", path.display())),
        }
    }

    let mut folders: Vec<&Path> = Vec::new();
    for path in paths {
        let mut folder = path.parent();
        while let Some(current) = folder {
            if !current.starts_with(output_folder) || current == download_dir {
                break;
            }
            if !folders.contains(&current) {
                folders.push(current);
            }
            folder = current.parent();
        }
    }
    folders.sort_by_key(|folder| std::cmp::Reverse(folder.components().count()));
    for folder in folders {
        // `remove_dir` only removes an EMPTY folder, which is exactly what we want.
        let _ = std::fs::remove_dir(folder);
    }
    result
}

/// Removes a file. Returns `Ok(false)` if it was already gone. On Windows a file can stay "in use"
/// for a moment after the engine lets go of it, so this tries again for up to about two seconds.
fn remove_file_patiently(path: &Path) -> std::io::Result<bool> {
    let mut attempts = 0;
    loop {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) if attempts < 10 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny `.torrent` file written by hand ("bencode": d = dictionary, l = list, i5e = 5).
    /// `files` are (path parts, length); one file makes a single-file torrent.
    fn torrent(name: &str, files: &[(&[&str], u64)]) -> Vec<u8> {
        let mut info = String::from("d");
        if files.len() == 1 && files[0].0.len() == 1 {
            info += &format!("6:lengthi{}e", files[0].1);
        } else {
            info += "5:filesl";
            for (parts, length) in files {
                info += &format!("d6:lengthi{length}e4:pathl");
                for part in *parts {
                    info += &format!("{}:{part}", part.len());
                }
                info += "ee";
            }
            info += "e";
        }
        info += &format!(
            "4:name{}:{name}12:piece lengthi16384e6:pieces20:AAAAAAAAAAAAAAAAAAAAe",
            name.len()
        );
        format!("d4:info{info}e").into_bytes()
    }

    #[test]
    fn a_multi_file_torrent_owns_its_files_inside_its_folder() {
        let downloads = Path::new("downloads");
        let folder = downloads.join("album");
        let bytes = torrent("album", &[(&["a.mp3"], 3), (&["extras", "b.txt"], 2)]);
        let paths = files_of(&bytes, downloads, &folder).unwrap();
        assert_eq!(
            paths,
            vec![folder.join("a.mp3"), folder.join("extras").join("b.txt")]
        );
    }

    #[test]
    fn a_single_file_torrent_owns_one_file_in_the_download_folder() {
        let downloads = Path::new("downloads");
        let bytes = torrent("linux.iso", &[(&["linux.iso"], 5)]);
        let paths = files_of(&bytes, downloads, downloads).unwrap();
        assert_eq!(paths, vec![downloads.join("linux.iso")]);
    }

    #[test]
    fn a_folder_that_is_not_where_bitdinghy_puts_downloads_is_refused() {
        let bytes = torrent("album", &[(&["a.mp3"], 3), (&["b.mp3"], 2)]);
        let downloads = Path::new("downloads");
        for wrong in [
            PathBuf::from("elsewhere"),
            downloads.join("deeper").join("album"),
            PathBuf::from("."),
            downloads.join(".."),
        ] {
            assert!(
                files_of(&bytes, downloads, &wrong).is_err(),
                "{wrong:?} must be refused"
            );
        }
    }

    #[test]
    fn several_files_without_a_name_are_refused() {
        // The BitTorrent standard requires a name. Without one, the folder would be named after
        // the magnet link's `dn=` (chosen by whoever wrote the link) or after a file.
        let mut bytes = torrent("x", &[(&["a.bin"], 3), (&["b.bin"], 2)]);
        let named = b"4:name1:x";
        let at = bytes
            .windows(named.len())
            .position(|window| window == named)
            .unwrap();
        bytes.splice(at..at + named.len(), *b"4:name0:");
        let downloads = Path::new("downloads");
        let error = files_of(&bytes, downloads, &downloads.join("x")).unwrap_err();
        assert!(error.to_string().contains("no name"), "{error}");
    }

    #[test]
    fn a_damaged_saved_file_list_deletes_nothing() {
        let downloads = Path::new("downloads");
        let error = files_of(b"not a torrent", downloads, downloads).unwrap_err();
        assert!(
            error.to_string().contains("won't delete anything"),
            "{error}"
        );
    }

    #[test]
    fn existing_files_are_refused_and_named() {
        let work = tempfile::tempdir().unwrap();
        let free = work.path().join("free.bin");
        let taken = work.path().join("taken.bin");
        std::fs::write(&taken, b"someone's own file").unwrap();
        assert!(refuse_existing_files(std::slice::from_ref(&free)).is_ok());
        let error = refuse_existing_files(&[free, taken.clone()]).unwrap_err();
        assert!(matches!(&error, Error::NameClash(path) if *path == taken));
        assert!(error.to_string().contains("taken.bin"), "{error}");
    }

    #[test]
    fn deleting_removes_the_files_and_the_folders_they_leave_empty() {
        let work = tempfile::tempdir().unwrap();
        let downloads = work.path().join("downloads");
        let folder = downloads.join("album");
        std::fs::create_dir_all(folder.join("extras")).unwrap();
        let paths = vec![folder.join("a.mp3"), folder.join("extras").join("b.txt")];
        for path in &paths {
            std::fs::write(path, b"x").unwrap();
        }
        let neighbour = downloads.join("someone-elses.txt");
        std::fs::write(&neighbour, b"keep me").unwrap();

        let deleted = delete(&paths, &downloads, &folder);
        assert_eq!(
            deleted,
            Deleted {
                files: 2,
                problems: vec![]
            }
        );
        assert!(!folder.exists(), "the empty torrent folder is removed");
        assert!(downloads.is_dir(), "the download folder itself stays");
        assert!(neighbour.exists(), "other files are never touched");
    }

    #[test]
    fn a_folder_that_still_holds_something_is_left_alone() {
        let work = tempfile::tempdir().unwrap();
        let downloads = work.path().join("downloads");
        let folder = downloads.join("album");
        std::fs::create_dir_all(&folder).unwrap();
        let ours = folder.join("a.mp3");
        std::fs::write(&ours, b"x").unwrap();
        let added_later = folder.join("my notes.txt");
        std::fs::write(&added_later, b"mine").unwrap();

        let deleted = delete(std::slice::from_ref(&ours), &downloads, &folder);
        assert_eq!(deleted.files, 1);
        assert!(added_later.exists());
        assert!(folder.is_dir());
    }

    #[test]
    fn unticked_files_and_folders_holding_only_them_are_hidden() {
        let downloads = Path::new("downloads");
        let folder = downloads.join("album");
        let bytes = torrent(
            "album",
            &[
                (&["a.mp3"], 3),
                (&["extras", "b.txt"], 2),
                (&["extras", "deep", "c.txt"], 2),
                (&["bonus", "d.txt"], 2),
            ],
        );
        // b.txt (index 1) and c.txt (index 2) unticked: "extras" holds only unticked files.
        let marks = leftover_marks(&bytes, downloads, &folder, &BTreeSet::from([1, 2])).unwrap();
        let hidden: BTreeSet<PathBuf> = marks
            .iter()
            .filter(|(_, hide)| *hide)
            .map(|(path, _)| path.clone())
            .collect();
        assert_eq!(
            hidden,
            BTreeSet::from([
                folder.join("extras").join("b.txt"),
                folder.join("extras").join("deep").join("c.txt"),
                folder.join("extras").join("deep"),
                folder.join("extras"),
            ])
        );
        assert_eq!(marks.len(), 4 + 3, "four files and three folders");
        assert!(
            !marks.iter().any(|(path, _)| *path == folder),
            "the download's own folder is never marked"
        );

        // Only c.txt unticked: "extras" holds a ticked file, so it stays visible.
        let marks = leftover_marks(&bytes, downloads, &folder, &BTreeSet::from([2])).unwrap();
        let extras = marks
            .iter()
            .find(|(path, _)| *path == folder.join("extras"));
        assert_eq!(extras, Some(&(folder.join("extras"), false)));
    }

    #[test]
    fn leftover_marks_check_the_folder_like_deleting_does() {
        let bytes = torrent("album", &[(&["a.mp3"], 3), (&["b.mp3"], 2)]);
        let downloads = Path::new("downloads");
        let wrong = downloads.join("deeper").join("album");
        assert!(leftover_marks(&bytes, downloads, &wrong, &BTreeSet::new()).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn hiding_and_showing_really_changes_the_mark() {
        let work = tempfile::tempdir().unwrap();
        let folder = work.path().join("album");
        std::fs::create_dir_all(folder.join("extras")).unwrap();
        let file = folder.join("extras").join("b.txt");
        std::fs::write(&file, b"x").unwrap();
        let missing = folder.join("never-written.bin");
        let extras = folder.join("extras");

        let hide = [
            (file.clone(), true),
            (extras.clone(), true),
            (missing.clone(), true),
        ];
        assert_eq!(apply_hidden(&hide), 0, "a missing file is not a problem");
        assert!(is_hidden(&file) && is_hidden(&extras));
        assert!(!missing.exists(), "nothing is created");
        // Hidden files can still be read and written (the engine keeps writing to them).
        std::fs::write(&file, b"still writable").unwrap();
        assert!(is_hidden(&file), "writing keeps the mark");

        assert_eq!(
            apply_hidden(&[(file.clone(), false), (extras.clone(), false)]),
            0
        );
        assert!(!is_hidden(&file) && !is_hidden(&extras));
    }

    /// Makes `link` point to the folder `target`: a junction on Windows (no special rights
    /// needed), a symbolic link elsewhere.
    fn link_folder(target: &Path, link: &Path) {
        #[cfg(windows)]
        {
            let made = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .expect("cmd runs");
            assert!(made.status.success(), "{made:?}");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    #[test]
    fn a_link_on_the_way_to_a_file_is_refused() {
        let work = tempfile::tempdir().unwrap();
        let downloads = work.path().join("downloads");
        let elsewhere = work.path().join("elsewhere");
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        // A real folder: fine (and so is a path that doesn't exist yet).
        std::fs::create_dir_all(downloads.join("real")).unwrap();
        let fine = [
            downloads.join("real").join("a.bin"),
            downloads.join("new").join("b.bin"),
        ];
        assert!(refuse_links(&downloads, &fine).is_ok());

        // "album" is a link to a folder outside the download folder.
        link_folder(&elsewhere, &downloads.join("album"));
        let error = refuse_links(&downloads, &[downloads.join("album").join("a.mp3")]).unwrap_err();
        assert!(
            matches!(&error, Error::LinkInTheWay(place) if *place == downloads.join("album")),
            "{error}"
        );
        assert!(
            error.to_string().contains("link to another place"),
            "{error}"
        );
    }

    #[test]
    fn the_download_folder_itself_may_be_a_link() {
        let work = tempfile::tempdir().unwrap();
        let real = work.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        let downloads = work.path().join("downloads");
        link_folder(&real, &downloads);
        assert!(refuse_links(&downloads, &[downloads.join("a.bin")]).is_ok());
    }

    #[test]
    fn files_already_gone_are_not_problems() {
        let work = tempfile::tempdir().unwrap();
        let downloads = work.path().to_path_buf();
        let deleted = delete(
            &[downloads.join("never-written.bin")],
            &downloads,
            &downloads,
        );
        assert_eq!(deleted, Deleted::default());
        assert!(downloads.is_dir(), "the download folder is never removed");
    }
}
