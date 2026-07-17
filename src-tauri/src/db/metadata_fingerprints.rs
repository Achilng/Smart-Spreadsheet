use rusqlite::params;

use super::{Database, DatabaseError, RowImageLocator};

impl Database {
    pub fn missing_metadata_fingerprints(&self) -> Result<Vec<RowImageLocator>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT id, image_path, stored_image_path, stored_image_is_original
             FROM rows
             WHERE metadata_fingerprint IS NULL
             ORDER BY id",
        )?;
        let stored = statement
            .query_map([], |row| {
                Ok(RowImageLocator {
                    row_id: row.get(0)?,
                    image_path: row.get(1)?,
                    stored_image_path: row.get(2)?,
                    stored_image_is_original: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(stored)
    }

    pub fn update_metadata_fingerprints(
        &mut self,
        fingerprints: &[(i64, String)],
    ) -> Result<(), DatabaseError> {
        let transaction = self.connection.transaction()?;
        {
            let mut update = transaction.prepare(
                "UPDATE rows SET metadata_fingerprint = ?2
                 WHERE id = ?1 AND metadata_fingerprint IS NULL",
            )?;
            for (row_id, fingerprint) in fingerprints {
                if update.execute(params![row_id, fingerprint])? != 1 {
                    return Err(DatabaseError::RowNotFound(*row_id));
                }
            }
        }
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::append_rows;
    use super::*;

    #[test]
    fn lists_and_updates_missing_metadata_fingerprints() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &super::super::test_support::test_rows(2));

        assert_eq!(
            database
                .missing_metadata_fingerprints()
                .unwrap()
                .into_iter()
                .map(|row| row.row_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );

        database
            .update_metadata_fingerprints(&[(1, "fingerprint".into())])
            .unwrap();

        assert_eq!(
            database
                .missing_metadata_fingerprints()
                .unwrap()
                .into_iter()
                .map(|row| row.row_id)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }
}
