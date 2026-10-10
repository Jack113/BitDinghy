//! Choosing which files of a torrent to download.
//!
//! Every download starts with all its files chosen. The person can untick files at any time (the
//! window's Files list, or `dinghy files <n> --pick 1,3-5`), while it runs or while it is paused.
//!
//! * **People see numbers, the engine sees indexes.** A torrent's file list can contain "padding"
//!   entries (a BitTorrent detail that is never written to disk). People never see those, so the
//!   files they see are numbered 1, 2, 3... in torrent order, skipping padding. The engine and the
//!   saved list use the torrent's own index of each file, which never changes.
//! * **What is remembered is the files NOT wanted** (`skipped`). An empty set means "everything",
//!   which is also what an older saved list without this information means.
//! * **At least one file stays chosen.** To stop a download completely, pause or remove it.

use std::collections::BTreeSet;
use std::path::PathBuf;

use librqbit::ValidatedTorrentMetaV1Info;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// One real file in a torrent (padding entries are left out).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorrentFile {
    /// The torrent's own index for this file (what the engine uses).
    pub index: usize,
    /// Its path inside the download's folder, as the torrent names it.
    pub path: PathBuf,
    pub size: u64,
}

/// One file as the person sees it in a list with tick boxes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChoice {
    /// Its number in the list, starting at 1 (what `--pick` uses).
    pub number: usize,
    /// Its path inside the download's folder.
    pub path: PathBuf,
    pub size: u64,
    /// How much of it has arrived and been checked, when the engine knows (it doesn't while the
    /// download isn't running).
    pub done_bytes: Option<u64>,
    /// Whether it is ticked (wanted).
    pub chosen: bool,
}

/// The real files in a checked torrent, in torrent order.
pub fn files_in<B: AsRef<[u8]>>(info: &ValidatedTorrentMetaV1Info<B>) -> Vec<TorrentFile> {
    info.iter_file_details()
        .enumerate()
        .filter(|(_, file)| !file.attrs().padding)
        .map(|(index, file)| TorrentFile {
            index,
            path: file.filename.to_pathbuf(),
            size: file.len,
        })
        .collect()
}

/// The real files in a saved file list (a `.torrent` file).
pub fn files_of_torrent(torrent: &[u8]) -> Result<Vec<TorrentFile>> {
    let unreadable = |reason: String| {
        Error::FileChoice(format!(
            "BitDinghy couldn't read this download's file list ({reason})."
        ))
    };
    let parsed = librqbit::torrent_from_bytes(torrent).map_err(|e| unreadable(e.to_string()))?;
    let info = parsed
        .info
        .data
        .validate()
        .map_err(|e| unreadable(e.to_string()))?;
    Ok(files_in(&info))
}

/// The list with tick boxes. `progress` is the engine's bytes done per torrent index, if known.
pub fn choices(
    files: &[TorrentFile],
    skipped: &BTreeSet<usize>,
    progress: Option<&[u64]>,
) -> Vec<FileChoice> {
    files
        .iter()
        .enumerate()
        .map(|(position, file)| FileChoice {
            number: position + 1,
            path: file.path.clone(),
            size: file.size,
            done_bytes: progress.and_then(|done| done.get(file.index).copied()),
            chosen: !skipped.contains(&file.index),
        })
        .collect()
}

/// Turns the numbers the person ticked into the torrent indexes to skip. Refuses an empty choice
/// and numbers that aren't in the list.
pub fn skipped_for(
    files: &[TorrentFile],
    chosen_numbers: &BTreeSet<usize>,
) -> Result<BTreeSet<usize>> {
    if chosen_numbers.is_empty() {
        return Err(Error::FileChoice(
            "Choose at least one file. To stop the whole download, pause or remove it instead."
                .to_string(),
        ));
    }
    if let Some(wrong) = chosen_numbers
        .iter()
        .find(|number| **number == 0 || **number > files.len())
    {
        return Err(Error::FileChoice(format!(
            "There is no file number {wrong}: this download has {} file{}.",
            files.len(),
            if files.len() == 1 { "" } else { "s" }
        )));
    }
    Ok(files
        .iter()
        .enumerate()
        .filter(|(position, _)| !chosen_numbers.contains(&(position + 1)))
        .map(|(_, file)| file.index)
        .collect())
}

/// A skipped set read back from the saved list, made safe: only indexes of real files are kept,
/// and if that would leave no file chosen (a damaged list), everything is chosen again.
pub fn clean_skipped(files: &[TorrentFile], skipped: &BTreeSet<usize>) -> BTreeSet<usize> {
    let kept: BTreeSet<usize> = files
        .iter()
        .map(|file| file.index)
        .filter(|index| skipped.contains(index))
        .collect();
    if kept.len() == files.len() {
        BTreeSet::new()
    } else {
        kept
    }
}

/// What the engine is told to download: `None` means every file.
pub fn only_files(files: &[TorrentFile], skipped: &BTreeSet<usize>) -> Option<Vec<usize>> {
    let skipped = clean_skipped(files, skipped);
    if skipped.is_empty() {
        return None;
    }
    Some(
        files
            .iter()
            .map(|file| file.index)
            .filter(|index| !skipped.contains(index))
            .collect(),
    )
}

/// Reads a choice typed on the command line: `all`, or numbers and ranges such as `1,3-5,8`.
/// `count` is how many files the download has.
pub fn parse_numbers(text: &str, count: usize) -> Result<BTreeSet<usize>> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("all") {
        return Ok((1..=count).collect());
    }
    let not_understood = |part: &str| {
        Error::FileChoice(format!(
            "\"{part}\" isn't a file number. Use numbers and ranges such as 1,3-5 (or the word \
             all); `dinghy files <n>` shows the numbers."
        ))
    };
    let mut numbers = BTreeSet::new();
    for part in text
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        // Any problem names the whole part, so "1-" is quoted as typed.
        let number = |piece: &str| {
            piece
                .trim()
                .parse::<usize>()
                .map_err(|_| not_understood(part))
        };
        match part.split_once('-') {
            Some((first, last)) => {
                let (first, last) = (number(first)?, number(last)?);
                if first > last {
                    return Err(not_understood(part));
                }
                // Checked against the list here, so a huge range like 1-99999999 can't use up
                // memory before it is refused.
                if last > count {
                    return Err(Error::FileChoice(format!(
                        "There is no file number {last}: this download has {count} file{}.",
                        if count == 1 { "" } else { "s" }
                    )));
                }
                numbers.extend(first..=last);
            }
            None => {
                numbers.insert(number(part)?);
            }
        }
    }
    Ok(numbers)
}

/// The skipped set as the saved list keeps it: `"2,5"`. Empty means every file.
pub fn skipped_to_text(skipped: &BTreeSet<usize>) -> String {
    skipped
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Reads the saved list's text back. Anything that isn't a number is ignored (the result is
/// checked against the real file list before use, see [`clean_skipped`]).
pub fn skipped_from_text(text: &str) -> BTreeSet<usize> {
    text.split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three real files with a padding entry (index 1) between the first two.
    fn sample() -> Vec<TorrentFile> {
        vec![
            TorrentFile {
                index: 0,
                path: PathBuf::from("a.bin"),
                size: 100,
            },
            TorrentFile {
                index: 2,
                path: PathBuf::from("sub/b.bin"),
                size: 200,
            },
            TorrentFile {
                index: 3,
                path: PathBuf::from("c.txt"),
                size: 3,
            },
        ]
    }

    fn set(items: &[usize]) -> BTreeSet<usize> {
        items.iter().copied().collect()
    }

    #[test]
    fn people_see_numbers_from_one_and_padding_is_left_out() {
        let shown = choices(&sample(), &set(&[2]), Some(&[100, 0, 50, 0]));
        let numbers: Vec<usize> = shown.iter().map(|c| c.number).collect();
        assert_eq!(numbers, [1, 2, 3]);
        assert!(shown[0].chosen && !shown[1].chosen && shown[2].chosen);
        // Progress is looked up by the torrent's own index, not the number.
        assert_eq!(shown[1].done_bytes, Some(50));
        assert_eq!(choices(&sample(), &set(&[]), None)[0].done_bytes, None);
    }

    #[test]
    fn the_ticked_numbers_become_the_indexes_to_skip() {
        assert_eq!(skipped_for(&sample(), &set(&[1, 3])).unwrap(), set(&[2]));
        assert_eq!(skipped_for(&sample(), &set(&[1, 2, 3])).unwrap(), set(&[]));
        assert_eq!(skipped_for(&sample(), &set(&[2])).unwrap(), set(&[0, 3]));
    }

    #[test]
    fn at_least_one_file_must_stay_chosen() {
        let error = skipped_for(&sample(), &set(&[])).unwrap_err().to_string();
        assert!(error.contains("at least one file"), "{error}");
    }

    #[test]
    fn numbers_outside_the_list_are_refused() {
        for wrong in [0, 4] {
            let error = skipped_for(&sample(), &set(&[1, wrong]))
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(&format!("no file number {wrong}")),
                "{error}"
            );
            assert!(error.contains("3 files"), "{error}");
        }
    }

    #[test]
    fn the_engine_is_told_only_the_wanted_files() {
        assert_eq!(only_files(&sample(), &set(&[])), None);
        assert_eq!(only_files(&sample(), &set(&[2])), Some(vec![0, 3]));
    }

    #[test]
    fn a_damaged_saved_choice_never_leaves_nothing_to_download() {
        // Indexes that aren't real files (padding, or past the end) are dropped...
        assert_eq!(clean_skipped(&sample(), &set(&[1, 2, 99])), set(&[2]));
        // ...and "skip everything" means "skip nothing".
        assert_eq!(clean_skipped(&sample(), &set(&[0, 2, 3])), set(&[]));
        assert_eq!(only_files(&sample(), &set(&[0, 2, 3])), None);
    }

    #[test]
    fn typed_choices_are_read_with_ranges_and_all() {
        assert_eq!(
            parse_numbers("1,3-5, 8", 10).unwrap(),
            set(&[1, 3, 4, 5, 8])
        );
        assert_eq!(parse_numbers("all", 3).unwrap(), set(&[1, 2, 3]));
        assert_eq!(parse_numbers(" 2 ", 3).unwrap(), set(&[2]));
        assert_eq!(parse_numbers("2,,", 3).unwrap(), set(&[2]));
    }

    #[test]
    fn badly_typed_choices_are_refused_with_a_hint() {
        for bad in ["x", "1-", "5-2", "1;2", "-3"] {
            let error = parse_numbers(bad, 10).unwrap_err().to_string();
            assert!(error.contains("isn't a file number"), "{bad}: {error}");
        }
        let error = parse_numbers("1-99999999999", 10).unwrap_err().to_string();
        assert!(error.contains("no file number 99999999999"), "{error}");
    }

    #[test]
    fn the_saved_text_round_trips_and_ignores_junk() {
        assert_eq!(skipped_to_text(&set(&[5, 2])), "2,5");
        assert_eq!(skipped_from_text("2,5"), set(&[2, 5]));
        assert_eq!(skipped_from_text(""), set(&[]));
        assert_eq!(skipped_from_text("2,x,-1, 7"), set(&[2, 7]));
    }
}
