//! Recommended settings, learned from real downloads (M15).
//!
//! There is no speed-test server (that would break the privacy promise). Instead the engine
//! remembers the fastest download and upload speeds it has seen, as averages over 10 seconds, and
//! only while no limit was set in that direction (a limited speed says nothing about the line).
//! From those, [`recommend`] suggests:
//!
//! * an **upload limit** of 80% of the fastest upload: a completely full upload slows everything
//!   on the line, downloads included, because the line also carries the replies to them;
//! * a **download limit** of 90% of the fastest download, so browsing and calls still work while
//!   BitDinghy downloads;
//! * how many **downloads at the same time** suit the line: fewer on a slow line, so each one
//!   finishes sooner.
//!
//! Nothing changes until the person presses "Use these" (or runs `dinghy recommend --apply`).

use crate::settings::MIN_SPEED_LIMIT_KIB;

/// Names under which the saved list keeps the fastest speeds seen (bytes per second).
pub const FASTEST_DOWNLOAD: &str = "fastest_download";
pub const FASTEST_UPLOAD: &str = "fastest_upload";

/// Speeds below these aren't enough to learn from (a download that barely ran).
pub const ENOUGH_DOWNLOAD: u64 = 64 * 1024;
pub const ENOUGH_UPLOAD: u64 = 32 * 1024;

/// The fastest speeds seen, in bytes per second.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Fastest {
    pub download: Option<u64>,
    pub upload: Option<u64>,
}

/// What BitDinghy recommends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recommended {
    /// KiB/s; 0 = no limit (nothing learned for that direction yet).
    pub download_limit_kib: u32,
    pub upload_limit_kib: u32,
    pub max_active_downloads: u32,
}

/// The recommendation, once something was learned.
pub fn recommend(fastest: Fastest) -> Option<Recommended> {
    let download = fastest.download.filter(|speed| *speed >= ENOUGH_DOWNLOAD);
    let upload = fastest.upload.filter(|speed| *speed >= ENOUGH_UPLOAD);
    if download.is_none() && upload.is_none() {
        return None;
    }
    let share_kib = |speed: Option<u64>, percent: u64| match speed {
        None => 0,
        Some(speed) => {
            let kib = speed * percent / 100 / 1024;
            u32::try_from(kib)
                .unwrap_or(u32::MAX)
                .max(MIN_SPEED_LIMIT_KIB)
        }
    };
    let max_active_downloads = match download {
        Some(speed) if speed < 2 * 1024 * 1024 => 2,
        Some(speed) if speed >= 10 * 1024 * 1024 => 5,
        _ => 3,
    };
    Some(Recommended {
        download_limit_kib: share_kib(download, 90),
        upload_limit_kib: share_kib(upload, 80),
        max_active_downloads,
    })
}

/// "Fastest seen: downloading 9.5 MiB/s, uploading 1.2 MiB/s (10-second averages)."
pub fn fastest_text(fastest: Fastest) -> String {
    let speed = |seen: Option<u64>| match seen {
        Some(speed) => crate::display::format_speed(speed),
        None => "not seen yet".to_string(),
    };
    format!(
        "Fastest seen: downloading {}, uploading {} (10-second averages, without limits).",
        speed(fastest.download),
        speed(fastest.upload)
    )
}

/// What to say before anything was learned.
pub const NOTHING_LEARNED: &str = "BitDinghy learns recommended settings from your own downloads \
(it never runs a speed test). Download something for a minute or two, without speed limits, then \
look again.";

/// Why the recommendation is what it is.
pub const REASONS: &str = "90% of the fastest download leaves room for browsing and calls; 80% \
of the fastest upload keeps the line responsive (a full upload slows everything, downloads \
included).";

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn nothing_is_recommended_before_anything_was_learned() {
        assert_eq!(recommend(Fastest::default()), None);
        let barely = Fastest {
            download: Some(10_000),
            upload: Some(1_000),
        };
        assert_eq!(recommend(barely), None, "too little to learn from");
    }

    #[test]
    fn limits_leave_room_and_the_queue_suits_the_line() {
        let fastest = Fastest {
            download: Some(5 * MIB),
            upload: Some(MIB),
        };
        assert_eq!(
            recommend(fastest),
            Some(Recommended {
                download_limit_kib: 4608,
                upload_limit_kib: 819,
                max_active_downloads: 3,
            })
        );
        let slow = recommend(Fastest {
            download: Some(MIB),
            upload: None,
        })
        .unwrap();
        assert_eq!(slow.max_active_downloads, 2);
        assert_eq!(
            slow.upload_limit_kib, 0,
            "nothing learned about uploads yet"
        );
        let fast = recommend(Fastest {
            download: Some(50 * MIB),
            upload: Some(40 * 1024),
        })
        .unwrap();
        assert_eq!(fast.max_active_downloads, 5);
        assert_eq!(
            fast.upload_limit_kib, MIN_SPEED_LIMIT_KIB,
            "never below what works"
        );
    }
}
