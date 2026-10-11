//! Turning numbers into short, friendly text ("1.5 MiB", "3m 20s").
//!
//! Plain functions with no state, so they are easy to test and identical in the window and the
//! command line.

use std::time::Duration;

/// `1536` becomes `"1.5 KiB"`. Binary units (1 KiB = 1024 bytes), the same as Windows Explorer.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // Start in KiB, then move up a unit while the value would *display* as 1024.0 or more
    // (so 1,048,575 bytes reads "1.0 MiB", not "1024.0 KiB").
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 1;
    while unit < UNITS.len() - 1 && (value * 10.0).round() / 10.0 >= 1024.0 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Bytes per second as text, for example `"12.3 MiB/s"`.
pub fn format_speed(bytes_per_second: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_second))
}

/// Time left as text. `None` means "we can't tell yet" (for example, no speed yet).
pub fn format_eta(eta: Option<Duration>) -> String {
    let Some(eta) = eta else {
        return "unknown".to_string();
    };
    let secs = eta.as_secs();
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours >= 24 {
        format!("{}d {}h", hours / 24, hours % 24)
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

/// After this long without finding the torrent's file list, we tell the user it may be dead.
pub const NOBODY_SHARING_AFTER: Duration = Duration::from_secs(90);

/// The hint shown when a magnet link has found nobody for a while.
pub const NOBODY_SHARING_HINT: &str =
    "No one is sharing this torrent right now. It may be old or dead.";

/// What to say while still looking for the torrent's file list. Used by both front-ends so they
/// always use the same words.
pub fn waiting_text(waited: Duration) -> String {
    let mut text = format!(
        "Looking for people sharing this torrent... {}s",
        waited.as_secs()
    );
    if waited >= NOBODY_SHARING_AFTER {
        text.push_str(" - ");
        text.push_str(NOBODY_SHARING_HINT);
    }
    text
}

/// How far along something is, as a number from 0.0 to 1.0. An unknown total (0) counts as 0.
/// One removed download: "Name (1.4 GiB), finished 2026-10-10, removed 2026-10-11, 200 MiB uploaded".
pub fn history_line(entry: &crate::HistoryEntry) -> String {
    let finished = entry
        .finished_at
        .map(|at| format!("finished {}", crate::days::date_of(at)))
        .unwrap_or_else(|| "not finished".to_string());
    format!(
        "{} ({}), {finished}, removed {}, {} uploaded",
        entry.name,
        format_bytes(entry.size_bytes),
        crate::days::date_of(entry.removed_at),
        format_bytes(entry.uploaded_bytes)
    )
}

/// A sharing ratio: how many times its size a download has uploaded, like "0.42".
pub fn ratio_text(uploaded: u64, size: u64) -> String {
    if size == 0 {
        return "0.00".to_string();
    }
    format!("{:.2}", uploaded as f64 / size as f64)
}

/// What a finished download is doing about sharing, in one sentence (window and `dinghy`).
pub fn sharing_text(download: &crate::DownloadSnapshot) -> String {
    let shared = format!(
        "{} uploaded, ratio {}, for {}",
        format_bytes(download.uploaded_bytes),
        ratio_text(download.uploaded_bytes, download.total_bytes),
        format_eta(Some(std::time::Duration::from_secs(
            download.shared_seconds
        )))
    );
    if download.sharing_stopped {
        format!("Stopped sharing: it reached the sharing limit ({shared}). Resume shares it again.")
    } else {
        format!(
            "Sharing back to {} people ({shared}). Closing BitDinghy stops sharing.",
            download.peers_connected
        )
    }
}

pub fn fraction(done: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    ((done as f64 / total as f64).clamp(0.0, 1.0)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_have_two_decimals() {
        assert_eq!(ratio_text(0, 0), "0.00");
        assert_eq!(ratio_text(500, 1000), "0.50");
        assert_eq!(ratio_text(1500, 1000), "1.50");
    }

    #[test]
    fn small_sizes_are_plain_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn sizes_step_up_through_the_units() {
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.0 GiB");
        assert_eq!(format_bytes(3 * 1024u64.pow(4)), "3.0 TiB");
    }

    #[test]
    fn a_size_just_below_a_unit_does_not_display_as_1024() {
        assert_eq!(format_bytes(1024 * 1024 - 1), "1.0 MiB");
    }

    #[test]
    fn huge_sizes_stay_in_tib() {
        assert!(format_bytes(u64::MAX).ends_with("TiB"));
    }

    #[test]
    fn speed_adds_per_second() {
        assert_eq!(format_speed(2 * 1024 * 1024), "2.0 MiB/s");
    }

    #[test]
    fn eta_reads_naturally() {
        assert_eq!(format_eta(None), "unknown");
        assert_eq!(format_eta(Some(Duration::from_secs(45))), "45s");
        assert_eq!(format_eta(Some(Duration::from_secs(200))), "3m 20s");
        assert_eq!(
            format_eta(Some(Duration::from_secs(3 * 3600 + 5 * 60))),
            "3h 5m"
        );
        assert_eq!(format_eta(Some(Duration::from_secs(50 * 3600))), "2d 2h");
    }

    #[test]
    fn waiting_text_warns_only_after_a_while() {
        let early = waiting_text(Duration::from_secs(12));
        assert!(early.contains("12s"));
        assert!(!early.contains("old or dead"));
        let late = waiting_text(Duration::from_secs(95));
        assert!(late.contains("No one is sharing this torrent right now. It may be old or dead."));
    }

    #[test]
    fn fraction_handles_zero_and_overshoot() {
        assert_eq!(fraction(5, 0), 0.0);
        assert_eq!(fraction(50, 100), 0.5);
        assert_eq!(fraction(200, 100), 1.0);
    }
}
