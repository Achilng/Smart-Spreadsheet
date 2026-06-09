use std::collections::HashSet;

use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use thiserror::Error;

use super::{Database, DatabaseError};

const TARGET_ROWS_TABLE: &str = "temp.tag_target_rows";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagMutationResult {
    pub affected_rows: usize,
    pub normalized_tags: Vec<String>,
    pub associations_changed: usize,
}

#[derive(Debug, Error)]
pub enum TagMutationError {
    #[error("数据库操作失败: {0}")]
    Database(#[from] DatabaseError),
    #[error("Tag 操作包含不存在的行 ID: {0:?}")]
    UnknownRows(Vec<i64>),
    #[error("行 ID 必须为正整数: {0}")]
    InvalidRowId(i64),
}

impl From<rusqlite::Error> for TagMutationError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(DatabaseError::Sqlite(error))
    }
}

impl Database {
    pub fn add_tags_to_rows(
        &mut self,
        row_ids: &[i64],
        tags: &[String],
    ) -> Result<TagMutationResult, TagMutationError> {
        mutate_tags(&mut self.connection, row_ids, tags, Mutation::Add)
    }

    pub fn remove_tags_from_rows(
        &mut self,
        row_ids: &[i64],
        tags: &[String],
    ) -> Result<TagMutationResult, TagMutationError> {
        mutate_tags(&mut self.connection, row_ids, tags, Mutation::Remove)
    }
}

#[derive(Debug, Clone, Copy)]
enum Mutation {
    Add,
    Remove,
}

fn mutate_tags(
    connection: &mut rusqlite::Connection,
    row_ids: &[i64],
    tags: &[String],
    mutation: Mutation,
) -> Result<TagMutationResult, TagMutationError> {
    let row_ids = normalize_row_ids(row_ids)?;
    let normalized_tags = normalize_tags(tags);
    if row_ids.is_empty() || normalized_tags.is_empty() {
        return Ok(TagMutationResult {
            affected_rows: row_ids.len(),
            normalized_tags,
            associations_changed: 0,
        });
    }

    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    create_target_rows(&transaction, &row_ids)?;
    let unknown_rows = find_unknown_rows(&transaction)?;
    if !unknown_rows.is_empty() {
        return Err(TagMutationError::UnknownRows(unknown_rows));
    }

    let associations_changed = match mutation {
        Mutation::Add => add_tags(&transaction, &normalized_tags)?,
        Mutation::Remove => remove_tags(&transaction, &normalized_tags)?,
    };
    transaction.execute_batch(&format!("DROP TABLE {TARGET_ROWS_TABLE};"))?;
    transaction.commit()?;

    Ok(TagMutationResult {
        affected_rows: row_ids.len(),
        normalized_tags,
        associations_changed,
    })
}

fn normalize_row_ids(row_ids: &[i64]) -> Result<Vec<i64>, TagMutationError> {
    let mut seen = HashSet::with_capacity(row_ids.len());
    let mut normalized = Vec::with_capacity(row_ids.len());
    for &row_id in row_ids {
        if row_id <= 0 {
            return Err(TagMutationError::InvalidRowId(row_id));
        }
        if seen.insert(row_id) {
            normalized.push(row_id);
        }
    }
    Ok(normalized)
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = HashSet::with_capacity(tags.len());
    let mut normalized = Vec::with_capacity(tags.len());
    for tag in tags {
        let tag = tag.trim();
        if !tag.is_empty() && seen.insert(tag.to_owned()) {
            normalized.push(tag.to_owned());
        }
    }
    normalized
}

fn create_target_rows(
    transaction: &Transaction<'_>,
    row_ids: &[i64],
) -> Result<(), rusqlite::Error> {
    transaction.execute_batch(&format!(
        "DROP TABLE IF EXISTS {TARGET_ROWS_TABLE};
         CREATE TEMP TABLE {TARGET_ROWS_TABLE} (
             id INTEGER PRIMARY KEY
         ) STRICT, WITHOUT ROWID;"
    ))?;
    let mut insert =
        transaction.prepare(&format!("INSERT INTO {TARGET_ROWS_TABLE}(id) VALUES (?1)"))?;
    for row_id in row_ids {
        insert.execute([row_id])?;
    }
    Ok(())
}

fn find_unknown_rows(transaction: &Transaction<'_>) -> Result<Vec<i64>, rusqlite::Error> {
    let mut statement = transaction.prepare(&format!(
        "SELECT target.id
         FROM {TARGET_ROWS_TABLE} AS target
         LEFT JOIN rows ON rows.id = target.id
         WHERE rows.id IS NULL
         ORDER BY target.id"
    ))?;
    statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()
}

fn add_tags(transaction: &Transaction<'_>, tags: &[String]) -> Result<usize, rusqlite::Error> {
    let mut changed = 0;
    for tag in tags {
        transaction.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [tag])?;
        let tag_id: i64 = transaction.query_row(
            "SELECT id FROM tags WHERE name = ?1 COLLATE BINARY",
            [tag],
            |row| row.get(0),
        )?;
        changed += transaction.execute(
            &format!(
                "INSERT OR IGNORE INTO row_tags(row_id, tag_id)
                 SELECT id, ?1 FROM {TARGET_ROWS_TABLE}"
            ),
            [tag_id],
        )?;
    }
    Ok(changed)
}

fn remove_tags(transaction: &Transaction<'_>, tags: &[String]) -> Result<usize, rusqlite::Error> {
    let mut changed = 0;
    for tag in tags {
        let tag_id = transaction
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE BINARY",
                [tag],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if let Some(tag_id) = tag_id {
            changed += transaction.execute(
                &format!(
                    "DELETE FROM row_tags
                     WHERE tag_id = ?1
                       AND row_id IN (SELECT id FROM {TARGET_ROWS_TABLE})"
                ),
                params![tag_id],
            )?;
        }
    }
    transaction.execute(
        "DELETE FROM tags
         WHERE NOT EXISTS (
             SELECT 1 FROM row_tags WHERE row_tags.tag_id = tags.id
         )",
        [],
    )?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use crate::excel::{ImportedRow, ParsedWorkbook};

    use super::*;

    #[test]
    fn batch_add_trims_deduplicates_and_preserves_case() {
        let mut database = database_with_rows(3);

        let result = database
            .add_tags_to_rows(
                &[1, 2, 2],
                &[
                    " Landscape ".into(),
                    "landscape".into(),
                    "Landscape".into(),
                    "   ".into(),
                ],
            )
            .unwrap();

        assert_eq!(result.affected_rows, 2);
        assert_eq!(
            result.normalized_tags,
            vec!["Landscape".to_owned(), "landscape".to_owned()]
        );
        assert_eq!(result.associations_changed, 4);
        assert_eq!(stored_tags(&database), vec!["Landscape", "landscape"]);
        assert_eq!(stored_row_tags(&database), 4);

        let repeated = database
            .add_tags_to_rows(&[1, 2], &["Landscape".into()])
            .unwrap();
        assert_eq!(repeated.associations_changed, 0);
    }

    #[test]
    fn batch_remove_is_case_sensitive_and_prunes_unused_tags() {
        let mut database = database_with_rows(2);
        database
            .add_tags_to_rows(&[1, 2], &["Landscape".into(), "landscape".into()])
            .unwrap();

        let first = database
            .remove_tags_from_rows(&[1], &["Landscape".into()])
            .unwrap();
        assert_eq!(first.associations_changed, 1);
        assert_eq!(stored_tags(&database), vec!["Landscape", "landscape"]);

        let second = database
            .remove_tags_from_rows(&[2], &["Landscape".into()])
            .unwrap();
        assert_eq!(second.associations_changed, 1);
        assert_eq!(stored_tags(&database), vec!["landscape"]);
        assert_eq!(stored_row_tags(&database), 2);
    }

    #[test]
    fn unknown_row_rolls_back_entire_batch() {
        let mut database = database_with_rows(2);
        database
            .add_tags_to_rows(&[1], &["existing".into()])
            .unwrap();

        let error = database
            .add_tags_to_rows(&[1, 999], &["new".into()])
            .unwrap_err();

        assert!(matches!(error, TagMutationError::UnknownRows(rows) if rows == vec![999]));
        assert_eq!(stored_tags(&database), vec!["existing"]);
        assert_eq!(stored_row_tags(&database), 1);

        let recovered = database
            .add_tags_to_rows(&[2], &["after-rollback".into()])
            .unwrap();
        assert_eq!(recovered.associations_changed, 1);
        assert_eq!(stored_tags(&database), vec!["after-rollback", "existing"]);
    }

    #[test]
    fn supports_ten_thousand_row_batch() {
        let mut database = database_with_rows(10_000);
        let row_ids = (1..=10_000).collect::<Vec<_>>();

        let added = database
            .add_tags_to_rows(&row_ids, &["large-batch".into()])
            .unwrap();
        let removed = database
            .remove_tags_from_rows(&row_ids, &["large-batch".into()])
            .unwrap();

        assert_eq!(added.associations_changed, 10_000);
        assert_eq!(removed.associations_changed, 10_000);
        assert!(stored_tags(&database).is_empty());
    }

    fn database_with_rows(count: i64) -> Database {
        let mut database = Database::open_in_memory().unwrap();
        let workbook = ParsedWorkbook {
            sheet_name: "NovelAI Metadata".into(),
            rows: (1..=count)
                .map(|id| ImportedRow {
                    source_row: u32::try_from(id + 1).unwrap(),
                    time: None,
                    positive_prompt: Some(format!("prompt {id}")),
                    negative_prompt: None,
                    artists: None,
                    image_folder: None,
                    image_path: None,
                })
                .collect(),
        };
        database
            .replace_workbook("test.xlsx", &workbook, &[])
            .unwrap();
        database
    }

    fn stored_tags(database: &Database) -> Vec<String> {
        let mut statement = database
            .connection
            .prepare("SELECT name FROM tags ORDER BY name COLLATE BINARY")
            .unwrap();
        statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    fn stored_row_tags(database: &Database) -> u32 {
        database
            .connection
            .query_row("SELECT COUNT(*) FROM row_tags", [], |row| row.get(0))
            .unwrap()
    }
}
