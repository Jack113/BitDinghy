//! Calendar days in local time, for the history's daily totals (M14).
//!
//! A day is counted as a number: days since 1 January 1970, in the computer's own time zone, so
//! "today" starts at local midnight. Windows tells the time zone's offset; elsewhere UTC is used
//! for now.

/// The current local day.
pub fn today() -> i64 {
    day_of(now_seconds())
}

/// The local day of a moment (seconds since 1970, UTC).
pub fn day_of(unix_seconds: u64) -> i64 {
    let local = i64::try_from(unix_seconds).unwrap_or(i64::MAX) + local_offset_seconds();
    local.div_euclid(86_400)
}

/// A day as people write dates: "2026-10-11".
pub fn date_text(day: i64) -> String {
    let (year, month, day) = civil_from_days(day);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The date of a moment (seconds since 1970), in local time.
pub fn date_of(unix_seconds: u64) -> String {
    date_text(day_of(unix_seconds))
}

/// Year, month and day of a day number (Howard Hinnant's well-known algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// How far local time is ahead of UTC, in seconds (for example +3600 at UTC+1).
pub fn local_offset_seconds() -> i64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
        // SAFETY: an all-zero TIME_ZONE_INFORMATION is valid (plain numbers and arrays), and
        // Windows only writes into it.
        let mut zone: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: `zone` is a valid place for the answer.
        let kind = unsafe { GetTimeZoneInformation(&mut zone) };
        // 2 = daylight saving time is in effect now.
        let extra = if kind == 2 {
            zone.DaylightBias
        } else {
            zone.StandardBias
        };
        // Windows counts the bias as minutes BEHIND UTC.
        -i64::from(zone.Bias + extra) * 60
    }
    #[cfg(not(windows))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers_become_dates() {
        assert_eq!(date_text(0), "1970-01-01");
        assert_eq!(date_text(20_737), "2026-10-11");
        assert_eq!(date_text(19_782), "2024-02-29");
        assert_eq!(date_text(-1), "1969-12-31");
    }

    #[test]
    fn a_moment_falls_on_its_local_day() {
        let offset = local_offset_seconds();
        // Midnight UTC on 2026-10-11, moved to local time: the same day, or the one either side.
        let day = day_of(20_737 * 86_400);
        assert!((20_736..=20_738).contains(&day), "{day} (offset {offset})");
        assert!(offset.abs() <= 14 * 3600, "{offset}");
    }
}
