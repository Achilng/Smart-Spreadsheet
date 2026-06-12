use rusqlite::{OptionalExtension, params};

use super::{Database, DatabaseError};

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
}
