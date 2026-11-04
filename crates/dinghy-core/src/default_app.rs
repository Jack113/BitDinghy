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
//! The portable copy can be moved, so [`status`] says which BitDinghy.exe is registered, and
//! [`opener`] says which program Windows really uses for each (it only reads; the person's choice
//! in Default apps is Windows' own, kept under a protected `UserChoice` key).

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
    /// Also look in the whole computer's file types (`HKEY_CLASSES_ROOT`) when reading which
    /// program opens something. Only the real place does; tests must not see the PC's own.
    whole_computer: bool,
}

impl Place {
    /// The real place (`HKEY_CURRENT_USER\Software`).
    pub fn real() -> Self {
        Self {
            base: "Software".to_string(),
            whole_computer: true,
        }
    }

    /// A place for tests (`HKEY_CURRENT_USER\<base>`).
    pub fn for_tests(base: &str) -> Self {
        Self {
            base: base.to_string(),
            whole_computer: false,
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

/// The two kinds of thing BitDinghy can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    MagnetLinks,
    TorrentFiles,
}

/// Which program Windows opens magnet links or `.torrent` files with, as chosen in Default apps
/// (or with "Open with" and "Always").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opener {
    /// Nothing chosen for this Windows user (Windows asks, or uses a computer-wide program).
    NotChosen,
    /// A BitDinghy.exe (`None`: Windows' entry for it doesn't say where it is).
    BitDinghy { exe: Option<PathBuf> },
    /// Another program, by its file name (`qbittorrent.exe`) or Windows' name for it.
    Other { name: String },
}

/// Which program Windows opens `kind` with for this user. It only reads.
pub fn opener(place: &Place, kind: Kind) -> Opener {
    platform::opener(place, kind)
}

/// Where Windows keeps the person's choice for `kind`, under the place's base key.
fn user_choice_key(kind: Kind) -> &'static str {
    match kind {
        Kind::MagnetLinks => {
            r"Microsoft\Windows\Shell\Associations\UrlAssociations\magnet\UserChoice"
        }
        Kind::TorrentFiles => {
            r"Microsoft\Windows\CurrentVersion\Explorer\FileExts\.torrent\UserChoice"
        }
    }
}

/// What a chosen program id (`BitDinghy.Magnet`, `Applications\BitDinghy.exe`, ...) and its open
/// command (if one was found) mean.
fn opener_from(progid: &str, command: Option<&str>) -> Opener {
    let exe = command.and_then(exe_in_any_command);
    let named = exe
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .or_else(|| progid.strip_prefix(r"Applications\").map(str::to_string));
    let is_bitdinghy = progid == TORRENT_PROGID
        || progid == MAGNET_PROGID
        || named
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("BitDinghy.exe"));
    if is_bitdinghy {
        Opener::BitDinghy { exe }
    } else {
        Opener::Other {
            name: named.unwrap_or_else(|| progid.to_string()),
        }
    }
}

/// The program in an open command, quoted (`"C:\x\a.exe" "%1"`) or not (`C:\x\a.exe %1`).
fn exe_in_any_command(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    if command.starts_with('"') {
        return exe_in_command(command);
    }
    let end = command.to_ascii_lowercase().find(".exe")? + ".exe".len();
    Some(PathBuf::from(&command[..end]))
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
        HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
        RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteKeyW,
        RegDeleteTreeW, RegGetValueW, RegSetValueExW,
    };
    use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};

    use super::{
        APP_KEY, Kind, MAGNET_PROGID, Opener, Place, Status, TORRENT_PROGID, exe_in_command,
        open_command, opener_from, user_choice_key,
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
        get_in(HKEY_CURRENT_USER, key, name)
    }

    /// Reads a text value under `root\<key>`.
    fn get_in(root: HKEY, key: &str, name: Option<&str>) -> Option<String> {
        let key_name = wide(key);
        let value_name = name.map(wide);
        let mut buffer = vec![0u16; 2048];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: the names are zero-terminated (or null); `buffer` holds `size` bytes and
        // Windows writes at most that many.
        let status = unsafe {
            RegGetValueW(
                root,
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

    pub fn opener(place: &Place, kind: Kind) -> Opener {
        let progid = get(&place.key(user_choice_key(kind)), Some("ProgId")).unwrap_or_default();
        if progid.is_empty() {
            return Opener::NotChosen;
        }
        let command_key = format!(r"{progid}\shell\open\command");
        // The person's own file types first, then (real place only) the whole computer's.
        let command = get(&place.key(&format!(r"Classes\{command_key}")), None).or_else(|| {
            place
                .whole_computer
                .then(|| get_in(HKEY_CLASSES_ROOT, &command_key, None))
                .flatten()
        });
        opener_from(&progid, command.as_deref())
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

    use super::{Kind, Opener, Place, Status};
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

    pub fn opener(_place: &Place, _kind: Kind) -> Opener {
        Opener::NotChosen
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

    #[test]
    fn the_chosen_program_is_recognised() {
        let ours = Path::new(r"C:\cargo-target\debug\BitDinghy.exe").to_path_buf();
        assert_eq!(
            opener_from(
                "BitDinghy.Magnet",
                Some(r#""C:\cargo-target\debug\BitDinghy.exe" "%1""#)
            ),
            Opener::BitDinghy {
                exe: Some(ours.clone())
            }
        );
        // Chosen with "Open with": Windows' own entry, named after the program file.
        assert_eq!(
            opener_from(
                r"Applications\BitDinghy.exe",
                Some(r#""C:\cargo-target\debug\BitDinghy.exe" "%1""#)
            ),
            Opener::BitDinghy { exe: Some(ours) }
        );
        assert_eq!(
            opener_from(r"Applications\bitdinghy.EXE", None),
            Opener::BitDinghy { exe: None }
        );
        assert_eq!(
            opener_from(
                "qBittorrent.File.Torrent",
                Some(r#""C:\Program Files\qBittorrent\qbittorrent.exe" "%1""#)
            ),
            Opener::Other {
                name: "qbittorrent.exe".to_string()
            }
        );
        // A Store app has no open command: Windows' own name for it is all there is.
        assert_eq!(
            opener_from("AppXabc123", None),
            Opener::Other {
                name: "AppXabc123".to_string()
            }
        );
    }

    #[test]
    fn the_program_is_found_in_quoted_and_unquoted_commands() {
        assert_eq!(
            exe_in_any_command(r"C:\Tools\Old.EXE %1"),
            Some(PathBuf::from(r"C:\Tools\Old.EXE"))
        );
        assert_eq!(
            exe_in_any_command(r#""C:\A B\x.exe" "%1""#),
            Some(PathBuf::from(r"C:\A B\x.exe"))
        );
        assert_eq!(exe_in_any_command("rundll32 something"), None);
    }

    /// Writes a pretend "UserChoice" under a key of its own (the real one is never touched) and
    /// reads it back like Windows' own.
    #[cfg(windows)]
    #[test]
    fn what_windows_opens_is_read_from_its_user_choice() {
        let base = format!(
            "Software\\BitDinghy-test-choice-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let place = Place::for_tests(&base);
        assert_eq!(opener(&place, Kind::MagnetLinks), Opener::NotChosen);
        let exe = Path::new(r"C:\Apps\BitDinghy\BitDinghy.exe");
        register(&place, exe).unwrap();
        let reg_add = |key: &str, value: &str| {
            let done = std::process::Command::new("reg")
                .args(["add", &format!("HKCU\\{base}\\{key}"), "/v", "ProgId"])
                .args(["/d", value, "/f"])
                .output()
                .unwrap();
            assert!(done.status.success(), "{done:?}");
        };
        reg_add(user_choice_key(Kind::MagnetLinks), MAGNET_PROGID);
        reg_add(user_choice_key(Kind::TorrentFiles), "Some.Other.Program");
        assert_eq!(
            opener(&place, Kind::MagnetLinks),
            Opener::BitDinghy {
                exe: Some(exe.to_path_buf())
            }
        );
        assert_eq!(
            opener(&place, Kind::TorrentFiles),
            Opener::Other {
                name: "Some.Other.Program".to_string()
            }
        );
        let _ = std::process::Command::new("reg")
            .args(["delete", &format!("HKCU\\{base}"), "/f"])
            .output();
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
