//! What kind of drive a folder is on, and how much room it has (M12).
//!
//! Used to say things in plain words BEFORE a download fails: a FAT32 drive can't hold a file of
//! 4 GB or more, and a nearly full drive will stop a download halfway. Only Windows answers these
//! questions for now; elsewhere they return `None` and nothing is checked.

use std::path::Path;

/// The biggest file a FAT32 drive can hold: 4 GiB minus one byte.
pub const FAT32_MAX_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024 - 1;

/// The drive a folder is on, as people know it ("C:\\"), and its format ("NTFS", "FAT32",
/// "exFAT"...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    pub root: String,
    pub format: String,
}

impl Drive {
    /// Whether files of 4 GB or more can't be stored on it.
    pub fn has_4gb_limit(&self) -> bool {
        self.format.eq_ignore_ascii_case("FAT32") || self.format.eq_ignore_ascii_case("FAT")
    }
}

/// The drive `folder` is on. `folder` must exist.
pub fn drive_of(folder: &Path) -> Option<Drive> {
    platform::drive_of(folder)
}

/// How many bytes can still be written to the drive `folder` is on (for this user).
pub fn free_space(folder: &Path) -> Option<u64> {
    platform::free_space(folder)
}

#[cfg(windows)]
mod platform {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::Path;

    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetVolumeInformationW, GetVolumePathNameW,
    };

    use super::Drive;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain([0]).collect()
    }

    fn text(buffer: &[u16]) -> String {
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        std::ffi::OsString::from_wide(&buffer[..end])
            .to_string_lossy()
            .into_owned()
    }

    pub fn drive_of(folder: &Path) -> Option<Drive> {
        let path = wide(folder);
        let mut root = [0u16; 512];
        // SAFETY: `path` is zero-terminated and `root` is a buffer of the length given.
        if unsafe { GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
            return None;
        }
        let mut format = [0u16; 64];
        // SAFETY: `root` is zero-terminated (written by Windows above); `format` is a buffer of
        // the length given; the other answers aren't wanted (null).
        let ok = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                format.as_mut_ptr(),
                format.len() as u32,
            )
        };
        if ok == 0 {
            return None;
        }
        Some(Drive {
            root: text(&root),
            format: text(&format),
        })
    }

    pub fn free_space(folder: &Path) -> Option<u64> {
        let path = wide(folder);
        let mut available: u64 = 0;
        // SAFETY: `path` is zero-terminated; `available` is a valid place for the answer; the
        // other two answers aren't wanted (null).
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(available)
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    use super::Drive;

    pub fn drive_of(_folder: &Path) -> Option<Drive> {
        None
    }

    pub fn free_space(_folder: &Path) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fat_drives_have_the_4gb_limit_and_others_dont() {
        let drive = |format: &str| Drive {
            root: "E:\\".to_string(),
            format: format.to_string(),
        };
        assert!(drive("FAT32").has_4gb_limit());
        assert!(!drive("NTFS").has_4gb_limit());
        assert!(!drive("exFAT").has_4gb_limit());
        assert_eq!(FAT32_MAX_FILE_BYTES, 4_294_967_295);
    }

    #[cfg(windows)]
    #[test]
    fn the_temp_folder_is_on_a_real_drive_with_room() {
        let folder = std::env::temp_dir();
        let drive = drive_of(&folder).expect("Windows knows the drive");
        assert!(drive.root.ends_with('\\'), "{drive:?}");
        assert!(!drive.format.is_empty(), "{drive:?}");
        assert!(free_space(&folder).expect("Windows knows the space") > 0);
    }
}
