//! BitDinghy's remembered settings, kept in `settings.toml` inside the data folder.
//!
//! The rules this file follows (they are the "updates never lose your data" promise):
//!
//! * **Versioned from day one.** The file says which version of BitDinghy's settings format wrote
//!   it. Reading an older file runs it through the migration steps up to the current version.
//! * **Never destroys a damaged file.** If the file can't be understood, it is renamed to
//!   `settings.toml.broken-<time>` (so you can recover from it) and defaults are used.
//! * **Never overwrites a newer file.** If the file was written by a newer BitDinghy, we use
//!   defaults for this run and refuse to save, so going back to an older version can't wipe out
//!   settings it doesn't understand.
//! * **Saves atomically.** We write a temporary file and then swap it into place, so a crash or a
//!   power cut mid-save leaves either the old file or the new one, never half of each.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::data_dir::DataDir;
use crate::error::{Error, Result};

/// The settings format this version of BitDinghy writes.
pub const CURRENT_VERSION: u32 = 1;

/// The file's name inside the data folder.
pub const FILE_NAME: &str = "settings.toml";

/// A settings file bigger than this is certainly not ours, so we don't try to read it.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// What BitDinghy remembers between runs.
///
/// `#[serde(default)]` means a missing setting simply takes its default. That is what lets a file
/// written by an OLDER version (which doesn't know about newer settings) still load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Which settings-format version this is. Always [`CURRENT_VERSION`] in memory.
    pub version: u32,
    /// Where downloads are saved. `None` means the standard folder (see
    /// [`crate::default_download_dir`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_dir: Option<PathBuf>,
    /// Also open plain `http://` links to `.torrent` files (added in M5; off unless turned on).
    /// Secure `https://` links are always allowed. Plain http can be changed on the way, so a
    /// different torrent could be swapped in without anyone noticing.
    #[serde(skip_serializing_if = "is_false")]
    pub allow_http_links: bool,
    /// How many downloads run at the same time; the others wait in the queue (added in M8). 0
    /// means no limit. Downloads that are only sharing don't count.
    #[serde(skip_serializing_if = "is_default_max_active")]
    pub max_active_downloads: u32,
    /// The most all downloads together may download, in KiB per second (added in M9). 0 means no
    /// limit.
    #[serde(skip_serializing_if = "is_zero")]
    pub download_limit_kib: u32,
    /// The most all downloads together may upload (share), in KiB per second. 0 means no limit.
    #[serde(skip_serializing_if = "is_zero")]
    pub upload_limit_kib: u32,
    /// A finished download stops sharing once it has uploaded this share of its size, in percent
    /// (100 = "ratio 1.0": as much as it downloaded). 0 means no such limit (added in M10).
    #[serde(skip_serializing_if = "is_zero")]
    pub stop_sharing_ratio_percent: u32,
    /// A finished download stops sharing after sharing this many minutes in total. 0 means no
    /// such limit (added in M10).
    #[serde(skip_serializing_if = "is_zero")]
    pub stop_sharing_after_minutes: u32,
    /// Accept connections from other people (added in M11; on unless turned off). Downloading
    /// works without it, but sharing works much better with it.
    #[serde(skip_serializing_if = "is_true")]
    pub accept_incoming: bool,
    /// The port other people connect to. 0 means automatic: a different free port at each start.
    #[serde(skip_serializing_if = "is_zero_port")]
    pub listen_port: u16,
    /// Ask the router to forward the port (UPnP), so people outside the home network can connect.
    /// Off unless turned on: BitDinghy doesn't change the router unless asked.
    #[serde(skip_serializing_if = "is_false")]
    pub upnp: bool,
    /// The window's colours (added with the redesigned window): dark (the standard), light, or
    /// like Windows' own light/dark choice.
    #[serde(skip_serializing_if = "Look::is_standard")]
    pub look: Look,
    /// Whether the window has already asked once to open magnet links and `.torrent` files with
    /// BitDinghy (added with the redesigned window). Either answer stops the question.
    #[serde(skip_serializing_if = "is_false")]
    pub asked_about_default_app: bool,
    /// What happens when a download is finished (added with the redesigned window): keep sharing
    /// it (the standard), stop right away, or stop once "Check files" finds it complete.
    #[serde(skip_serializing_if = "WhenFinished::is_standard")]
    pub when_finished: WhenFinished,
}

/// What happens when a download is finished (every piece is checked as it arrives, so a finished
/// download is a verified one).
///
/// Saved as a word, like [`Look`]; a word this version doesn't know reads as the standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum WhenFinished {
    /// It shares with others while BitDinghy runs (until a sharing limit, if one is set).
    #[default]
    KeepSharing,
    /// It stops (is paused) as soon as it is finished.
    StopRightAway,
    /// It keeps sharing, but stops once "Check files" finds every chosen file complete.
    StopAfterCheck,
}

impl WhenFinished {
    fn is_standard(&self) -> bool {
        *self == WhenFinished::KeepSharing
    }

    /// The word used in the settings file and by `dinghy settings set when-finished`.
    pub fn word(self) -> &'static str {
        match self {
            WhenFinished::KeepSharing => "keep-sharing",
            WhenFinished::StopRightAway => "stop",
            WhenFinished::StopAfterCheck => "stop-after-check",
        }
    }

    /// The choice for a word, if it is one of the three.
    pub fn from_word(word: &str) -> Option<WhenFinished> {
        [
            WhenFinished::KeepSharing,
            WhenFinished::StopRightAway,
            WhenFinished::StopAfterCheck,
        ]
        .into_iter()
        .find(|choice| choice.word() == word.trim().to_ascii_lowercase())
    }
}

impl From<String> for WhenFinished {
    fn from(word: String) -> Self {
        WhenFinished::from_word(&word).unwrap_or_default()
    }
}

impl From<WhenFinished> for String {
    fn from(choice: WhenFinished) -> Self {
        choice.word().to_string()
    }
}

/// The window's colours.
///
/// Saved as a word (`"light"`, `"like-windows"`). A word this version doesn't know (written by a
/// newer BitDinghy) reads as the standard dark look instead of making the whole file unreadable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Look {
    /// The brand's main look, and the standard.
    #[default]
    Dark,
    Light,
    /// Follows Windows' own light/dark setting.
    LikeWindows,
}

impl Look {
    fn is_standard(&self) -> bool {
        *self == Look::Dark
    }
}

impl From<String> for Look {
    fn from(word: String) -> Self {
        match word.as_str() {
            "light" => Look::Light,
            "like-windows" => Look::LikeWindows,
            _ => Look::Dark,
        }
    }
}

impl From<Look> for String {
    fn from(look: Look) -> Self {
        match look {
            Look::Dark => "dark",
            Look::Light => "light",
            Look::LikeWindows => "like-windows",
        }
        .to_string()
    }
}

/// The lowest port that can be chosen (lower ones belong to well-known services).
pub const MIN_LISTEN_PORT: u16 = 1024;

fn is_true(value: &bool) -> bool {
    *value
}

fn is_zero_port(value: &u16) -> bool {
    *value == 0
}

/// The highest sharing ratio that can be set, in percent (a ratio of 1000).
pub const MAX_SHARING_RATIO_PERCENT: u32 = 100_000;

/// The longest sharing time that can be set, in minutes (a year).
pub const MAX_SHARING_MINUTES: u32 = 525_600;

/// The lowest speed limit, in KiB per second. librqbit asks for data in blocks of 16 KiB and can't
/// wait for a block bigger than the limit, so anything lower would stop downloads altogether.
pub const MIN_SPEED_LIMIT_KIB: u32 = 32;

/// The highest speed limit that can be set, in KiB per second (about 3.8 GiB/s).
pub const MAX_SPEED_LIMIT_KIB: u32 = 4_000_000;

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// The standard number of downloads that run at the same time.
pub const DEFAULT_MAX_ACTIVE_DOWNLOADS: u32 = 3;

/// The highest number that can be chosen (0 means no limit at all).
pub const MAX_ACTIVE_DOWNLOADS_LIMIT: u32 = 100;

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_default_max_active(value: &u32) -> bool {
    *value == DEFAULT_MAX_ACTIVE_DOWNLOADS
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            download_dir: None,
            allow_http_links: false,
            max_active_downloads: DEFAULT_MAX_ACTIVE_DOWNLOADS,
            download_limit_kib: 0,
            upload_limit_kib: 0,
            stop_sharing_ratio_percent: 0,
            stop_sharing_after_minutes: 0,
            accept_incoming: true,
            listen_port: 0,
            upnp: false,
            look: Look::Dark,
            asked_about_default_app: false,
            when_finished: WhenFinished::KeepSharing,
        }
    }
}

impl Settings {
    /// The folder downloads go to: the chosen one, or the standard one.
    pub fn download_dir_or_default(&self) -> PathBuf {
        self.download_dir
            .clone()
            .unwrap_or_else(crate::default_download_dir)
    }
}

/// Something that went wrong while loading, explained for a person. The program still carries on
/// (with defaults), so this is shown as a notice, not treated as a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The file couldn't be understood. It was kept as `backup` (if that worked).
    Unreadable {
        reason: String,
        backup: Option<PathBuf>,
    },
    /// The file was written by a newer BitDinghy than this one.
    NewerVersion { found: u32 },
}

impl Problem {
    /// A plain-language message.
    pub fn message(&self) -> String {
        match self {
            Problem::Unreadable {
                reason,
                backup: Some(backup),
            } => format!(
                "BitDinghy couldn't read your saved settings ({reason}), so it is using the \
                 standard ones. The old file was kept as {}.",
                backup.display()
            ),
            Problem::Unreadable {
                reason,
                backup: None,
            } => format!(
                "BitDinghy couldn't read your saved settings ({reason}), so it is using the \
                 standard ones for now."
            ),
            Problem::NewerVersion { found } => format!(
                "Your settings were saved by a newer BitDinghy (settings version {found}; this \
                 one understands up to {CURRENT_VERSION}). They are left untouched and the \
                 standard ones are used. Update BitDinghy to use them."
            ),
        }
    }
}

/// The result of loading: usable settings, plus a problem to tell the user about, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub settings: Settings,
    pub problem: Option<Problem>,
}

/// One step that upgrades a settings file from version N to N+1. `MIGRATIONS[0]` upgrades v1 to
/// v2, and so on. There are none yet because v1 is the first version. When the format changes,
/// add a step here and a test with a saved example of the old file.
type Migration = fn(&mut toml::Table);
const MIGRATIONS: &[Migration] = &[];

/// Why a file's text could not be turned into settings.
enum ParseFailure {
    Newer(u32),
    Invalid(String),
}

/// Reads the settings-format version out of a parsed file. A file with no `version` line
/// (for example one written by hand) is treated as version 1.
fn version_of(table: &toml::Table) -> std::result::Result<u32, ParseFailure> {
    match table.get("version") {
        None => Ok(1),
        Some(toml::Value::Integer(n)) if (1..=i64::from(u32::MAX)).contains(n) => Ok(*n as u32),
        Some(_) => Err(ParseFailure::Invalid(
            "the version number is not a positive whole number".to_string(),
        )),
    }
}

/// Turns the text of a settings file into settings, upgrading old formats on the way.
fn parse_and_migrate(
    text: &str,
    migrations: &[Migration],
) -> std::result::Result<Settings, ParseFailure> {
    let mut table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| ParseFailure::Invalid(e.message().to_string()))?;

    let found = version_of(&table)?;
    let newest = migrations.len() as u32 + 1;
    if found > newest {
        return Err(ParseFailure::Newer(found));
    }
    for step in &migrations[(found as usize - 1)..] {
        step(&mut table);
    }
    table.insert(
        "version".to_string(),
        toml::Value::Integer(i64::from(newest)),
    );

    let mut settings: Settings = table
        .try_into()
        .map_err(|e: toml::de::Error| ParseFailure::Invalid(e.message().to_string()))?;
    // An empty path means "not set".
    if settings
        .download_dir
        .as_ref()
        .is_some_and(|p| p.as_os_str().is_empty())
    {
        settings.download_dir = None;
    }
    Ok(settings)
}

/// Reads and writes the settings file.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    /// The settings file inside this data folder.
    pub fn in_dir(data: &DataDir) -> Self {
        Self {
            path: data.file(FILE_NAME),
        }
    }

    /// Where the file is (or will be).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads the settings. This never fails: anything wrong is reported in [`Loaded::problem`]
    /// and defaults are used.
    pub fn load(&self) -> Loaded {
        self.load_with(MIGRATIONS)
    }

    fn load_with(&self, migrations: &[Migration]) -> Loaded {
        let defaults = |problem| Loaded {
            settings: Settings::default(),
            problem,
        };

        let metadata = match std::fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return defaults(None),
            Err(e) => {
                return defaults(Some(Problem::Unreadable {
                    reason: e.to_string(),
                    backup: None,
                }));
            }
        };
        if metadata.len() > MAX_FILE_BYTES {
            return self
                .set_aside("the file is much bigger than a settings file should be".to_string());
        }
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) => {
                return defaults(Some(Problem::Unreadable {
                    reason: e.to_string(),
                    backup: None,
                }));
            }
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return self.set_aside("the file is not plain text".to_string());
        };

        match parse_and_migrate(&text, migrations) {
            Ok(settings) => Loaded {
                settings,
                problem: None,
            },
            Err(ParseFailure::Newer(found)) => defaults(Some(Problem::NewerVersion { found })),
            Err(ParseFailure::Invalid(reason)) => self.set_aside(reason),
        }
    }

    /// Renames a file we can't understand to `settings.toml.broken-<time>` so nothing is lost,
    /// and returns defaults.
    fn set_aside(&self, reason: String) -> Loaded {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut backup_name = self.path.as_os_str().to_owned();
        backup_name.push(format!(".broken-{stamp}"));
        let backup = PathBuf::from(backup_name);
        let backup = std::fs::rename(&self.path, &backup)
            .is_ok()
            .then_some(backup);
        Loaded {
            settings: Settings::default(),
            problem: Some(Problem::Unreadable { reason, backup }),
        }
    }

    /// Saves the settings, replacing the file safely. Refuses if the file on disk was written by a
    /// newer BitDinghy, so it can't destroy settings this version doesn't understand.
    pub fn save(&self, settings: &Settings) -> Result<()> {
        if let Ok(text) = std::fs::read_to_string(&self.path)
            && let Ok(table) = text.parse::<toml::Table>()
            && let Ok(found) = version_of(&table)
            && found > CURRENT_VERSION
        {
            return Err(Error::Settings(Problem::NewerVersion { found }.message()));
        }

        let mut to_write = settings.clone();
        to_write.version = CURRENT_VERSION;
        let body = toml::to_string(&to_write)
            .map_err(|e| Error::Settings(format!("couldn't write your settings: {e}")))?;
        let text = format!(
            "# BitDinghy settings. It is safe to edit this by hand while BitDinghy is closed.\n{body}"
        );

        let io_error = |e: std::io::Error| {
            Error::Settings(format!(
                "couldn't save your settings to {}: {e}",
                self.path.display()
            ))
        };
        let mut temporary_name = self.path.as_os_str().to_owned();
        temporary_name.push(".tmp");
        let temporary = PathBuf::from(temporary_name);
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&temporary).map_err(io_error)?;
            file.write_all(text.as_bytes()).map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
        }
        std::fs::rename(&temporary, &self.path).map_err(io_error)
    }
}

/// Checks that a folder can be used for downloads: it must be (or be creatable as) a folder we can
/// write in. Returns the absolute path to store. Creates the folder, because the person chose it
/// on purpose and it would be created on the first download anyway.
pub fn validate_download_dir(requested: &Path) -> Result<PathBuf> {
    let problem = |reason: String| Error::DownloadDir {
        path: requested.to_path_buf(),
        reason,
    };
    if requested.as_os_str().is_empty() {
        return Err(problem("the folder name is empty".to_string()));
    }
    let absolute = std::path::absolute(requested).map_err(|e| problem(e.to_string()))?;
    if absolute.exists() && !absolute.is_dir() {
        return Err(problem("that is a file, not a folder".to_string()));
    }
    std::fs::create_dir_all(&absolute).map_err(|e| problem(e.to_string()))?;
    let probe = absolute.join(format!(".write-test-{}", std::process::id()));
    std::fs::write(&probe, b"ok").map_err(|e| problem(format!("can't write there: {e}")))?;
    let _ = std::fs::remove_file(&probe);
    Ok(absolute)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &Path) -> SettingsStore {
        SettingsStore {
            path: dir.join(FILE_NAME),
        }
    }

    /// A settings file exactly as version 1 of BitDinghy writes it. KEEP THIS FIXTURE FOREVER:
    /// every future version must still be able to read it. That is the "updates never lose your
    /// data" test.
    const V1_FILE: &str = include_str!("../tests/fixtures/settings-v1.toml");

    #[test]
    fn no_file_means_defaults_and_no_problem() {
        let work = tempfile::tempdir().unwrap();
        let loaded = store_in(work.path()).load();
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.problem, None);
    }

    #[test]
    fn a_version_1_file_still_loads() {
        let work = tempfile::tempdir().unwrap();
        std::fs::write(work.path().join(FILE_NAME), V1_FILE).unwrap();
        let loaded = store_in(work.path()).load();
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.settings.version, CURRENT_VERSION);
        assert_eq!(
            loaded.settings.download_dir,
            Some(PathBuf::from("C:/Games/Downloads"))
        );
    }

    #[test]
    fn what_we_save_is_read_back_exactly() {
        let work = tempfile::tempdir().unwrap();
        let store = store_in(work.path());
        let settings = Settings {
            download_dir: Some(work.path().join("my downloads")),
            ..Settings::default()
        };
        store.save(&settings).unwrap();
        assert_eq!(
            store.load(),
            Loaded {
                settings,
                problem: None
            }
        );
    }

    #[test]
    fn the_number_of_downloads_at_once_is_remembered_and_3_by_default() {
        let work = tempfile::tempdir().unwrap();
        let store = store_in(work.path());
        assert_eq!(store.load().settings.max_active_downloads, 3);
        store.save(&Settings::default()).unwrap();
        let text = std::fs::read_to_string(work.path().join(FILE_NAME)).unwrap();
        assert!(
            !text.contains("max_active_downloads"),
            "the standard isn't written: {text}"
        );
        let settings = Settings {
            max_active_downloads: 0,
            ..Settings::default()
        };
        store.save(&settings).unwrap();
        assert_eq!(store.load().settings.max_active_downloads, 0);
    }

    #[test]
    fn connection_settings_are_remembered_with_safe_defaults() {
        let work = tempfile::tempdir().unwrap();
        let store = store_in(work.path());
        let loaded = store.load().settings;
        assert!(loaded.accept_incoming);
        assert_eq!(loaded.listen_port, 0);
        assert!(!loaded.upnp, "the router is never changed unless asked");
        store.save(&Settings::default()).unwrap();
        let text = std::fs::read_to_string(work.path().join(FILE_NAME)).unwrap();
        for word in ["accept_incoming", "listen_port", "upnp"] {
            assert!(!text.contains(word), "the standard isn't written: {text}");
        }
        let settings = Settings {
            accept_incoming: false,
            listen_port: 51413,
            upnp: true,
            ..Settings::default()
        };
        store.save(&settings).unwrap();
        assert_eq!(store.load().settings, settings);
    }

    #[test]
    fn sharing_limits_are_remembered_and_off_by_default() {
        let work = tempfile::tempdir().unwrap();
        let store = store_in(work.path());
        let loaded = store.load().settings;
        assert_eq!(
            (
                loaded.stop_sharing_ratio_percent,
                loaded.stop_sharing_after_minutes
            ),
            (0, 0)
        );
        let settings = Settings {
            stop_sharing_ratio_percent: 150,
            stop_sharing_after_minutes: 90,
            ..Settings::default()
        };
        store.save(&settings).unwrap();
        assert_eq!(store.load().settings, settings);
    }

    #[test]
    fn speed_limits_are_remembered_and_off_by_default() {
        let work = tempfile::tempdir().unwrap();
        let store = store_in(work.path());
        let loaded = store.load().settings;
        assert_eq!((loaded.download_limit_kib, loaded.upload_limit_kib), (0, 0));
        let settings = Settings {
            download_limit_kib: 500,
            upload_limit_kib: 64,
            ..Settings::default()
        };
        store.save(&settings).unwrap();
        let loaded = store.load().settings;
        assert_eq!(
            (loaded.download_limit_kib, loaded.upload_limit_kib),
            (500, 64)
        );
        store.save(&Settings::default()).unwrap();
        let text = std::fs::read_to_string(work.path().join(FILE_NAME)).unwrap();
        assert!(
            !text.contains("limit_kib"),
            "no limit isn't written: {text}"
        );
    }

    #[test]
    fn allowing_http_links_is_remembered_and_off_by_default() {
        let work = tempfile::tempdir().unwrap();
        let store = store_in(work.path());
        assert!(!store.load().settings.allow_http_links);
        let settings = Settings {
            allow_http_links: true,
            ..Settings::default()
        };
        store.save(&settings).unwrap();
        assert!(store.load().settings.allow_http_links);
        // Off is the standard, so it isn't even written down.
        store.save(&Settings::default()).unwrap();
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert!(!text.contains("allow_http_links"), "{text}");
    }

    #[test]
    fn the_window_colours_and_the_asked_once_flag_are_remembered() {
        let work = tempfile::tempdir().unwrap();
        let store = store_in(work.path());
        let loaded = store.load().settings;
        assert_eq!(loaded.look, Look::Dark, "dark is the standard");
        assert!(!loaded.asked_about_default_app);
        for look in [Look::Light, Look::LikeWindows, Look::Dark] {
            let settings = Settings {
                look,
                asked_about_default_app: true,
                ..Settings::default()
            };
            store.save(&settings).unwrap();
            let loaded = store.load().settings;
            assert_eq!(loaded.look, look);
            assert!(loaded.asked_about_default_app);
        }
        // The standards aren't written down.
        store.save(&Settings::default()).unwrap();
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert!(!text.contains("look"), "{text}");
        assert!(!text.contains("asked_about"), "{text}");
    }

    #[test]
    fn what_happens_when_finished_is_remembered_and_keep_sharing_is_the_standard() {
        let work = tempfile::tempdir().unwrap();
        let store = store_in(work.path());
        assert_eq!(
            store.load().settings.when_finished,
            WhenFinished::KeepSharing
        );
        for choice in [WhenFinished::StopRightAway, WhenFinished::StopAfterCheck] {
            let settings = Settings {
                when_finished: choice,
                ..Settings::default()
            };
            store.save(&settings).unwrap();
            assert_eq!(store.load().settings.when_finished, choice);
            assert_eq!(WhenFinished::from_word(choice.word()), Some(choice));
        }
        assert_eq!(
            WhenFinished::from_word(" STOP "),
            Some(WhenFinished::StopRightAway)
        );
        assert_eq!(WhenFinished::from_word("sometimes"), None);
        store.save(&Settings::default()).unwrap();
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert!(!text.contains("when_finished"), "{text}");
        // A word from a newer version: the standard, and the file still loads.
        std::fs::write(store.path(), "version = 1\nwhen_finished = \"later\"\n").unwrap();
        let loaded = store.load();
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.settings.when_finished, WhenFinished::KeepSharing);
    }

    #[test]
    fn a_look_from_a_newer_version_reads_as_dark_and_keeps_the_other_settings() {
        let work = tempfile::tempdir().unwrap();
        std::fs::write(
            work.path().join(FILE_NAME),
            "version = 1\nlook = \"purple-stripes\"\nupnp = true\n",
        )
        .unwrap();
        let loaded = store_in(work.path()).load();
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.settings.look, Look::Dark);
        assert!(loaded.settings.upnp);
    }

    #[test]
    fn the_saved_file_says_its_version_and_has_a_friendly_header() {
        let work = tempfile::tempdir().unwrap();
        let store = store_in(work.path());
        store.save(&Settings::default()).unwrap();
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert!(text.starts_with("# BitDinghy settings"), "{text}");
        assert!(text.contains("version = 1"), "{text}");
    }

    #[test]
    fn saving_leaves_no_temporary_file_behind() {
        let work = tempfile::tempdir().unwrap();
        let store = store_in(work.path());
        store.save(&Settings::default()).unwrap();
        store.save(&Settings::default()).unwrap();
        let names: Vec<_> = std::fs::read_dir(work.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![FILE_NAME.to_string()]);
    }

    #[test]
    fn a_file_with_no_version_line_is_treated_as_version_1() {
        let work = tempfile::tempdir().unwrap();
        std::fs::write(work.path().join(FILE_NAME), "download_dir = \"D:/Stuff\"\n").unwrap();
        let loaded = store_in(work.path()).load();
        assert_eq!(loaded.problem, None);
        assert_eq!(
            loaded.settings.download_dir,
            Some(PathBuf::from("D:/Stuff"))
        );
    }

    #[test]
    fn settings_this_version_does_not_know_about_are_ignored_not_fatal() {
        let work = tempfile::tempdir().unwrap();
        let text = "version = 1\nfuture_setting = true\n[future_section]\nx = 1\n";
        std::fs::write(work.path().join(FILE_NAME), text).unwrap();
        let loaded = store_in(work.path()).load();
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.settings, Settings::default());
    }

    #[test]
    fn an_empty_download_folder_means_not_set() {
        let work = tempfile::tempdir().unwrap();
        std::fs::write(work.path().join(FILE_NAME), "download_dir = \"\"\n").unwrap();
        assert_eq!(store_in(work.path()).load().settings.download_dir, None);
    }

    #[test]
    fn a_damaged_file_is_kept_aside_and_defaults_are_used() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        std::fs::write(&path, "this is [not valid toml = =").unwrap();
        let loaded = store_in(work.path()).load();
        assert_eq!(loaded.settings, Settings::default());
        let Some(Problem::Unreadable {
            backup: Some(backup),
            ..
        }) = loaded.problem.clone()
        else {
            panic!("expected a kept-aside backup, got {:?}", loaded.problem);
        };
        assert!(!path.exists(), "the damaged file is moved out of the way");
        assert_eq!(
            std::fs::read_to_string(backup).unwrap(),
            "this is [not valid toml = ="
        );
        assert!(loaded.problem.unwrap().message().contains("kept as"));
    }

    #[test]
    fn a_wrongly_typed_setting_counts_as_damaged_too() {
        let work = tempfile::tempdir().unwrap();
        std::fs::write(work.path().join(FILE_NAME), "download_dir = 5\n").unwrap();
        let loaded = store_in(work.path()).load();
        assert!(matches!(loaded.problem, Some(Problem::Unreadable { .. })));
        assert_eq!(loaded.settings, Settings::default());
    }

    #[test]
    fn binary_junk_and_huge_files_are_set_aside() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        std::fs::write(&path, [0xff, 0xfe, 0x00, 0x80]).unwrap();
        assert!(matches!(
            store_in(work.path()).load().problem,
            Some(Problem::Unreadable {
                backup: Some(_),
                ..
            })
        ));
        std::fs::write(&path, vec![b'#'; (MAX_FILE_BYTES + 1) as usize]).unwrap();
        assert!(matches!(
            store_in(work.path()).load().problem,
            Some(Problem::Unreadable {
                backup: Some(_),
                ..
            })
        ));
    }

    #[test]
    fn a_file_from_a_newer_bitdinghy_is_left_alone_and_cannot_be_overwritten() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        let newer = "version = 99\ndownload_dir = \"D:/Newer\"\nbrand_new_setting = 1\n";
        std::fs::write(&path, newer).unwrap();
        let store = store_in(work.path());

        let loaded = store.load();
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.problem, Some(Problem::NewerVersion { found: 99 }));

        let error = store.save(&Settings::default()).unwrap_err();
        assert!(error.to_string().contains("newer BitDinghy"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            newer,
            "file untouched"
        );
    }

    #[test]
    fn a_nonsense_version_number_counts_as_damaged() {
        let work = tempfile::tempdir().unwrap();
        for text in ["version = \"two\"\n", "version = 0\n", "version = -3\n"] {
            std::fs::write(work.path().join(FILE_NAME), text).unwrap();
            assert!(
                matches!(
                    store_in(work.path()).load().problem,
                    Some(Problem::Unreadable { .. })
                ),
                "{text}"
            );
        }
    }

    /// Proves the upgrade machinery works, using a pretend history: v1 had `folder`, v2 renamed
    /// it to `download_dir`.
    #[test]
    fn old_files_are_upgraded_step_by_step() {
        fn v1_to_v2(table: &mut toml::Table) {
            if let Some(old) = table.remove("folder") {
                table.insert("download_dir".to_string(), old);
            }
        }
        let steps: &[Migration] = &[v1_to_v2];

        // A version-1 file is upgraded.
        let settings = parse_and_migrate("version = 1\nfolder = \"D:/Old\"\n", steps)
            .ok()
            .unwrap();
        assert_eq!(settings.download_dir, Some(PathBuf::from("D:/Old")));

        // A version-2 file is already current and is read as is.
        let settings = parse_and_migrate("version = 2\ndownload_dir = \"D:/New\"\n", steps)
            .ok()
            .unwrap();
        assert_eq!(settings.download_dir, Some(PathBuf::from("D:/New")));

        // A version-3 file is newer than these steps understand.
        assert!(matches!(
            parse_and_migrate("version = 3\n", steps),
            Err(ParseFailure::Newer(3))
        ));
    }

    #[test]
    fn download_folders_are_checked_before_they_are_remembered() {
        let work = tempfile::tempdir().unwrap();

        // A new folder is created and returned as an absolute path.
        let chosen = validate_download_dir(&work.path().join("new").join("deeper")).unwrap();
        assert!(chosen.is_absolute() && chosen.is_dir());

        // A file is not a folder.
        let file = work.path().join("a-file");
        std::fs::write(&file, b"x").unwrap();
        let error = validate_download_dir(&file).unwrap_err();
        assert!(
            error.to_string().contains("a file, not a folder"),
            "{error}"
        );

        // Nothing at all.
        assert!(validate_download_dir(Path::new("")).is_err());

        // The probe file is cleaned up.
        assert_eq!(std::fs::read_dir(&chosen).unwrap().count(), 0);
    }
}
