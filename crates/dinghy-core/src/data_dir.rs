//! Where BitDinghy keeps its own files (settings now; the download list and history later).
//!
//! BitDinghy is portable: its files live in a `data` folder right next to the program, so you can
//! copy the whole folder to a USB stick and carry your settings with it. If that folder can't be
//! written to (for example the program sits in a read-only place), we fall back to the normal
//! per-user settings folder instead.
//!
//! `dinghy.exe` and `BitDinghy.exe` sit in the same folder, so they share one data folder.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Name of the folder created next to the program.
pub const PORTABLE_FOLDER_NAME: &str = "data";

/// Setting this environment variable to a folder path overrides the choice. Handy for testing and
/// for people who want their files somewhere specific.
pub const OVERRIDE_ENV_VAR: &str = "BITDINGHY_DATA_DIR";

/// Which of the possible places was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    /// Next to the program: travels with it.
    Portable,
    /// The normal per-user settings folder (used only when the portable folder isn't writable).
    UserProfile,
    /// Chosen explicitly through the environment variable.
    Custom,
}

impl DataKind {
    /// A short plain-language description for settings screens.
    pub fn describe(self) -> &'static str {
        match self {
            DataKind::Portable => "next to the program, so it travels with it",
            DataKind::UserProfile => {
                "in your user profile, because the folder next to the program can't be written to"
            }
            DataKind::Custom => "chosen with the BITDINGHY_DATA_DIR setting",
        }
    }
}

/// BitDinghy's data folder: known to exist and to be writable when it was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDir {
    path: PathBuf,
    kind: DataKind,
}

impl DataDir {
    /// Finds (and creates, if needed) the data folder: the override if set, otherwise the folder
    /// next to the program, otherwise the user's settings folder.
    pub fn locate() -> Result<DataDir> {
        if let Some(custom) = std::env::var_os(OVERRIDE_ENV_VAR).filter(|v| !v.is_empty()) {
            return Self::custom(PathBuf::from(custom));
        }
        let portable = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join(PORTABLE_FOLDER_NAME)));
        let user_profile = directories::ProjectDirs::from("", "", crate::APP_NAME)
            .map(|dirs| dirs.data_dir().to_path_buf());
        Self::choose(portable, user_profile)
    }

    /// Uses exactly this folder (created if needed).
    pub fn custom(path: PathBuf) -> Result<DataDir> {
        let path = std::path::absolute(&path).unwrap_or(path);
        ensure_writable(&path).map_err(|e| Error::DataDir {
            path: path.clone(),
            reason: e.to_string(),
        })?;
        Ok(DataDir {
            path,
            kind: DataKind::Custom,
        })
    }

    /// The decision itself, separated so tests can try every case: use `portable` if it is
    /// writable, otherwise `user_profile`, otherwise report that nothing works.
    pub fn choose(portable: Option<PathBuf>, user_profile: Option<PathBuf>) -> Result<DataDir> {
        let mut last_problem = None;
        for (candidate, kind) in [
            (portable, DataKind::Portable),
            (user_profile, DataKind::UserProfile),
        ] {
            let Some(path) = candidate else { continue };
            match ensure_writable(&path) {
                Ok(()) => return Ok(DataDir { path, kind }),
                Err(e) => {
                    last_problem = Some(Error::DataDir {
                        path,
                        reason: e.to_string(),
                    });
                }
            }
        }
        Err(last_problem.unwrap_or_else(|| Error::DataDir {
            path: PathBuf::new(),
            reason: "no place to keep BitDinghy's files could be found".to_string(),
        }))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn kind(&self) -> DataKind {
        self.kind
    }

    /// The full path of a file inside the data folder.
    pub fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

/// Creates the folder if needed and proves we can really write in it, by creating and deleting a
/// small file. (A folder can exist and still refuse writes, for example on a read-only disc.)
fn ensure_writable(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(format!(".write-test-{}", std::process::id()));
    std::fs::write(&probe, b"ok")?;
    std::fs::remove_file(&probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portable_folder_is_used_when_it_can_be_written() {
        let work = tempfile::tempdir().unwrap();
        let portable = work.path().join("app").join("data");
        let profile = work.path().join("profile");
        let chosen = DataDir::choose(Some(portable.clone()), Some(profile.clone())).unwrap();
        assert_eq!(chosen.kind(), DataKind::Portable);
        assert_eq!(chosen.path(), portable);
        assert!(portable.is_dir(), "the folder is created");
        assert!(!profile.exists(), "the fallback is left alone");
    }

    #[test]
    fn it_falls_back_to_the_user_profile_when_the_portable_folder_cannot_be_used() {
        let work = tempfile::tempdir().unwrap();
        // A "folder" that can't be made because its parent is actually a file.
        let blocker = work.path().join("not-a-folder");
        std::fs::write(&blocker, b"x").unwrap();
        let portable = blocker.join("data");
        let profile = work.path().join("profile");
        let chosen = DataDir::choose(Some(portable), Some(profile.clone())).unwrap();
        assert_eq!(chosen.kind(), DataKind::UserProfile);
        assert_eq!(chosen.path(), profile);
    }

    #[test]
    fn it_reports_a_plain_message_when_nothing_works() {
        let work = tempfile::tempdir().unwrap();
        let blocker = work.path().join("not-a-folder");
        std::fs::write(&blocker, b"x").unwrap();
        let error = DataDir::choose(Some(blocker.join("a")), Some(blocker.join("b"))).unwrap_err();
        assert!(
            error.to_string().contains("can't keep its files"),
            "{error}"
        );
        assert!(DataDir::choose(None, None).is_err());
    }

    #[test]
    fn a_custom_folder_is_created_and_marked_custom() {
        let work = tempfile::tempdir().unwrap();
        let chosen = DataDir::custom(work.path().join("mine")).unwrap();
        assert_eq!(chosen.kind(), DataKind::Custom);
        assert!(chosen.path().is_dir());
        assert_eq!(
            chosen.file("settings.toml"),
            chosen.path().join("settings.toml")
        );
    }

    #[test]
    fn the_write_test_leaves_nothing_behind() {
        let work = tempfile::tempdir().unwrap();
        DataDir::choose(Some(work.path().join("data")), None).unwrap();
        let leftovers = std::fs::read_dir(work.path().join("data")).unwrap().count();
        assert_eq!(leftovers, 0);
    }
}
