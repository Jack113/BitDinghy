//! Making BitDinghy the program for magnet links and `.torrent` files (M16).
//!
//! Windows 10 and 11 don't let a program make itself the default: the person chooses in Windows'
//! own Settings ("Default apps"). What a program may do is **register** itself as able to open
//! them, so that it appears in that choice. BitDinghy does exactly that, for the current Windows
//! user only (no administrator needed), and only when the person asks:
//!
//! * `Software\Classes\BitDinghy.Torrent` and `BitDinghy.Magnet`: how to start BitDinghy with a
//!   file or link (`"BitDinghy.exe" "%1"`), and its icon;
//! * `Software\Classes\.torrent\OpenWithProgids`: BitDinghy in the "Open with" list;
//! * `Software\BitDinghy\Capabilities` and `Software\RegisteredApplications`: the entry that makes
//!   BitDinghy appear in Default apps, for `.torrent` and MAGNET.
//!
//! It never writes `Software\Classes\magnet` or the `.torrent` default itself (that would be the
//! silent take-over Windows forbids). [`unregister`] removes exactly these keys and values again.
//! What the person chose in Default apps belongs to Windows; after undoing, Windows asks again.
//!
//! The portable copy can be moved, so [`status`] says which BitDinghy.exe is registered.

// Only Windows registers file types this way; elsewhere these parts are unused (Linux will use
// a .desktop file with the port, see TODO.md).
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::{Path, PathBuf};

use crate::error::Result;

/// Where in the current user's part of the registry everything goes. Tests use a key of their
/// own instead of `Software`, and delete it afterwards.
#[derive(Debug, Clone)]
pub struct Place {
    base: String,
}

impl Place {
    /// The real place (`HKEY_CURRENT_USER\Software`).
    pub fn real() -> Self {
        Self {
            base: "Software".to_string(),
        }
    }

    /// A place for tests (`HKEY_CURRENT_USER\<base>`).
    pub fn for_tests(base: &str) -> Self {
        Self {
            base: base.to_string(),
        }
    }

    fn key(&self, rest: &str) -> String {
        format!("{}\\{rest}", self.base)
    }
}

/// The names BitDinghy registers under.
const TORRENT_PROGID: &str = "BitDinghy.Torrent";
const MAGNET_PROGID: &str = "BitDinghy.Magnet";
const APP_KEY: &str = "BitDinghy";

/// Whether, and which, BitDinghy is registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    NotRegistered,
    Registered { exe: PathBuf },
}

/// Registers `exe` (BitDinghy.exe) as able to open `.torrent` files and magnet links.
pub fn register(place: &Place, exe: &Path) -> Result<()> {
    platform::register(place, exe)
}

/// Removes everything [`register`] wrote. Nothing registered: nothing to do.
pub fn unregister(place: &Place) -> Result<()> {
    platform::unregister(place)
}

/// Whether BitDinghy is registered, and which BitDinghy.exe.
pub fn status(place: &Place) -> Status {
    platform::status(place)
}

/// Opens Windows' Default apps page (on BitDinghy's own entry where Windows supports that).
pub fn open_default_apps() {
    platform::open_default_apps();
}

/// The command Windows runs: `"C:\...\BitDinghy.exe" "%1"`.
fn open_command(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

/// The Default apps link for this Windows build. Windows 11 (build 22000 and later) can open
/// BitDinghy's own entry; Windows 10 doesn't know `registeredAppUser` (found in the big guided
/// test: Explorer then opened a folder instead), so it gets the plain Default apps page.
fn default_apps_link(windows_build: Option<u32>) -> &'static str {
    match windows_build {
        Some(build) if build >= 22000 => "ms-settings:defaultapps?registeredAppUser=BitDinghy",
        _ => "ms-settings:defaultapps",
    }
}

/// The program in an open command (the part in the first quotation marks).
fn exe_in_command(command: &str) -> Option<PathBuf> {
    let rest = command.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(PathBuf::from(&rest[..end]))
}

#[cfg(windows)]
mod platform {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteKeyW, RegDeleteTreeW,
        RegGetValueW, RegSetValueExW,
    };
    use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};

    use super::{
        APP_KEY, MAGNET_PROGID, Place, Status, TORRENT_PROGID, exe_in_command, open_command,
    };
    use crate::error::{Error, Result};

    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain([0])
            .collect()
    }

    fn failed(what: &str, code: u32) -> Error {
        Error::Settings(format!(
            "Windows didn't let BitDinghy change its registration ({what}, error {code})."
        ))
    }

    /// Sets a text value (`None`: the key's own value) under `HKCU\<key>`, creating the key.
    fn set(key: &str, name: Option<&str>, value: &str) -> Result<()> {
        let key_name = wide(key);
        let mut handle: HKEY = std::ptr::null_mut();
        // SAFETY: `key_name` is zero-terminated; `handle` receives the opened key, which is
        // closed below.
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                key_name.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                std::ptr::null(),
                &mut handle,
                std::ptr::null_mut(),
            )
        };
        if status != 0 {
            return Err(failed(key, status));
        }
        let value_name = name.map(wide);
        let data = wide(value);
        // SAFETY: `handle` is open; the name is zero-terminated or null (the key's own value);
        // `data` is a zero-terminated wide text and its size in bytes is given.
        let status = unsafe {
            RegSetValueExW(
                handle,
                value_name
                    .as_ref()
                    .map_or(std::ptr::null(), |name| name.as_ptr()),
                0,
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        // SAFETY: `handle` was opened above and is closed once.
        unsafe { RegCloseKey(handle) };
        if status != 0 {
            return Err(failed(key, status));
        }
        Ok(())
    }

    /// Reads a text value (`None`: the key's own value) under `HKCU\<key>`.
    fn get(key: &str, name: Option<&str>) -> Option<String> {
        let key_name = wide(key);
        let value_name = name.map(wide);
        let mut buffer = vec![0u16; 2048];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: the names are zero-terminated (or null); `buffer` holds `size` bytes and
        // Windows writes at most that many.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key_name.as_ptr(),
                value_name
                    .as_ref()
                    .map_or(std::ptr::null(), |name| name.as_ptr()),
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

    /// Deletes a key and everything in it. Already gone: fine.
    fn delete_tree(key: &str) -> Result<()> {
        let key_name = wide(key);
        // SAFETY: `key_name` is zero-terminated.
        let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key_name.as_ptr()) };
        if status != 0 && status != ERROR_FILE_NOT_FOUND {
            return Err(failed(key, status));
        }
        // RegDeleteTreeW empties the key; this removes the key itself.
        // SAFETY: as above.
        let status = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, key_name.as_ptr()) };
        if status != 0 && status != ERROR_FILE_NOT_FOUND {
            return Err(failed(key, status));
        }
        Ok(())
    }

    /// Deletes one value. Already gone: fine.
    fn delete_value(key: &str, name: &str) -> Result<()> {
        let (key_name, value_name) = (wide(key), wide(name));
        // SAFETY: both names are zero-terminated.
        let status = unsafe {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, key_name.as_ptr(), value_name.as_ptr())
        };
        if status != 0 && status != ERROR_FILE_NOT_FOUND {
            return Err(failed(key, status));
        }
        Ok(())
    }

    /// Deletes a key only if it is empty (it may hold other things). Errors are ignored.
    fn delete_if_empty(key: &str) {
        let key_name = wide(key);
        // SAFETY: zero-terminated; RegDeleteKeyW refuses a key that has sub-keys.
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, key_name.as_ptr()) };
    }

    /// Tells Explorer the associations changed (so icons and "Open with" refresh).
    fn tell_explorer() {
        // SAFETY: a plain notification with no items.
        unsafe {
            SHChangeNotify(
                SHCNE_ASSOCCHANGED as i32,
                SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
    }

    pub fn register(place: &Place, exe: &Path) -> Result<()> {
        let command = open_command(exe);
        let icon = format!("\"{}\",0", exe.display());
        for (progid, description) in [
            (TORRENT_PROGID, "Torrent file (BitDinghy)"),
            (MAGNET_PROGID, "Magnet link (BitDinghy)"),
        ] {
            let progid_key = place.key(&format!("Classes\\{progid}"));
            set(&progid_key, None, description)?;
            set(&format!("{progid_key}\\DefaultIcon"), None, &icon)?;
            set(
                &format!("{progid_key}\\shell\\open\\command"),
                None,
                &command,
            )?;
        }
        set(
            &place.key(&format!("Classes\\{MAGNET_PROGID}")),
            Some("URL Protocol"),
            "",
        )?;
        set(
            &place.key("Classes\\.torrent\\OpenWithProgids"),
            Some(TORRENT_PROGID),
            "",
        )?;
        let capabilities = place.key(&format!("{APP_KEY}\\Capabilities"));
        set(&capabilities, Some("ApplicationName"), "BitDinghy")?;
        set(
            &capabilities,
            Some("ApplicationDescription"),
            "A fast, lightweight download tool for magnet links and .torrent files.",
        )?;
        set(&capabilities, Some("ApplicationIcon"), &icon)?;
        set(
            &format!("{capabilities}\\FileAssociations"),
            Some(".torrent"),
            TORRENT_PROGID,
        )?;
        set(
            &format!("{capabilities}\\URLAssociations"),
            Some("magnet"),
            MAGNET_PROGID,
        )?;
        set(
            &place.key("RegisteredApplications"),
            Some(APP_KEY),
            &capabilities,
        )?;
        tell_explorer();
        Ok(())
    }

    pub fn unregister(place: &Place) -> Result<()> {
        delete_value(&place.key("RegisteredApplications"), APP_KEY)?;
        delete_tree(&place.key(&format!("{APP_KEY}\\Capabilities")))?;
        delete_if_empty(&place.key(APP_KEY));
        delete_value(
            &place.key("Classes\\.torrent\\OpenWithProgids"),
            TORRENT_PROGID,
        )?;
        delete_tree(&place.key(&format!("Classes\\{TORRENT_PROGID}")))?;
        delete_tree(&place.key(&format!("Classes\\{MAGNET_PROGID}")))?;
        tell_explorer();
        Ok(())
    }

    pub fn status(place: &Place) -> Status {
        let command = get(
            &place.key(&format!("Classes\\{TORRENT_PROGID}\\shell\\open\\command")),
            None,
        );
        match command.as_deref().and_then(exe_in_command) {
            Some(exe) if get(&place.key("RegisteredApplications"), Some(APP_KEY)).is_some() => {
                Status::Registered { exe }
            }
            _ => Status::NotRegistered,
        }
    }

    pub fn open_default_apps() {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        // Windows' own "open this link" (Explorer treats a link it doesn't know as a folder).
        let link = wide(super::default_apps_link(crate::crash::windows_build()));
        let operation = wide("open");
        // SAFETY: `operation` and `link` are zero-terminated; the window, parameters and folder
        // may be null. The result only says whether Windows could open it; nothing to clean up.
        unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                link.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    use super::{Place, Status};
    use crate::error::{Error, Result};

    fn not_here() -> Error {
        Error::Settings("This only works on Windows for now.".to_string())
    }

    pub fn register(_place: &Place, _exe: &Path) -> Result<()> {
        Err(not_here())
    }

    pub fn unregister(_place: &Place) -> Result<()> {
        Err(not_here())
    }

    pub fn status(_place: &Place) -> Status {
        Status::NotRegistered
    }

    pub fn open_default_apps() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_10_gets_the_plain_default_apps_page() {
        assert_eq!(default_apps_link(Some(19045)), "ms-settings:defaultapps");
        assert_eq!(default_apps_link(None), "ms-settings:defaultapps");
        assert_eq!(
            default_apps_link(Some(22631)),
            "ms-settings:defaultapps?registeredAppUser=BitDinghy"
        );
    }

    #[test]
    fn the_open_command_quotes_the_program_and_the_link() {
        let exe = Path::new(r"C:\Apps\BitDinghy\BitDinghy.exe");
        let command = open_command(exe);
        assert_eq!(command, r#""C:\Apps\BitDinghy\BitDinghy.exe" "%1""#);
        assert_eq!(exe_in_command(&command), Some(exe.to_path_buf()));
        assert_eq!(exe_in_command("no quotes"), None);
    }

    /// Uses a key of its own under HKEY_CURRENT_USER and deletes it, so the real registration
    /// is never touched.
    #[cfg(windows)]
    #[test]
    fn registering_and_undoing_leave_nothing_behind() {
        let base = format!(
            "Software\\BitDinghy-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let place = Place::for_tests(&base);
        assert_eq!(status(&place), Status::NotRegistered);
        let exe = Path::new(r"C:\Apps\BitDinghy\BitDinghy.exe");
        register(&place, exe).unwrap();
        assert_eq!(
            status(&place),
            Status::Registered {
                exe: exe.to_path_buf()
            }
        );
        // Registering again (for example from a moved copy) just updates it.
        let moved = Path::new(r"D:\BitDinghy\BitDinghy.exe");
        register(&place, moved).unwrap();
        assert_eq!(
            status(&place),
            Status::Registered {
                exe: moved.to_path_buf()
            }
        );
        unregister(&place).unwrap();
        assert_eq!(status(&place), Status::NotRegistered);
        unregister(&place).unwrap(); // Nothing left: still fine.
        // Tidy up the test's own key (what's left is only empty parent keys).
        let _ = std::process::Command::new("reg")
            .args(["delete", &format!("HKCU\\{base}"), "/f"])
            .output();
    }
}
