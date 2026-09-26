use rusqlite::params;

use super::{Database, DatabaseError};

impl Database {
    pub fn set_favorite(&mut self, row_id: i64, favorite: bool) -> Result<u64, DatabaseError> {
        let updated = self.connection.execute(
            "UPDATE rows SET favorite = ?2 WHERE id = ?1",
            params![row_id, favorite],
        )?;
        if updated > 0 {
            self.bump_data_version();
        }
        Ok(updated as u64)
    }
}
