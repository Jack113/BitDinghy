//! Everything a front-end needs at start-up, in one call: where BitDinghy's files are, what it
//! remembers, and any problems worth mentioning. The window and the command line both use this,
//! so they always agree.

use std::path::Path;

use crate::data_dir::DataDir;
use crate::engine::EngineConfig;
use crate::error::{Error, Result};
use crate::settings::{Loaded, Settings, SettingsStore, validate_download_dir};

/// The name of the file where the DHT keeps its saved contacts, inside the data folder.
pub const DHT_STATE_FILE: &str = "dht.json";

/// The loaded settings plus the places they live.
#[derive(Debug)]
pub struct Profile {
    data_dir: Option<DataDir>,
    store: Option<SettingsStore>,
    settings: Settings,
    notices: Vec<String>,
}

impl Profile {
    /// Finds the data folder and loads the settings. This never fails: if something is wrong,
    /// BitDinghy carries on with standard settings and `notices()` explains why.
    pub fn load() -> Profile {
        Self::from_data_dir(DataDir::locate())
    }

    /// Same, with the data folder already decided (this is what the tests use).
    pub fn from_data_dir(found: Result<DataDir>) -> Profile {
        match found {
            Ok(data_dir) => {
                let store = SettingsStore::in_dir(&data_dir);
                let Loaded { settings, problem } = store.load();
                Profile {
                    data_dir: Some(data_dir),
                    store: Some(store),
                    settings,
                    notices: problem.map(|p| p.message()).into_iter().collect(),
                }
            }
            Err(error) => Profile {
                data_dir: None,
                store: None,
                settings: Settings::default(),
                notices: vec![format!(
                    "{error}. BitDinghy will work, but it can't remember your settings this time."
                )],
            },
        }
    }

    /// Messages to show the user, such as "your settings file was damaged".
    pub fn notices(&self) -> &[String] {
        &self.notices
    }

    /// The data folder, if one could be used.
    pub fn data_dir(&self) -> Option<&DataDir> {
        self.data_dir.as_ref()
    }

    /// The settings file's path, if there is a data folder.
    pub fn settings_path(&self) -> Option<&Path> {
        self.store.as_ref().map(|store| store.path())
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Settings for the engine: the remembered download folder, and the DHT's saved contacts kept
    /// inside the data folder.
    pub fn engine_config(&self) -> EngineConfig {
        let mut config = EngineConfig::new(self.settings.download_dir_or_default());
        config.dht_state_file = self
            .data_dir
            .as_ref()
            .map(|data_dir| data_dir.file(DHT_STATE_FILE));
        config
    }

    /// Remembers a new download folder (or goes back to the standard one with `None`). The folder
    /// is checked first, and the change is only kept if it was saved.
    pub fn set_download_dir(&mut self, requested: Option<&Path>) -> Result<()> {
        let Some(store) = &self.store else {
            return Err(Error::Settings(
                "BitDinghy has nowhere to keep settings right now, so this can't be remembered."
                    .to_string(),
            ));
        };
        let mut updated = self.settings.clone();
        updated.download_dir = match requested {
            Some(path) => Some(validate_download_dir(path)?),
            None => None,
        };
        store.save(&updated)?;
        self.settings = updated;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::FILE_NAME;

    fn profile_in(dir: &Path) -> Profile {
        Profile::from_data_dir(DataDir::custom(dir.to_path_buf()))
    }

    #[test]
    fn a_fresh_profile_uses_the_standard_settings_and_has_no_notices() {
        let work = tempfile::tempdir().unwrap();
        let profile = profile_in(work.path());
        assert_eq!(profile.settings(), &Settings::default());
        assert!(profile.notices().is_empty());
        assert!(profile.data_dir().is_some());
    }

    #[test]
    fn a_remembered_download_folder_survives_a_restart() {
        let work = tempfile::tempdir().unwrap();
        let data = work.path().join("data");
        let chosen = work.path().join("my-downloads");

        let mut first_run = profile_in(&data);
        first_run.set_download_dir(Some(&chosen)).unwrap();

        let second_run = profile_in(&data); // "restarting the program"
        assert_eq!(
            second_run.settings().download_dir.as_deref(),
            Some(chosen.as_path())
        );
        assert_eq!(second_run.engine_config().download_dir, chosen);
    }

    #[test]
    fn going_back_to_the_standard_folder_is_remembered_too() {
        let work = tempfile::tempdir().unwrap();
        let data = work.path().join("data");
        let mut profile = profile_in(&data);
        profile
            .set_download_dir(Some(&work.path().join("x")))
            .unwrap();
        profile.set_download_dir(None).unwrap();
        assert_eq!(profile_in(&data).settings().download_dir, None);
    }

    #[test]
    fn an_unusable_folder_is_refused_and_nothing_changes() {
        let work = tempfile::tempdir().unwrap();
        let data = work.path().join("data");
        let mut profile = profile_in(&data);
        let a_file = work.path().join("a-file");
        std::fs::write(&a_file, b"x").unwrap();

        assert!(profile.set_download_dir(Some(&a_file)).is_err());
        assert_eq!(profile.settings(), &Settings::default());
        assert!(!data.join(FILE_NAME).exists(), "nothing was saved");
    }

    #[test]
    fn the_dht_keeps_its_contacts_inside_the_data_folder() {
        let work = tempfile::tempdir().unwrap();
        let profile = profile_in(work.path());
        let config = profile.engine_config();
        let file = config.dht_state_file.expect("a file is set");
        assert_eq!(
            file,
            profile.data_dir().unwrap().path().join(DHT_STATE_FILE)
        );
    }

    #[test]
    fn a_damaged_settings_file_gives_a_notice_and_keeps_the_original() {
        let work = tempfile::tempdir().unwrap();
        std::fs::write(work.path().join(FILE_NAME), "= = broken").unwrap();
        let profile = profile_in(work.path());
        assert_eq!(profile.settings(), &Settings::default());
        assert_eq!(profile.notices().len(), 1);
        assert!(profile.notices()[0].contains("couldn't read your saved settings"));
    }

    #[test]
    fn with_no_usable_data_folder_the_program_still_works_but_says_so() {
        let work = tempfile::tempdir().unwrap();
        let blocker = work.path().join("a-file");
        std::fs::write(&blocker, b"x").unwrap();
        let profile = Profile::from_data_dir(DataDir::custom(blocker.join("data")));
        assert!(profile.data_dir().is_none());
        assert!(profile.engine_config().dht_state_file.is_none());
        assert!(profile.notices()[0].contains("can't remember your settings"));

        let mut profile = profile;
        assert!(
            profile
                .set_download_dir(Some(&work.path().join("x")))
                .is_err()
        );
    }
}
