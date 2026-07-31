use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Database, DatabaseError};

const AUTO_ARTIST_PREFIX_ON_IMPORT_SETTING: &str = "auto_artist_prefix_on_import";
const IMAGE_EXPORT_SETTINGS: &str = "image_export_settings_v1";

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageExportRenameMode {
    #[default]
    Random,
    Custom,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ImageExportSettings {
    pub destination: Option<String>,
    pub rename_enabled: bool,
    pub rename_mode: ImageExportRenameMode,
    pub custom_name: String,
    pub strip_metadata: bool,
}

impl Database {
    pub fn setting(&self, key: &str) -> Result<Option<String>, DatabaseError> {
        Ok(self
            .connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<(), DatabaseError> {
        self.connection.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn auto_artist_prefix_on_import(&self) -> Result<bool, DatabaseError> {
        Ok(self
            .setting(AUTO_ARTIST_PREFIX_ON_IMPORT_SETTING)?
            .as_deref()
            == Some("1"))
    }

    pub fn set_auto_artist_prefix_on_import(
        &mut self,
        enabled: bool,
    ) -> Result<(), DatabaseError> {
        self.set_setting(
            AUTO_ARTIST_PREFIX_ON_IMPORT_SETTING,
            if enabled { "1" } else { "0" },
        )
    }

    pub fn image_export_settings(&self) -> Result<ImageExportSettings, DatabaseError> {
        let Some(json) = self.setting(IMAGE_EXPORT_SETTINGS)? else {
            return Ok(ImageExportSettings::default());
        };
        Ok(serde_json::from_str(&json).unwrap_or_default())
    }

    pub fn set_image_export_settings(
        &mut self,
        settings: &ImageExportSettings,
    ) -> Result<(), DatabaseError> {
        let json = serde_json::to_string(settings)
            .expect("image export settings contain only serializable fields");
        self.set_setting(IMAGE_EXPORT_SETTINGS, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_updates_setting() {
        let mut database = Database::open_in_memory().unwrap();

        assert_eq!(database.setting("reject-dir").unwrap(), None);
        database.set_setting("reject-dir", r"D:\first").unwrap();
        database.set_setting("reject-dir", r"D:\second").unwrap();

        assert_eq!(
            database.setting("reject-dir").unwrap().as_deref(),
            Some(r"D:\second")
        );
    }

    #[test]
    fn artist_prefix_import_setting_defaults_off_and_persists() {
        let mut database = Database::open_in_memory().unwrap();

        assert!(!database.auto_artist_prefix_on_import().unwrap());
        database.set_auto_artist_prefix_on_import(true).unwrap();
        assert!(database.auto_artist_prefix_on_import().unwrap());
        database.set_auto_artist_prefix_on_import(false).unwrap();
        assert!(!database.auto_artist_prefix_on_import().unwrap());
    }

    #[test]
    fn image_export_settings_default_and_persist_all_options() {
        let mut database = Database::open_in_memory().unwrap();

        assert_eq!(
            database.image_export_settings().unwrap(),
            ImageExportSettings::default()
        );

        let settings = ImageExportSettings {
            destination: Some(r"D:\exports".into()),
            rename_enabled: true,
            rename_mode: ImageExportRenameMode::Custom,
            custom_name: "胡桃精选".into(),
            strip_metadata: true,
        };
        database.set_image_export_settings(&settings).unwrap();

        assert_eq!(database.image_export_settings().unwrap(), settings);
    }

    #[test]
    fn invalid_image_export_settings_fall_back_to_defaults() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .set_setting(IMAGE_EXPORT_SETTINGS, "not valid json")
            .unwrap();

        assert_eq!(
            database.image_export_settings().unwrap(),
            ImageExportSettings::default()
        );
    }
}
