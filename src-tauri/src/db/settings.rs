use rusqlite::{OptionalExtension, params};

use super::{Database, DatabaseError};

const AUTO_ARTIST_PREFIX_ON_IMPORT_SETTING: &str = "auto_artist_prefix_on_import";

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
}
