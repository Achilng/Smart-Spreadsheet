use rusqlite::params;

use super::{Database, DatabaseError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentHashCandidate {
    pub row_id: i64,
    pub image_path: Option<String>,
    pub stored_image_path: Option<String>,
}

impl Database {
    pub fn missing_content_hashes(&self) -> Result<Vec<ContentHashCandidate>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT id, image_path, stored_image_path
             FROM rows
             WHERE content_hash IS NULL
             ORDER BY id",
        )?;
        let candidates = statement
            .query_map([], |row| {
                Ok(ContentHashCandidate {
                    row_id: row.get(0)?,
                    image_path: row.get(1)?,
                    stored_image_path: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(candidates)
    }

    pub fn update_content_hashes(&mut self, hashes: &[(i64, String)]) -> Result<(), DatabaseError> {
        let transaction = self.connection.transaction()?;
        {
            let mut update = transaction.prepare(
                "UPDATE rows SET content_hash = ?2
                 WHERE id = ?1 AND content_hash IS NULL",
            )?;
            for (row_id, content_hash) in hashes {
                if update.execute(params![row_id, content_hash])? != 1 {
                    return Err(DatabaseError::RowNotFound(*row_id));
                }
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn content_hash_for_row(&self, row_id: i64) -> Result<Option<String>, DatabaseError> {
        self.connection
            .query_row(
                "SELECT content_hash FROM rows WHERE id = ?1",
                [row_id],
                |row| row.get(0),
            )
            .map_err(DatabaseError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::append_rows;
    use super::*;

    #[test]
    fn lists_missing_hashes_and_updates_them_transactionally() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &super::super::test_support::test_rows(2));

        let candidates = database.missing_content_hashes().unwrap();
        assert_eq!(
            candidates.iter().map(|row| row.row_id).collect::<Vec<_>>(),
            vec![1, 2]
        );

        database
            .update_content_hashes(&[(1, "abc".into()), (2, "abc".into())])
            .unwrap();

        assert!(database.missing_content_hashes().unwrap().is_empty());
        assert_eq!(
            database.content_hash_for_row(1).unwrap().as_deref(),
            Some("abc")
        );
        assert_eq!(
            database.content_hash_for_row(2).unwrap().as_deref(),
            Some("abc")
        );
    }
}
