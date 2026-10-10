//! The saved download list: what BitDinghy remembers about each download between runs, kept in
//! `downloads.db` (an SQLite database file) inside the data folder.
//!
//! It keeps the same promises as the settings file (see `settings.rs`):
//!
//! * **Versioned from day one.** SQLite keeps a version number inside the file
//!   (`PRAGMA user_version`). An older list is upgraded step by step; a list written by a newer
//!   BitDinghy is never changed.
//! * **Never destroys a damaged file.** A file that isn't a healthy BitDinghy list is renamed to
//!   `downloads.db.broken-<time>`, and a fresh, empty list is started.
//! * **Safe against crashes.** SQLite writes each change completely or not at all, even if the
//!   power goes off halfway.
//!
//! **Only one program may change the list at a time.** Opening it for changes takes a lock on
//! `downloads.db.lock`, held until the list is closed. Windows (and Linux) release the lock
//! automatically when the program ends, even after a crash. A second BitDinghy that tries gets
//! [`Error::AlreadyRunning`]. Just reading the list ([`SavedList::read`]) needs no lock.

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::file_choice;

/// The file's name inside the data folder.
pub const FILE_NAME: &str = "downloads.db";

/// The list format this version of BitDinghy writes.
pub const CURRENT_VERSION: u32 = 2;

/// Marks the file as BitDinghy's (the letters "BDdy"), so another program's SQLite file is never
/// mistaken for our list.
const APPLICATION_ID: i32 = 0x4244_6479;

/// Version 1 of the list's layout. **Never change this text**: a later version adds a migration
/// step instead (see [`MIGRATIONS`]), so lists saved by older versions keep loading.
const SCHEMA_V1: &str = "
CREATE TABLE downloads (
    info_hash     TEXT PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL,
    magnet        TEXT NOT NULL,
    torrent       BLOB,
    download_dir  TEXT NOT NULL,
    output_folder TEXT,
    paused        INTEGER NOT NULL DEFAULT 0,
    added_at      INTEGER NOT NULL
);";

/// One step that upgrades the list from version N to N+1. `MIGRATIONS[0]` upgrades v1 to v2, and
/// so on. When the layout changes, add a step here, and a saved example of the newest list to
/// `tests/fixtures/` (like `downloads-v1.sql` and `downloads-v2.sql`) with a test that loads it.
/// **Never change a step that has shipped**: lists out there have already been upgraded by it.
type Migration = fn(&rusqlite::Transaction) -> rusqlite::Result<()>;
const MIGRATIONS: &[Migration] = &[v1_to_v2];

/// Version 2 (M6, choosing files): remembers the files the person unticked, as the torrent's file
/// indexes ("2,5"). Empty means every file, which is what every download from version 1 wants.
fn v1_to_v2(transaction: &rusqlite::Transaction) -> rusqlite::Result<()> {
    transaction
        .execute_batch("ALTER TABLE downloads ADD COLUMN skipped_files TEXT NOT NULL DEFAULT ''")
}

/// The columns, in the order [`row_to_download`] reads them.
const COLUMNS: &str = "info_hash, name, magnet, torrent, download_dir, output_folder, paused, \
                       added_at, skipped_files";

/// The same columns for a version 1 list (read without upgrading it, by `dinghy list`): every
/// file was wanted then.
const COLUMNS_V1: &str = "info_hash, name, magnet, torrent, download_dir, output_folder, paused, \
                          added_at, '' AS skipped_files";

/// One remembered download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedDownload {
    /// The torrent's fingerprint (40 lowercase hex characters).
    pub info_hash: String,
    /// The name shown in the list.
    pub name: String,
    /// The cleaned-up magnet link it was added from.
    pub magnet: String,
    /// The torrent's file list (as a `.torrent` file), saved once it has passed the safety check.
    /// `None` while BitDinghy is still asking other people for it.
    pub torrent: Option<Vec<u8>>,
    /// The download folder that was chosen when it was added.
    pub download_dir: PathBuf,
    /// The exact folder its files are saved in (inside `download_dir`), once known.
    pub output_folder: Option<PathBuf>,
    /// Whether the person paused it.
    pub paused: bool,
    /// When it was added, in seconds since 1970.
    pub added_at: u64,
    /// The files the person unticked, as the torrent's own file indexes (see `file_choice.rs`).
    /// Empty means every file.
    pub skipped_files: BTreeSet<usize>,
}

impl SavedDownload {
    /// A download that was just added from a magnet link: nothing known about its files yet.
    pub fn new(info_hash: &str, name: &str, magnet: &str, download_dir: &Path) -> Self {
        Self {
            info_hash: info_hash.to_string(),
            name: name.to_string(),
            magnet: magnet.to_string(),
            torrent: None,
            download_dir: download_dir.to_path_buf(),
            output_folder: None,
            paused: false,
            added_at: now_seconds(),
            skipped_files: BTreeSet::new(),
        }
    }
}

/// Something that went wrong while opening the list, explained for a person. BitDinghy still
/// works, so this is shown as a notice, not treated as a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The file couldn't be used. It was kept as `backup` (if that worked).
    Unreadable {
        reason: String,
        backup: Option<PathBuf>,
    },
    /// The list was written by a newer BitDinghy than this one.
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
                "BitDinghy couldn't read your saved download list ({reason}), so it started a \
                 new, empty one. The old file was kept as {}.",
                backup.display()
            ),
            Problem::Unreadable {
                reason,
                backup: None,
            } => format!(
                "BitDinghy couldn't use your saved download list ({reason}). Downloads you add \
                 now won't be remembered after BitDinghy closes."
            ),
            Problem::NewerVersion { found } => format!(
                "Your download list was saved by a newer BitDinghy (list version {found}; this one \
                 understands up to {CURRENT_VERSION}). It is left untouched, and downloads you add \
                 now won't be remembered. Update BitDinghy to see your list again."
            ),
        }
    }
}

/// Why a file couldn't be used as our list.
enum Failure {
    /// Written by a newer BitDinghy.
    Newer(u32),
    /// Not a healthy BitDinghy list: set it aside and start again.
    Damaged(String),
    /// It couldn't even be opened (no permission, disk gone...). Leave it alone.
    CantOpen(String),
}

impl From<rusqlite::Error> for Failure {
    fn from(error: rusqlite::Error) -> Self {
        match error.sqlite_error_code() {
            Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => {
                Failure::Damaged("the file is damaged or is not a download list".to_string())
            }
            _ => Failure::CantOpen(error.to_string()),
        }
    }
}

/// The result of [`SavedList::open`]: the list, plus a problem to tell the person about, if any.
pub struct Opened {
    pub list: SavedList,
    pub problem: Option<Problem>,
}

/// The saved list, open for changes. Only one program can hold it at a time (see the module notes).
pub struct SavedList {
    path: PathBuf,
    /// `None` when the list can't be used this run (the [`Problem`] says why). Changes are then
    /// simply not remembered, and BitDinghy otherwise works normally.
    connection: Option<Connection>,
    /// Held for as long as the list is open. Closing the file releases the lock.
    _lock: File,
}

impl SavedList {
    /// Opens (or creates) the list at `path` for changes. Fails only with
    /// [`Error::AlreadyRunning`] or when the lock itself can't be made; any problem with the list
    /// is reported in [`Opened::problem`] and BitDinghy carries on.
    pub fn open(path: &Path) -> Result<Opened> {
        Self::open_with(path, MIGRATIONS)
    }

    fn open_with(path: &Path, migrations: &[Migration]) -> Result<Opened> {
        let lock = take_lock(path)?;
        let (connection, problem) = match open_and_upgrade(path, migrations) {
            Ok(connection) => (Some(connection), None),
            Err(Failure::Newer(found)) => (None, Some(Problem::NewerVersion { found })),
            Err(Failure::CantOpen(reason)) => (
                None,
                Some(Problem::Unreadable {
                    reason,
                    backup: None,
                }),
            ),
            Err(Failure::Damaged(reason)) => match set_aside(path) {
                Some(backup) => match open_and_upgrade(path, migrations) {
                    Ok(connection) => (
                        Some(connection),
                        Some(Problem::Unreadable {
                            reason,
                            backup: Some(backup),
                        }),
                    ),
                    Err(_) => (
                        None,
                        Some(Problem::Unreadable {
                            reason,
                            backup: Some(backup),
                        }),
                    ),
                },
                None => (
                    None,
                    Some(Problem::Unreadable {
                        reason,
                        backup: None,
                    }),
                ),
            },
        };
        Ok(Opened {
            list: SavedList {
                path: path.to_path_buf(),
                connection,
                _lock: lock,
            },
            problem,
        })
    }

    /// Reads the list without changing it and without the lock, so it works while the window is
    /// open (this is what `dinghy list` uses). A missing file is an empty list.
    pub fn read(path: &Path) -> Result<Vec<SavedDownload>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let problem = |reason: String| {
            Error::SavedList(format!(
                "BitDinghy couldn't read the download list in {} ({reason}).",
                path.display()
            ))
        };
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| problem(e.to_string()))?;
        let _ = connection.busy_timeout(Duration::from_secs(5));

        match check_identity(&connection) {
            Ok(Some(found)) if found > CURRENT_VERSION => {
                Err(Error::SavedList(Problem::NewerVersion { found }.message()))
            }
            // An older list is read as it is (reading never upgrades it; the next BitDinghy that
            // opens it for changes does).
            Ok(Some(version)) => {
                let columns = if version == 1 { COLUMNS_V1 } else { COLUMNS };
                rows_with(&connection, columns).map_err(|e| problem(e.to_string()))
            }
            Ok(None) => Ok(Vec::new()), // An empty file: nothing saved yet.
            Err(Failure::Newer(found)) => {
                Err(Error::SavedList(Problem::NewerVersion { found }.message()))
            }
            Err(Failure::Damaged(reason) | Failure::CantOpen(reason)) => Err(problem(reason)),
        }
    }

    /// Where the list is kept.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether changes are being remembered this run (see [`SavedList::connection`]).
    pub fn is_saving(&self) -> bool {
        self.connection.is_some()
    }

    /// Every saved download, oldest first.
    pub fn all(&self) -> Result<Vec<SavedDownload>> {
        let Some(connection) = &self.connection else {
            return Ok(Vec::new());
        };
        all_rows(connection).map_err(|e| self.error("read", e))
    }

    /// One saved download, if it is in the list.
    pub fn get(&self, info_hash: &str) -> Result<Option<SavedDownload>> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        connection
            .query_row(
                &format!("SELECT {COLUMNS} FROM downloads WHERE info_hash = ?1"),
                params![info_hash],
                row_to_download,
            )
            .optional()
            .map_err(|e| self.error("read", e))
    }

    /// Remembers a new download. If this torrent is already in the list, the saved one is kept
    /// unchanged and `false` is returned.
    pub fn add(&self, download: &SavedDownload) -> Result<bool> {
        let Some(connection) = &self.connection else {
            return Ok(false);
        };
        let added = connection
            .execute(
                &format!(
                    "INSERT OR IGNORE INTO downloads ({COLUMNS}) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
                ),
                params![
                    download.info_hash,
                    download.name,
                    download.magnet,
                    download.torrent,
                    path_text(&download.download_dir)?,
                    download
                        .output_folder
                        .as_deref()
                        .map(path_text)
                        .transpose()?,
                    download.paused,
                    i64::try_from(download.added_at).unwrap_or(i64::MAX),
                    file_choice::skipped_to_text(&download.skipped_files),
                ],
            )
            .map_err(|e| self.error("save", e))?;
        Ok(added == 1)
    }

    /// Remembers a download's file list once it has passed the safety check, so the next start
    /// doesn't have to ask other people for it again.
    pub fn record_file_list(
        &self,
        info_hash: &str,
        name: &str,
        torrent: &[u8],
        output_folder: &Path,
    ) -> Result<()> {
        let Some(connection) = &self.connection else {
            return Ok(());
        };
        connection
            .execute(
                "UPDATE downloads SET name = ?2, torrent = ?3, output_folder = ?4 \
                 WHERE info_hash = ?1",
                params![info_hash, name, torrent, path_text(output_folder)?],
            )
            .map_err(|e| self.error("save", e))?;
        Ok(())
    }

    /// Remembers whether a download is paused. Returns whether it was in the list.
    pub fn set_paused(&self, info_hash: &str, paused: bool) -> Result<bool> {
        let Some(connection) = &self.connection else {
            return Ok(false);
        };
        let changed = connection
            .execute(
                "UPDATE downloads SET paused = ?2 WHERE info_hash = ?1",
                params![info_hash, paused],
            )
            .map_err(|e| self.error("save", e))?;
        Ok(changed == 1)
    }

    /// Remembers which files the person unticked (empty: every file is wanted). Returns whether
    /// it was in the list.
    pub fn set_skipped_files(&self, info_hash: &str, skipped: &BTreeSet<usize>) -> Result<bool> {
        let Some(connection) = &self.connection else {
            return Ok(false);
        };
        let changed = connection
            .execute(
                "UPDATE downloads SET skipped_files = ?2 WHERE info_hash = ?1",
                params![info_hash, file_choice::skipped_to_text(skipped)],
            )
            .map_err(|e| self.error("save", e))?;
        Ok(changed == 1)
    }

    /// Forgets a download. Returns whether it was in the list. (Its files are not touched here.)
    pub fn remove(&self, info_hash: &str) -> Result<bool> {
        let Some(connection) = &self.connection else {
            return Ok(false);
        };
        let removed = connection
            .execute(
                "DELETE FROM downloads WHERE info_hash = ?1",
                params![info_hash],
            )
            .map_err(|e| self.error("save", e))?;
        Ok(removed == 1)
    }

    fn error(&self, action: &str, error: rusqlite::Error) -> Error {
        Error::SavedList(format!(
            "BitDinghy couldn't {action} the download list in {} ({error}).",
            self.path.display()
        ))
    }
}

/// The current time in seconds since 1970.
fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `path` with `suffix` added to its file name (`downloads.db` + `.lock`).
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// The list stores folders as text. A folder name that isn't valid text (possible, but very rare,
/// on Windows) can't be stored, so it is refused with a clear message instead of being garbled.
fn path_text(path: &Path) -> Result<&str> {
    path.to_str().ok_or_else(|| {
        Error::SavedList(format!(
            "The folder name {} can't be saved in the download list.",
            path.display()
        ))
    })
}

/// Takes the "only one BitDinghy at a time" lock that sits next to the list.
fn take_lock(list_path: &Path) -> Result<File> {
    let lock_path = with_suffix(list_path, ".lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(|e| {
            Error::SavedList(format!(
                "BitDinghy couldn't create {}: {e}",
                lock_path.display()
            ))
        })?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(Error::AlreadyRunning),
        Err(TryLockError::Error(e)) => Err(Error::SavedList(format!(
            "BitDinghy couldn't lock {}: {e}",
            lock_path.display()
        ))),
    }
}

/// Checks that an open file is a BitDinghy list and returns its version, or `None` for an empty
/// file (nothing saved in it yet). Changes nothing.
fn check_identity(connection: &Connection) -> std::result::Result<Option<u32>, Failure> {
    let application_id: i32 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let tables: i64 =
        connection.query_row("SELECT count(*) FROM sqlite_schema", [], |row| row.get(0))?;

    if application_id == 0 && version == 0 && tables == 0 {
        return Ok(None);
    }
    if application_id != APPLICATION_ID {
        return Err(Failure::Damaged(
            "the file is not a BitDinghy download list".to_string(),
        ));
    }
    let health: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if health != "ok" {
        return Err(Failure::Damaged("the file is damaged".to_string()));
    }
    match u32::try_from(version) {
        Ok(version) if version >= 1 => Ok(Some(version)),
        _ => Err(Failure::Damaged(
            "the list's version number is not valid".to_string(),
        )),
    }
}

/// Opens the list, creates it if it is new, and upgrades an older one.
fn open_and_upgrade(
    path: &Path,
    migrations: &[Migration],
) -> std::result::Result<Connection, Failure> {
    let mut connection = Connection::open(path)?;
    // If `dinghy list` happens to be reading at this very moment, wait a little for it.
    connection.busy_timeout(Duration::from_secs(5))?;

    let newest = migrations.len() as u32 + 1;
    let version = match check_identity(&connection)? {
        Some(version) => version,
        None => {
            let transaction = connection.transaction()?;
            transaction.execute_batch(SCHEMA_V1)?;
            transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
            transaction.pragma_update(None, "user_version", 1)?;
            transaction.commit()?;
            1
        }
    };
    if version > newest {
        return Err(Failure::Newer(version));
    }
    for (index, step) in migrations.iter().enumerate().skip(version as usize - 1) {
        let transaction = connection.transaction()?;
        step(&transaction)?;
        transaction.pragma_update(None, "user_version", index as u32 + 2)?;
        transaction.commit()?;
    }

    // Finally make sure the table really has the columns this version reads.
    connection
        .prepare(&format!("SELECT {COLUMNS} FROM downloads LIMIT 0"))
        .map_err(|_| Failure::Damaged("the list is missing information it needs".to_string()))?;
    Ok(connection)
}

/// Renames a list we can't use to `downloads.db.broken-<time>` (with its journal, if there is
/// one), so nothing is lost and a fresh list can start. Returns the new name if that worked.
fn set_aside(path: &Path) -> Option<PathBuf> {
    let backup = with_suffix(path, &format!(".broken-{}", now_seconds()));
    std::fs::rename(path, &backup).ok()?;
    // SQLite's "journal" holds a half-finished change. Left behind, it could be applied to the
    // NEW list by mistake, so it moves with the old file.
    let journal = with_suffix(path, "-journal");
    if journal.exists() {
        let _ = std::fs::rename(&journal, with_suffix(&backup, "-journal"));
    }
    Some(backup)
}

fn all_rows(connection: &Connection) -> rusqlite::Result<Vec<SavedDownload>> {
    rows_with(connection, COLUMNS)
}

/// Every row, oldest first, read through `columns` (which must match [`row_to_download`]).
fn rows_with(connection: &Connection, columns: &str) -> rusqlite::Result<Vec<SavedDownload>> {
    let mut statement = connection.prepare(&format!(
        "SELECT {columns} FROM downloads ORDER BY added_at, rowid"
    ))?;
    let rows = statement.query_map([], row_to_download)?;
    rows.collect()
}

fn row_to_download(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedDownload> {
    Ok(SavedDownload {
        info_hash: row.get(0)?,
        name: row.get(1)?,
        magnet: row.get(2)?,
        torrent: row.get(3)?,
        download_dir: PathBuf::from(row.get::<_, String>(4)?),
        output_folder: row.get::<_, Option<String>>(5)?.map(PathBuf::from),
        paused: row.get(6)?,
        added_at: u64::try_from(row.get::<_, i64>(7)?).unwrap_or(0),
        skipped_files: file_choice::skipped_from_text(&row.get::<_, String>(8)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "c9e15763f722f23e98a29decdfae341b98d53056";
    const OTHER_HASH: &str = "0123456789abcdef0123456789abcdef01234567";

    fn sample(info_hash: &str) -> SavedDownload {
        SavedDownload::new(
            info_hash,
            "Ubuntu 24.04",
            &format!("magnet:?xt=urn%3Abtih%3A{info_hash}"),
            Path::new("C:/Downloads/BitDinghy"),
        )
    }

    fn open(path: &Path) -> Opened {
        SavedList::open(path).expect("the list opens")
    }

    #[test]
    fn a_new_list_is_created_empty_and_marked_with_its_version() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        let opened = open(&path);
        assert!(opened.problem.is_none());
        assert!(opened.list.is_saving());
        assert!(opened.list.all().unwrap().is_empty());
        drop(opened);

        let connection = Connection::open(&path).unwrap();
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        let id: i32 = connection
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_VERSION);
        assert_eq!(id, APPLICATION_ID);
    }

    #[test]
    fn what_is_saved_is_there_after_a_restart() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        let first = sample(HASH);
        let second = sample(OTHER_HASH);
        {
            let list = open(&path).list;
            assert!(list.add(&first).unwrap());
            assert!(list.add(&second).unwrap());
            list.record_file_list(
                HASH,
                "ubuntu.iso",
                b"d4:infod...e",
                Path::new("C:/Downloads/BitDinghy"),
            )
            .unwrap();
        } // "BitDinghy closes"

        let list = open(&path).list;
        let saved = list.all().unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].name, "ubuntu.iso");
        assert_eq!(saved[0].torrent.as_deref(), Some(&b"d4:infod...e"[..]));
        assert_eq!(
            saved[0].output_folder.as_deref(),
            Some(Path::new("C:/Downloads/BitDinghy"))
        );
        assert_eq!(saved[1], second);
        assert_eq!(list.get(OTHER_HASH).unwrap(), Some(second));
        assert_eq!(list.get("ffff").unwrap(), None);
    }

    #[test]
    fn adding_the_same_torrent_twice_keeps_the_first() {
        let work = tempfile::tempdir().unwrap();
        let list = open(&work.path().join(FILE_NAME)).list;
        let original = sample(HASH);
        assert!(list.add(&original).unwrap());
        let mut again = sample(HASH);
        again.name = "something else".to_string();
        assert!(!list.add(&again).unwrap());
        assert_eq!(list.all().unwrap(), vec![original]);
    }

    #[test]
    fn pausing_is_remembered_after_a_restart() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        {
            let list = open(&path).list;
            list.add(&sample(HASH)).unwrap();
            assert!(list.set_paused(HASH, true).unwrap());
            assert!(
                !list.set_paused(OTHER_HASH, true).unwrap(),
                "not in the list"
            );
        }
        let list = open(&path).list;
        assert!(list.get(HASH).unwrap().unwrap().paused);
        list.set_paused(HASH, false).unwrap();
        assert!(!list.get(HASH).unwrap().unwrap().paused);
    }

    #[test]
    fn a_removed_download_is_forgotten() {
        let work = tempfile::tempdir().unwrap();
        let list = open(&work.path().join(FILE_NAME)).list;
        list.add(&sample(HASH)).unwrap();
        assert!(list.remove(HASH).unwrap());
        assert!(!list.remove(HASH).unwrap());
        assert!(list.all().unwrap().is_empty());
    }

    #[test]
    fn only_one_program_can_change_the_list_at_a_time() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        let first = open(&path);
        assert!(matches!(SavedList::open(&path), Err(Error::AlreadyRunning)));
        drop(first); // "the window closes"
        assert!(SavedList::open(&path).is_ok());
    }

    #[test]
    fn the_list_can_be_read_while_another_program_has_it_open() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        let list = open(&path).list;
        let saved = sample(HASH);
        list.add(&saved).unwrap();
        assert_eq!(SavedList::read(&path).unwrap(), vec![saved]);
        assert!(
            SavedList::read(&work.path().join("missing.db"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_damaged_file_is_kept_aside_and_a_fresh_list_is_used() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        std::fs::write(&path, b"this is certainly not a database, just some text").unwrap();
        std::fs::write(with_suffix(&path, "-journal"), b"half a change").unwrap();

        let opened = open(&path);
        let Some(Problem::Unreadable {
            backup: Some(backup),
            ..
        }) = &opened.problem
        else {
            panic!("expected a notice with a backup, got {:?}", opened.problem);
        };
        assert_eq!(
            std::fs::read(backup).unwrap(),
            b"this is certainly not a database, just some text"
        );
        assert!(
            !with_suffix(&path, "-journal").exists(),
            "the old journal moved with the old file"
        );
        assert!(
            opened
                .problem
                .as_ref()
                .unwrap()
                .message()
                .contains("kept as")
        );
        // The fresh list works.
        assert!(opened.list.is_saving());
        opened.list.add(&sample(HASH)).unwrap();
        assert_eq!(opened.list.all().unwrap().len(), 1);
    }

    #[test]
    fn another_programs_database_is_not_mistaken_for_our_list() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE recipes (name TEXT); INSERT INTO recipes VALUES ('soup');")
            .unwrap();
        let opened = open(&path);
        assert!(matches!(
            opened.problem,
            Some(Problem::Unreadable {
                backup: Some(_),
                ..
            })
        ));
        assert!(opened.list.all().unwrap().is_empty());
    }

    #[test]
    fn a_list_from_a_newer_bitdinghy_is_left_untouched() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        {
            let list = open(&path).list;
            list.add(&sample(HASH)).unwrap();
        }
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        let before = std::fs::read(&path).unwrap();

        let opened = open(&path);
        assert_eq!(opened.problem, Some(Problem::NewerVersion { found: 99 }));
        assert!(!opened.list.is_saving());
        assert!(opened.list.all().unwrap().is_empty());
        // Changes are quietly not remembered, and the file is not touched.
        assert!(!opened.list.add(&sample(OTHER_HASH)).unwrap());
        drop(opened);
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let error = SavedList::read(&path).unwrap_err().to_string();
        assert!(error.contains("newer BitDinghy"), "{error}");
    }

    #[test]
    fn the_version_1_fixture_still_loads() {
        // This fixture was written with list version 1 and must load in EVERY later version.
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        Connection::open(&path)
            .unwrap()
            .execute_batch(include_str!("../tests/fixtures/downloads-v1.sql"))
            .unwrap();

        let opened = open(&path);
        assert!(opened.problem.is_none(), "{:?}", opened.problem);
        let saved = opened.list.all().unwrap();
        assert_eq!(saved.len(), 2);

        assert_eq!(saved[0].info_hash, HASH);
        assert_eq!(saved[0].name, "Ubuntu 24.04");
        assert_eq!(saved[0].torrent, None);
        assert_eq!(saved[0].output_folder, None);
        assert_eq!(
            saved[0].download_dir,
            PathBuf::from(r"C:\Users\Someone\Downloads\BitDinghy")
        );
        assert!(!saved[0].paused);
        assert_eq!(saved[0].added_at, 1_791_620_000);

        assert_eq!(saved[1].info_hash, OTHER_HASH);
        assert_eq!(saved[1].name, "testdata");
        assert_eq!(
            saved[1].torrent.as_deref(),
            Some(&b"d4:infod4:name8:testdataee"[..])
        );
        assert_eq!(
            saved[1].output_folder,
            Some(PathBuf::from(
                r"C:\Users\Someone\Downloads\BitDinghy\testdata"
            ))
        );
        assert!(saved[1].paused);
        // Version 1 had no choice of files: every file is wanted.
        assert!(
            saved
                .iter()
                .all(|download| download.skipped_files.is_empty())
        );
        drop(opened);
        let version: u32 = Connection::open(&path)
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_VERSION, "opening it upgraded it");
    }

    #[test]
    fn a_version_1_list_can_be_read_without_upgrading_it() {
        // `dinghy list` after installing a newer BitDinghy, before the window has opened.
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        Connection::open(&path)
            .unwrap()
            .execute_batch(include_str!("../tests/fixtures/downloads-v1.sql"))
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let saved = SavedList::read(&path).unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[1].name, "testdata");
        assert!(saved[1].skipped_files.is_empty());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "reading changes nothing"
        );
    }

    #[test]
    fn the_version_2_fixture_still_loads() {
        // This fixture was written with list version 2 and must load in EVERY later version.
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        Connection::open(&path)
            .unwrap()
            .execute_batch(include_str!("../tests/fixtures/downloads-v2.sql"))
            .unwrap();
        assert_eq!(SavedList::read(&path).unwrap().len(), 2);

        let opened = open(&path);
        assert!(opened.problem.is_none(), "{:?}", opened.problem);
        let saved = opened.list.all().unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].name, "Some Linux");
        assert_eq!(saved[0].torrent, None);
        assert!(saved[0].skipped_files.is_empty());
        assert!(!saved[0].paused);
        assert_eq!(saved[0].added_at, 1_791_700_000);

        assert_eq!(saved[1].info_hash, OTHER_HASH);
        assert_eq!(
            saved[1].torrent.as_deref(),
            Some(&b"d4:infod4:name8:testdataee"[..])
        );
        assert_eq!(
            saved[1].output_folder,
            Some(PathBuf::from(
                r"C:\Users\Someone\Downloads\BitDinghy\testdata"
            ))
        );
        assert!(saved[1].paused);
        assert_eq!(saved[1].skipped_files, BTreeSet::from([1, 3]));
    }

    #[test]
    fn the_unticked_files_are_remembered_after_a_restart() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        {
            let list = open(&path).list;
            list.add(&sample(HASH)).unwrap();
            assert!(
                list.set_skipped_files(HASH, &BTreeSet::from([0, 4]))
                    .unwrap()
            );
            assert!(
                !list
                    .set_skipped_files(OTHER_HASH, &BTreeSet::from([1]))
                    .unwrap(),
                "not in the list"
            );
        }
        let list = open(&path).list;
        assert_eq!(
            list.get(HASH).unwrap().unwrap().skipped_files,
            BTreeSet::from([0, 4])
        );
        assert_eq!(
            SavedList::read(&path).unwrap()[0].skipped_files,
            BTreeSet::from([0, 4])
        );
        list.set_skipped_files(HASH, &BTreeSet::new()).unwrap();
        assert!(list.get(HASH).unwrap().unwrap().skipped_files.is_empty());
    }

    #[test]
    fn upgrades_run_in_order_and_are_remembered() {
        // A pretend version 3 that adds a column, to prove the upgrade machinery keeps working
        // for the next real upgrade.
        fn add_rating(transaction: &rusqlite::Transaction) -> rusqlite::Result<()> {
            transaction.execute_batch("ALTER TABLE downloads ADD COLUMN rating INTEGER")
        }
        let work = tempfile::tempdir().unwrap();
        let path = work.path().join(FILE_NAME);
        Connection::open(&path)
            .unwrap()
            .execute_batch(include_str!("../tests/fixtures/downloads-v1.sql"))
            .unwrap();

        let upgraded = SavedList::open_with(&path, &[v1_to_v2, add_rating]).unwrap();
        assert!(upgraded.problem.is_none());
        assert_eq!(upgraded.list.all().unwrap().len(), 2);
        drop(upgraded);
        let connection = Connection::open(&path).unwrap();
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3, "both steps ran, from version 1");
        connection
            .prepare("SELECT skipped_files, rating FROM downloads")
            .expect("both new columns exist");
    }

    #[test]
    fn the_newest_version_is_the_number_of_upgrade_steps_plus_one() {
        assert_eq!(CURRENT_VERSION, MIGRATIONS.len() as u32 + 1);
    }
}
