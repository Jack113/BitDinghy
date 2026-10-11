//! Everything a front-end needs at start-up, in one call: where BitDinghy's files are, what it
//! remembers, and any problems worth mentioning. The window and the command line both use this,
//! so they always agree.

use std::path::{Path, PathBuf};

use crate::data_dir::DataDir;
use crate::engine::EngineConfig;
use crate::error::{Error, Result};
use crate::saved_list;
use crate::settings::{
    Loaded, MAX_ACTIVE_DOWNLOADS_LIMIT, MAX_SHARING_MINUTES, MAX_SHARING_RATIO_PERCENT,
    MAX_SPEED_LIMIT_KIB, MIN_SPEED_LIMIT_KIB, Settings, SettingsStore, validate_download_dir,
};

/// A speed limit must be 0 (no limit) or between the lowest and highest that work.
pub fn check_speed_limit(which: &str, kib: u32) -> Result<()> {
    if kib == 0 || (MIN_SPEED_LIMIT_KIB..=MAX_SPEED_LIMIT_KIB).contains(&kib) {
        return Ok(());
    }
    Err(Error::Settings(format!(
        "The {which} limit must be 0 (no limit) or at least {MIN_SPEED_LIMIT_KIB} KiB/s (and at \
          most {MAX_SPEED_LIMIT_KIB} KiB/s). Less than {MIN_SPEED_LIMIT_KIB} KiB/s would stop \
          downloads altogether."
    )))
}

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

    /// Settings for the engine: the remembered download folder, and the DHT's saved contacts and
    /// the saved download list, both kept inside the data folder.
    pub fn engine_config(&self) -> EngineConfig {
        let mut config = EngineConfig::new(self.settings.download_dir_or_default());
        config.dht_state_file = self
            .data_dir
            .as_ref()
            .map(|data_dir| data_dir.file(DHT_STATE_FILE));
        config.saved_list_file = self.saved_list_path();
        config.allow_http_links = self.settings.allow_http_links;
        config.max_active_downloads = self.settings.max_active_downloads as usize;
        config.download_limit_kib = self.settings.download_limit_kib;
        config.upload_limit_kib = self.settings.upload_limit_kib;
        config.stop_sharing_ratio_percent = self.settings.stop_sharing_ratio_percent;
        config.stop_sharing_after_minutes = self.settings.stop_sharing_after_minutes;
        config
    }

    /// Where the saved download list is kept, if there is a data folder.
    pub fn saved_list_path(&self) -> Option<PathBuf> {
        self.data_dir
            .as_ref()
            .map(|data_dir| data_dir.file(saved_list::FILE_NAME))
    }

    /// Remembers a new download folder (or goes back to the standard one with `None`). The folder
    /// is checked first, and the change is only kept if it was saved.
    pub fn set_download_dir(&mut self, requested: Option<&Path>) -> Result<()> {
        let download_dir = match requested {
            Some(path) => Some(validate_download_dir(path)?),
            None => None,
        };
        self.change(|settings| settings.download_dir = download_dir)
    }

    /// Remembers whether plain `http://` links to `.torrent` files may be opened too.
    pub fn set_allow_http_links(&mut self, allow: bool) -> Result<()> {
        self.change(|settings| settings.allow_http_links = allow)
    }

    /// Remembers how many downloads run at the same time (0: no limit).
    pub fn set_max_active_downloads(&mut self, limit: u32) -> Result<()> {
        if limit > MAX_ACTIVE_DOWNLOADS_LIMIT {
            return Err(Error::Settings(format!(
                "At most {MAX_ACTIVE_DOWNLOADS_LIMIT} downloads can run at the same time (0 means \
                  no limit)."
            )));
        }
        self.change(|settings| settings.max_active_downloads = limit)
    }

    /// Remembers the speed limits for all downloads together, in KiB per second (0: no limit).
    pub fn set_speed_limits(&mut self, download_kib: u32, upload_kib: u32) -> Result<()> {
        for (which, kib) in [("download", download_kib), ("upload", upload_kib)] {
            check_speed_limit(which, kib)?;
        }
        self.change(|settings| {
            settings.download_limit_kib = download_kib;
            settings.upload_limit_kib = upload_kib;
        })
    }

    /// Remembers when finished downloads stop sharing: at a ratio (in percent, 100 = 1.0) and/or
    /// after so many minutes of sharing. 0 means no such limit.
    pub fn set_sharing_limits(&mut self, ratio_percent: u32, minutes: u32) -> Result<()> {
        if ratio_percent > MAX_SHARING_RATIO_PERCENT {
            return Err(Error::Settings(format!(
                "The sharing ratio can be at most {}.",
                MAX_SHARING_RATIO_PERCENT / 100
            )));
        }
        if minutes > MAX_SHARING_MINUTES {
            return Err(Error::Settings(format!(
                "The sharing time can be at most {MAX_SHARING_MINUTES} minutes (a year)."
            )));
        }
        self.change(|settings| {
            settings.stop_sharing_ratio_percent = ratio_percent;
            settings.stop_sharing_after_minutes = minutes;
        })
    }

    /// Changes the settings and saves them; the change is only kept if it was saved.
    fn change(&mut self, edit: impl FnOnce(&mut Settings)) -> Result<()> {
        let Some(store) = &self.store else {
            return Err(Error::Settings(
                "BitDinghy has nowhere to keep settings right now, so this can't be remembered."
                    .to_string(),
            ));
        };
        // Start from the file as it is NOW: the window and `dinghy` both change settings, and one
        // must not undo the other's change with an old copy.
        let fresh = store.load();
        let mut updated = match fresh.problem {
            None => fresh.settings,
            Some(_) => self.settings.clone(),
        };
        edit(&mut updated);
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
    fn speed_limits_are_checked_and_reach_the_engine() {
        let work = tempfile::tempdir().unwrap();
        let mut first_run = profile_in(work.path());
        assert_eq!(first_run.engine_config().download_limit_kib, 0);
        let error = first_run.set_speed_limits(10, 0).unwrap_err().to_string();
        assert!(error.contains("at least 32 KiB/s"), "{error}");
        let error = first_run
            .set_speed_limits(0, 5_000_000)
            .unwrap_err()
            .to_string();
        assert!(error.contains("upload"), "{error}");
        first_run.set_speed_limits(500, 0).unwrap();
        let second_run = profile_in(work.path());
        assert_eq!(second_run.engine_config().download_limit_kib, 500);
        assert_eq!(second_run.engine_config().upload_limit_kib, 0);
    }

    #[test]
    fn two_programs_changing_settings_keep_each_others_changes() {
        let work = tempfile::tempdir().unwrap();
        let mut window = profile_in(work.path());
        let mut command_line = profile_in(work.path());
        command_line.set_allow_http_links(true).unwrap();
        // The window still has its old copy in memory; its change must not undo the other one.
        window.set_max_active_downloads(7).unwrap();
        let later = profile_in(work.path());
        assert!(later.settings().allow_http_links);
        assert_eq!(later.settings().max_active_downloads, 7);
    }

    #[test]
    fn the_number_of_downloads_at_once_reaches_the_engine() {
        let work = tempfile::tempdir().unwrap();
        let mut first_run = profile_in(work.path());
        assert_eq!(first_run.engine_config().max_active_downloads, 3);
        first_run.set_max_active_downloads(5).unwrap();
        let error = first_run.set_max_active_downloads(101).unwrap_err();
        assert!(error.to_string().contains("At most 100"), "{error}");
        let second_run = profile_in(work.path());
        assert_eq!(second_run.engine_config().max_active_downloads, 5);
    }

    #[test]
    fn allowing_http_links_reaches_the_engine_and_survives_a_restart() {
        let work = tempfile::tempdir().unwrap();
        let mut first_run = profile_in(work.path());
        assert!(!first_run.engine_config().allow_http_links);
        first_run.set_allow_http_links(true).unwrap();
        let second_run = profile_in(work.path());
        assert!(second_run.settings().allow_http_links);
        assert!(second_run.engine_config().allow_http_links);
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
    fn the_saved_list_lives_inside_the_data_folder() {
        let work = tempfile::tempdir().unwrap();
        let profile = profile_in(work.path());
        let expected = profile
            .data_dir()
            .unwrap()
            .path()
            .join(saved_list::FILE_NAME);
        assert_eq!(profile.saved_list_path(), Some(expected.clone()));
        assert_eq!(profile.engine_config().saved_list_file, Some(expected));
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
        assert!(profile.engine_config().saved_list_file.is_none());
        assert!(profile.notices()[0].contains("can't remember your settings"));

        let mut profile = profile;
        assert!(
            profile
                .set_download_dir(Some(&work.path().join("x")))
                .is_err()
        );
    }
}
