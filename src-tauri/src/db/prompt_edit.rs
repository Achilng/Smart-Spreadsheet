use rusqlite::TransactionBehavior;
use serde::Serialize;

use super::tags::{RowSelection, TagMutationError, create_selection_rows, drop_selection_tables};
use super::Database;
use crate::pipeline::extract_artist_tags;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptEditResult {
    pub affected_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SinglePromptEditResult {
    pub affected_rows: u64,
    pub new_artists: Option<String>,
}

impl Database {
    pub fn update_positive_prompt(
        &mut self,
        row_id: i64,
        new_prompt: &str,
    ) -> Result<SinglePromptEditResult, TagMutationError> {
        let artists = extract_artist_tags(new_prompt);
        let artists_str = if artists.is_empty() {
            None
        } else {
            Some(artists.join(", "))
        };
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated = transaction.execute(
            "UPDATE rows SET positive_prompt = ?2, artists = ?3 WHERE id = ?1",
            rusqlite::params![row_id, new_prompt, &artists_str],
        )?;
        transaction.commit()?;
        Ok(SinglePromptEditResult {
            affected_rows: updated as u64,
            new_artists: artists_str,
        })
    }

    pub fn update_negative_prompt(
        &mut self,
        row_id: i64,
        new_prompt: &str,
    ) -> Result<u64, TagMutationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated = transaction.execute(
            "UPDATE rows SET negative_prompt = ?2 WHERE id = ?1",
            rusqlite::params![row_id, new_prompt],
        )?;
        transaction.commit()?;
        Ok(updated as u64)
    }

    pub fn find_replace_prompt(
        &mut self,
        selection: &RowSelection,
        find: &str,
        replace: &str,
    ) -> Result<PromptEditResult, TagMutationError> {
        if find.is_empty() {
            return Ok(PromptEditResult { affected_rows: 0 });
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        create_selection_rows(&transaction, selection)?;

        let target = super::tags::TARGET_ROWS_TABLE;
        let mut stmt = transaction.prepare(&format!(
            "SELECT r.id, r.positive_prompt FROM rows r
             INNER JOIN {target} t ON t.id = r.id
             WHERE r.positive_prompt IS NOT NULL AND INSTR(r.positive_prompt, ?1) > 0"
        ))?;
        let rows_to_update: Vec<(i64, String)> = stmt
            .query_map([find], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut update = transaction.prepare(
            "UPDATE rows SET positive_prompt = ?2, artists = ?3 WHERE id = ?1",
        )?;
        let mut count = 0u64;
        for (id, prompt) in &rows_to_update {
            let new_prompt = prompt.replace(find, replace);
            let artists = extract_artist_tags(&new_prompt);
            let artists_str = if artists.is_empty() {
                None
            } else {
                Some(artists.join(", "))
            };
            update.execute(rusqlite::params![id, new_prompt, artists_str])?;
            count += 1;
        }
        drop(update);

        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        Ok(PromptEditResult {
            affected_rows: count,
        })
    }

    pub fn prepend_artist(
        &mut self,
        selection: &RowSelection,
        artist_name: &str,
    ) -> Result<PromptEditResult, TagMutationError> {
        let artist_name = artist_name.trim();
        if artist_name.is_empty() {
            return Ok(PromptEditResult { affected_rows: 0 });
        }
        let prefix = format!("artist:{artist_name}, ");

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        create_selection_rows(&transaction, selection)?;

        let target = super::tags::TARGET_ROWS_TABLE;
        let mut stmt = transaction.prepare(&format!(
            "SELECT r.id, r.positive_prompt FROM rows r
             INNER JOIN {target} t ON t.id = r.id"
        ))?;
        let rows_to_update: Vec<(i64, Option<String>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut update = transaction.prepare(
            "UPDATE rows SET positive_prompt = ?2, artists = ?3 WHERE id = ?1",
        )?;
        let mut count = 0u64;
        for (id, prompt) in &rows_to_update {
            let old = prompt.as_deref().unwrap_or("");
            let new_prompt = format!("{prefix}{old}");
            let artists = extract_artist_tags(&new_prompt);
            let artists_str = if artists.is_empty() {
                None
            } else {
                Some(artists.join(", "))
            };
            update.execute(rusqlite::params![id, new_prompt, artists_str])?;
            count += 1;
        }
        drop(update);

        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        Ok(PromptEditResult {
            affected_rows: count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::database_with_rows;
    use super::super::tags::RowSelection;

    #[test]
    fn update_single_row_prompt_and_reextracts_artists() {
        let mut db = database_with_rows(3);
        let result = db
            .update_positive_prompt(1, "artist:alice, best quality, artist:bob")
            .unwrap();
        assert_eq!(result.affected_rows, 1);

        let (prompt, artists): (String, String) = db
            .connection
            .query_row(
                "SELECT positive_prompt, artists FROM rows WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(prompt, "artist:alice, best quality, artist:bob");
        assert_eq!(artists, "artist:alice, artist:bob");
    }

    #[test]
    fn update_prompt_clears_artists_when_none_present() {
        let mut db = database_with_rows(1);
        db.update_positive_prompt(1, "artist:x").unwrap();
        db.update_positive_prompt(1, "best quality, 1girl").unwrap();

        let artists: Option<String> = db
            .connection
            .query_row("SELECT artists FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(artists, None);
    }

    #[test]
    fn find_replace_modifies_matching_rows_only() {
        let mut db = database_with_rows(3);
        db.update_positive_prompt(1, "best quality, artist:alice, masterpiece")
            .unwrap();
        db.update_positive_prompt(2, "best quality, 1girl")
            .unwrap();
        db.update_positive_prompt(3, "best quality, artist:alice, 1boy")
            .unwrap();

        let result = db
            .find_replace_prompt(
                &RowSelection::Explicit {
                    row_ids: vec![1, 2, 3],
                },
                "best quality",
                "amazing quality",
            )
            .unwrap();
        assert_eq!(result.affected_rows, 3);

        let p1: String = db
            .connection
            .query_row(
                "SELECT positive_prompt FROM rows WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(p1.starts_with("amazing quality"));
    }

    #[test]
    fn find_replace_reextracts_artists() {
        let mut db = database_with_rows(1);
        db.update_positive_prompt(1, "artist:old_name, best quality")
            .unwrap();

        db.find_replace_prompt(
            &RowSelection::Explicit { row_ids: vec![1] },
            "artist:old_name",
            "artist:new_name",
        )
        .unwrap();

        let artists: String = db
            .connection
            .query_row("SELECT artists FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(artists, "artist:new_name");
    }

    #[test]
    fn find_replace_empty_find_is_noop() {
        let mut db = database_with_rows(1);
        let result = db
            .find_replace_prompt(
                &RowSelection::Explicit { row_ids: vec![1] },
                "",
                "something",
            )
            .unwrap();
        assert_eq!(result.affected_rows, 0);
    }

    #[test]
    fn prepend_artist_adds_prefix_and_reextracts() {
        let mut db = database_with_rows(2);
        db.update_positive_prompt(1, "best quality, 1girl")
            .unwrap();
        db.update_positive_prompt(2, "masterpiece").unwrap();

        let result = db
            .prepend_artist(
                &RowSelection::Explicit {
                    row_ids: vec![1, 2],
                },
                "alice",
            )
            .unwrap();
        assert_eq!(result.affected_rows, 2);

        let p1: String = db
            .connection
            .query_row(
                "SELECT positive_prompt FROM rows WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(p1, "artist:alice, best quality, 1girl");

        let a1: String = db
            .connection
            .query_row("SELECT artists FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(a1, "artist:alice");
    }

    #[test]
    fn prepend_artist_handles_null_prompt() {
        let mut db = database_with_rows(1);

        let result = db
            .prepend_artist(
                &RowSelection::Explicit { row_ids: vec![1] },
                "bob",
            )
            .unwrap();
        assert_eq!(result.affected_rows, 1);

        let prompt: String = db
            .connection
            .query_row(
                "SELECT positive_prompt FROM rows WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(prompt.starts_with("artist:bob, "));
    }

    #[test]
    fn prepend_artist_empty_name_is_noop() {
        let mut db = database_with_rows(1);
        let result = db
            .prepend_artist(
                &RowSelection::Explicit { row_ids: vec![1] },
                "  ",
            )
            .unwrap();
        assert_eq!(result.affected_rows, 0);
    }
}
