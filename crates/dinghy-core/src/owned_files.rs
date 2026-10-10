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

use std::path::{Path, PathBuf};
use std::time::Duration;

use librqbit::ValidatedTorrentMetaV1Info;

use crate::error::{Error, Result};
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
    let unreadable = |reason: String| {
        Error::Control(format!(
            "BitDinghy couldn't read this download's saved file list ({reason}), so it won't \
             delete anything."
        ))
    };
    if output_folder != download_dir && output_folder.parent() != Some(download_dir) {
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
    paths_in(&info, output_folder)
}

/// Refuses a NEW download if any of its files already exists: it would overwrite (or, when it is
/// removed later, delete) a file BitDinghy didn't download.
pub fn refuse_existing_files(paths: &[PathBuf]) -> Result<()> {
    match paths.iter().find(|path| path.symlink_metadata().is_ok()) {
        Some(existing) => Err(Error::NameClash(existing.clone())),
        None => Ok(()),
    }
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
        ] {
            assert!(
                files_of(&bytes, downloads, &wrong).is_err(),
                "{wrong:?} must be refused"
            );
        }
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
