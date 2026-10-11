//! Crash reports (M13).
//!
//! If BitDinghy or `dinghy` crashes (a "panic" in Rust: the program finds itself in a state it
//! can't carry on from), [`install`] makes it write a short report first: the version, the
//! Windows version, what went wrong and where (a "backtrace"). The report is saved in the data
//! folder, in `crash-reports`, and **never leaves the PC**: the person decides whether to send it
//! to anyone. The next time the window opens it says that a report was saved
//! ([`unseen_reports`], [`mark_seen`]). Only the newest [`KEEP`] reports are kept.
//!
//! Crashes outside Rust's control (an "access violation" in a library) aren't caught yet; see
//! `TODO.md`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The folder inside the data folder.
pub const FOLDER: &str = "crash-reports";

/// How many reports are kept (the oldest go first).
pub const KEEP: usize = 10;

/// Set this environment variable (to anything) to make BitDinghy or `dinghy` crash on purpose, to
/// try out the crash report.
pub const TEST_VARIABLE: &str = "BITDINGHY_CRASH_TEST";

/// What a report says before the technical part.
const HEADER: &str = "This report stays on your computer. It may contain folder names (such as \
your user name). If you send it to anyone to help find the problem, read it first.";

/// Makes this program write a crash report if it crashes. `program` is "BitDinghy" or "dinghy";
/// `data_dir` is where reports go (`None`: the temp folder). Rust's own message still appears too.
pub fn install(program: &'static str, data_dir: Option<PathBuf>) {
    let folder = report_folder(data_dir.as_deref());
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let thread = std::thread::current();
        let text = report_text(
            program,
            thread.name().unwrap_or("unnamed"),
            &info.to_string(),
            &backtrace.to_string(),
            now_seconds(),
        );
        match write_report(&folder, &text) {
            Ok(path) => eprintln!(
                "{program} ran into a problem it can't carry on from. A report was saved in {} \
                 (it stays on this computer).",
                path.display()
            ),
            Err(error) => eprintln!("{program} couldn't save a crash report: {error}"),
        }
        previous(info);
    }));
}

/// Crashes on purpose if [`TEST_VARIABLE`] is set, to try out the crash report.
pub fn crash_if_testing() {
    if std::env::var_os(TEST_VARIABLE).is_some() {
        panic!("a crash on purpose, because {TEST_VARIABLE} is set (to try out crash reports)");
    }
}

/// Where reports are kept.
pub fn report_folder(data_dir: Option<&Path>) -> PathBuf {
    match data_dir {
        Some(dir) => dir.join(FOLDER),
        None => std::env::temp_dir().join("BitDinghy").join(FOLDER),
    }
}

/// The report itself.
fn report_text(program: &str, thread: &str, problem: &str, backtrace: &str, when: u64) -> String {
    format!(
        "{program} crash report\n\
         {HEADER}\n\n\
         Program: {program} {version}\n\
         System: {os} ({arch})\n\
         When: {when} (seconds since 1970)\n\
         Thread: {thread}\n\
         Problem: {problem}\n\n\
         Where (backtrace):\n{backtrace}\n",
        version = crate::VERSION,
        os = os_version(),
        arch = std::env::consts::ARCH,
    )
}

/// Saves a report as `crash-<time>-<n>.txt` and keeps only the newest [`KEEP`].
fn write_report(folder: &Path, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let when = now_seconds();
    // Several in the same second are numbered on from the highest number used (a lower free
    // number would sort as an old report).
    let number = all_reports(folder)
        .iter()
        .map(|path| report_order(path))
        .filter(|(time, _)| *time == when)
        .map(|(_, number)| number + 1)
        .max()
        .unwrap_or(0);
    let path = folder.join(format!("crash-{when}-{number}.txt"));
    std::fs::write(&path, text)?;
    remove_old_reports(folder);
    Ok(path)
}

/// Every report in the folder (seen or not), oldest first.
fn all_reports(folder: &Path) -> Vec<PathBuf> {
    let mut reports: Vec<PathBuf> = std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| is_report(path))
                .collect()
        })
        .unwrap_or_default();
    reports.sort_by_key(|path| report_order(path));
    reports
}

/// Only files BitDinghy wrote: `crash-<time>-<n>.txt` (or `.seen.txt`).
fn is_report(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("crash-") && name.ends_with(".txt"))
}

/// (time, number) from the file name, for sorting.
fn report_order(path: &Path) -> (u64, u64) {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let mut parts = name
        .trim_start_matches("crash-")
        .split(['-', '.'])
        .map(|part| part.parse::<u64>().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

fn remove_old_reports(folder: &Path) {
    let reports = all_reports(folder);
    if reports.len() > KEEP {
        for old in &reports[..reports.len() - KEEP] {
            let _ = std::fs::remove_file(old);
        }
    }
}

/// Reports the person hasn't been told about yet, oldest first.
pub fn unseen_reports(data_dir: Option<&Path>) -> Vec<PathBuf> {
    all_reports(&report_folder(data_dir))
        .into_iter()
        .filter(|path| !path.to_string_lossy().ends_with(".seen.txt"))
        .collect()
}

/// Remembers that the person was told about these reports (they are renamed `....seen.txt`).
pub fn mark_seen(reports: &[PathBuf]) {
    for report in reports {
        let seen = report.with_extension("seen.txt");
        let _ = std::fs::rename(report, seen);
    }
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The operating system and its version ("Windows 10 Pro 22H2, build 19045"), read from the
/// registry where Windows keeps it.
fn os_version() -> String {
    #[cfg(windows)]
    {
        let key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        let read = |name: &str| windows_registry_text(key, name);
        if let Some(product) = read("ProductName") {
            let release = read("DisplayVersion")
                .map(|v| format!(" {v}"))
                .unwrap_or_default();
            let build = read("CurrentBuildNumber")
                .map(|b| format!(", build {b}"))
                .unwrap_or_default();
            return format!("{product}{release}{build}");
        }
    }
    std::env::consts::OS.to_string()
}

/// A text value under `HKEY_LOCAL_MACHINE\<key>`, if it can be read.
#[cfg(windows)]
fn windows_registry_text(key: &str, name: &str) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};

    let wide = |text: &str| -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain([0])
            .collect()
    };
    let (key, name) = (wide(key), wide(name));
    let mut buffer = [0u16; 256];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: `key` and `name` are zero-terminated; `buffer` holds `size` bytes, and Windows
    // writes at most that many (with RRF_RT_REG_SZ, a zero-terminated text).
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_says_what_and_where_and_that_it_stays_here() {
        let text = report_text(
            "dinghy",
            "main",
            "index out of bounds",
            "0: here\n1: there",
            42,
        );
        assert!(text.starts_with("dinghy crash report\n"), "{text}");
        assert!(text.contains("stays on your computer"), "{text}");
        assert!(
            text.contains(&format!("Program: dinghy {}", crate::VERSION)),
            "{text}"
        );
        assert!(text.contains("Problem: index out of bounds"), "{text}");
        assert!(text.contains("0: here\n1: there"), "{text}");
    }

    #[test]
    fn reports_are_saved_and_only_the_newest_ten_are_kept() {
        let work = tempfile::tempdir().unwrap();
        let folder = work.path().join(FOLDER);
        let mut paths = Vec::new();
        for n in 0..12 {
            paths.push(write_report(&folder, &format!("report {n}")).unwrap());
        }
        let kept = all_reports(&folder);
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept.last(), paths.last(), "the newest is kept");
        assert!(
            !paths[0].exists() && !paths[1].exists(),
            "the two oldest went"
        );
        // Other files in the folder are never touched.
        std::fs::write(folder.join("notes.txt"), "mine").unwrap();
        write_report(&folder, "one more").unwrap();
        assert!(folder.join("notes.txt").exists());
    }

    #[test]
    fn the_window_is_told_once_about_new_reports() {
        let work = tempfile::tempdir().unwrap();
        let data = work.path();
        assert!(unseen_reports(Some(data)).is_empty());
        write_report(&report_folder(Some(data)), "first").unwrap();
        write_report(&report_folder(Some(data)), "second").unwrap();
        let unseen = unseen_reports(Some(data));
        assert_eq!(unseen.len(), 2);
        mark_seen(&unseen);
        assert!(unseen_reports(Some(data)).is_empty());
        // Still kept (and counted for "keep the newest ten").
        assert_eq!(all_reports(&report_folder(Some(data))).len(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn the_windows_version_is_read() {
        let version = os_version();
        assert!(version.starts_with("Windows"), "{version}");
        assert!(version.contains("build"), "{version}");
    }

    #[test]
    fn reports_sort_by_time_then_number() {
        let order = |name: &str| report_order(Path::new(name));
        assert!(order("crash-100-0.txt") < order("crash-100-1.txt"));
        assert!(order("crash-100-9.seen.txt") < order("crash-200-0.txt"));
    }
}
