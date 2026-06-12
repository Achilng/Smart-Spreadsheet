use std::collections::HashSet;

use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use thiserror::Error;

use super::query::{DedupeMode, FILTER_TAGS_TABLE, TagMatchMode, create_filter_tags, populate_filtered_rows};
use super::{Database, DatabaseError};

pub(super) const TARGET_ROWS_TABLE: &str = "temp.tag_target_rows";
const EXCLUDED_ROWS_TABLE: &str = "temp.tag_excluded_rows";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowSelection {
    Explicit {
        row_ids: Vec<i64>,
    },
    Filtered {
        tags: Vec<String>,
        tag_mode: TagMatchMode,
        dedupe: DedupeMode,
        excluded_row_ids: Vec<i64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagMutationResult {
    pub affected_rows: u64,
    pub normalized_tags: Vec<String>,
    pub associations_changed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagSelectionSummary {
    pub name: String,
    pub selected_rows: u64,
}

#[derive(Debug, Error)]
pub enum TagMutationError {
    #[error("数据库操作失败: {0}")]
    Database(#[from] DatabaseError),
    #[error("Tag 操作包含不存在的行 ID: {0:?}")]
    UnknownRows(Vec<i64>),
    #[error("行 ID 必须为正整数: {0}")]
    InvalidRowId(i64),
    #[error("Tag 名称不能为空")]
    EmptyTagName,
    #[error("Tag 不存在: {0:?}")]
    UnknownTags(Vec<String>),
}

impl From<rusqlite::Error> for TagMutationError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(DatabaseError::Sqlite(error))
    }
}

impl Database {
    pub fn create_tag(&mut self, name: &str) -> Result<bool, TagMutationError> {
        let Some(name) = normalize_tags(&[name.to_owned()]).into_iter().next() else {
            return Err(TagMutationError::EmptyTagName);
        };
        Ok(self
            .connection
            .execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [name])?
            > 0)
    }

    pub fn add_tags_to_rows(
        &mut self,
        row_ids: &[i64],
        tags: &[String],
    ) -> Result<TagMutationResult, TagMutationError> {
        self.add_tags_to_selection(
            &RowSelection::Explicit {
                row_ids: row_ids.to_vec(),
            },
            tags,
        )
    }

    pub fn remove_tags_from_rows(
        &mut self,
        row_ids: &[i64],
        tags: &[String],
    ) -> Result<TagMutationResult, TagMutationError> {
        self.remove_tags_from_selection(
            &RowSelection::Explicit {
                row_ids: row_ids.to_vec(),
            },
            tags,
        )
    }

    pub fn add_tags_to_selection(
        &mut self,
        selection: &RowSelection,
        tags: &[String],
    ) -> Result<TagMutationResult, TagMutationError> {
        mutate_tags(&mut self.connection, selection, tags, Mutation::Add)
    }

    pub fn remove_tags_from_selection(
        &mut self,
        selection: &RowSelection,
        tags: &[String],
    ) -> Result<TagMutationResult, TagMutationError> {
        mutate_tags(&mut self.connection, selection, tags, Mutation::Remove)
    }

    pub fn set_tags_for_row(
        &mut self,
        row_id: i64,
        tags: &[String],
    ) -> Result<TagMutationResult, TagMutationError> {
        if row_id <= 0 {
            return Err(TagMutationError::InvalidRowId(row_id));
        }
        let normalized_tags = normalize_tags(tags);
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row_exists: bool = transaction.query_row(
            "SELECT EXISTS (SELECT 1 FROM rows WHERE id = ?1)",
            [row_id],
            |row| row.get(0),
        )?;
        if !row_exists {
            return Err(TagMutationError::UnknownRows(vec![row_id]));
        }

        let mut desired_ids = Vec::with_capacity(normalized_tags.len());
        let mut unknown_tags = Vec::new();
        for tag in &normalized_tags {
            let tag_id = transaction
                .query_row(
                    "SELECT id FROM tags WHERE name = ?1 COLLATE BINARY",
                    [tag],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            match tag_id {
                Some(tag_id) => desired_ids.push(tag_id),
                None => unknown_tags.push(tag.clone()),
            }
        }
        if !unknown_tags.is_empty() {
            return Err(TagMutationError::UnknownTags(unknown_tags));
        }

        let desired = desired_ids.iter().copied().collect::<HashSet<_>>();
        let existing = {
            let mut statement = transaction
                .prepare("SELECT tag_id FROM row_tags WHERE row_id = ?1 ORDER BY tag_id")?;
            statement
                .query_map([row_id], |row| row.get::<_, i64>(0))?
                .collect::<Result<HashSet<_>, _>>()?
        };
        let mut associations_changed = 0;
        for tag_id in existing.difference(&desired) {
            associations_changed += transaction.execute(
                "DELETE FROM row_tags WHERE row_id = ?1 AND tag_id = ?2",
                params![row_id, tag_id],
            )?;
        }
        for tag_id in desired.difference(&existing) {
            associations_changed += transaction.execute(
                "INSERT INTO row_tags(row_id, tag_id) VALUES (?1, ?2)",
                params![row_id, tag_id],
            )?;
        }
        transaction.commit()?;

        Ok(TagMutationResult {
            affected_rows: 1,
            normalized_tags,
            associations_changed,
        })
    }

    pub fn count_selected_rows(
        &mut self,
        selection: &RowSelection,
    ) -> Result<u64, TagMutationError> {
        let transaction = self.connection.transaction()?;
        create_selection_rows(&transaction, selection)?;
        let count = target_row_count(&transaction)?;
        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        Ok(count)
    }

    pub fn list_selection_tags(
        &mut self,
        selection: &RowSelection,
    ) -> Result<Vec<TagSelectionSummary>, TagMutationError> {
        let transaction = self.connection.transaction()?;
        create_selection_rows(&transaction, selection)?;
        let summaries = {
            let mut statement = transaction.prepare(&format!(
                "SELECT tags.name, COUNT(target.id)
                 FROM tags
                 LEFT JOIN row_tags ON row_tags.tag_id = tags.id
                 LEFT JOIN {TARGET_ROWS_TABLE} AS target ON target.id = row_tags.row_id
                 GROUP BY tags.id, tags.name
                 ORDER BY tags.name COLLATE BINARY"
            ))?;
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        summaries
            .into_iter()
            .map(|(name, selected_rows)| {
                Ok(TagSelectionSummary {
                    name,
                    selected_rows: u64::try_from(selected_rows).map_err(|_| {
                        TagMutationError::Database(DatabaseError::CountOverflow)
                    })?,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
enum Mutation {
    Add,
    Remove,
}

fn mutate_tags(
    connection: &mut rusqlite::Connection,
    selection: &RowSelection,
    tags: &[String],
    mutation: Mutation,
) -> Result<TagMutationResult, TagMutationError> {
    let normalized_tags = normalize_tags(tags);
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    create_selection_rows(&transaction, selection)?;
    let affected_rows = target_row_count(&transaction)?;
    let associations_changed = if affected_rows == 0 || normalized_tags.is_empty() {
        0
    } else {
        match mutation {
            Mutation::Add => add_tags(&transaction, &normalized_tags)?,
            Mutation::Remove => remove_tags(&transaction, &normalized_tags)?,
        }
    };
    drop_selection_tables(&transaction)?;
    transaction.commit()?;

    Ok(TagMutationResult {
        affected_rows,
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

pub(super) fn normalize_tags(tags: &[String]) -> Vec<String> {
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

pub(super) fn create_selection_rows(
    transaction: &Transaction<'_>,
    selection: &RowSelection,
) -> Result<(), TagMutationError> {
    transaction.execute_batch(&format!(
        "DROP TABLE IF EXISTS {TARGET_ROWS_TABLE};
         CREATE TEMP TABLE {TARGET_ROWS_TABLE} (
             id INTEGER PRIMARY KEY
         ) STRICT, WITHOUT ROWID;"
    ))?;

    match selection {
        RowSelection::Explicit { row_ids } => {
            let row_ids = normalize_row_ids(row_ids)?;
            insert_row_ids(transaction, TARGET_ROWS_TABLE, &row_ids)?;
            let unknown_rows = find_unknown_rows(transaction)?;
            if !unknown_rows.is_empty() {
                return Err(TagMutationError::UnknownRows(unknown_rows));
            }
        }
        RowSelection::Filtered {
            tags,
            tag_mode,
            dedupe,
            excluded_row_ids,
        } => {
            let tags = normalize_tags(tags);
            let excluded_row_ids = normalize_row_ids(excluded_row_ids)?;
            create_filter_tags(transaction, &tags)?;
            transaction.execute_batch(&format!(
                "DROP TABLE IF EXISTS {EXCLUDED_ROWS_TABLE};
                 CREATE TEMP TABLE {EXCLUDED_ROWS_TABLE} (
                     id INTEGER PRIMARY KEY
                 ) STRICT, WITHOUT ROWID;"
            ))?;
            insert_row_ids(transaction, EXCLUDED_ROWS_TABLE, &excluded_row_ids)?;
            populate_filtered_rows(transaction, TARGET_ROWS_TABLE, *tag_mode, *dedupe)?;
            transaction.execute(
                &format!(
                    "DELETE FROM {TARGET_ROWS_TABLE}
                     WHERE EXISTS (
                         SELECT 1 FROM {EXCLUDED_ROWS_TABLE}
                         WHERE {EXCLUDED_ROWS_TABLE}.id = {TARGET_ROWS_TABLE}.id
                     )"
                ),
                [],
            )?;
        }
    }
    Ok(())
}

fn insert_row_ids(
    transaction: &Transaction<'_>,
    table: &str,
    row_ids: &[i64],
) -> Result<(), rusqlite::Error> {
    let mut insert = transaction.prepare(&format!("INSERT INTO {table}(id) VALUES (?1)"))?;
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

fn target_row_count(transaction: &Transaction<'_>) -> Result<u64, TagMutationError> {
    let count: i64 = transaction.query_row(
        &format!("SELECT COUNT(*) FROM {TARGET_ROWS_TABLE}"),
        [],
        |row| row.get(0),
    )?;
    u64::try_from(count).map_err(|_| TagMutationError::Database(DatabaseError::CountOverflow))
}

pub(super) fn drop_selection_tables(
    transaction: &Transaction<'_>,
) -> Result<(), rusqlite::Error> {
    transaction.execute_batch(&format!(
        "DROP TABLE IF EXISTS {TARGET_ROWS_TABLE};
         DROP TABLE IF EXISTS {EXCLUDED_ROWS_TABLE};
         DROP TABLE IF EXISTS {FILTER_TAGS_TABLE};"
    ))
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
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::database_with_rows;
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
    fn batch_remove_is_case_sensitive_and_keeps_tag_definitions() {
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
        assert_eq!(stored_tags(&database), vec!["Landscape", "landscape"]);
        assert_eq!(stored_row_tags(&database), 2);
    }

    #[test]
    fn creates_unassigned_tags_and_sets_a_row_from_existing_definitions() {
        let mut database = database_with_rows(2);
        assert!(database.create_tag(" 苹果 ").unwrap());
        assert!(database.create_tag("香蕉").unwrap());
        assert!(!database.create_tag("苹果").unwrap());

        let first = database
            .set_tags_for_row(1, &["苹果".into(), "香蕉".into()])
            .unwrap();
        assert_eq!(first.associations_changed, 2);
        assert!(row_has_tag(&database, 1, "苹果"));
        assert!(row_has_tag(&database, 1, "香蕉"));

        let second = database.set_tags_for_row(1, &["香蕉".into()]).unwrap();
        assert_eq!(second.associations_changed, 1);
        assert!(!row_has_tag(&database, 1, "苹果"));
        assert!(row_has_tag(&database, 1, "香蕉"));
        assert_eq!(stored_tags(&database), vec!["苹果", "香蕉"]);
    }

    #[test]
    fn setting_row_tags_rejects_unknown_definitions_without_changes() {
        let mut database = database_with_rows(1);
        database.create_tag("existing").unwrap();
        database.set_tags_for_row(1, &["existing".into()]).unwrap();

        let error = database
            .set_tags_for_row(1, &["existing".into(), "missing".into()])
            .unwrap_err();

        assert!(matches!(error, TagMutationError::UnknownTags(tags) if tags == vec!["missing"]));
        assert!(row_has_tag(&database, 1, "existing"));
        assert!(!row_has_tag(&database, 1, "missing"));
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
        assert_eq!(stored_tags(&database), vec!["large-batch"]);
    }

    #[test]
    fn filtered_selection_handles_ten_thousand_rows_with_only_exclusions() {
        let mut database = database_with_rows(10_000);
        let selection = RowSelection::Filtered {
            tags: Vec::new(),
            tag_mode: TagMatchMode::And,
            dedupe: DedupeMode::None,
            excluded_row_ids: vec![2, 9_999],
        };

        assert_eq!(database.count_selected_rows(&selection).unwrap(), 9_998);
        let result = database
            .add_tags_to_selection(&selection, &["selected".into()])
            .unwrap();

        assert_eq!(result.affected_rows, 9_998);
        assert_eq!(result.associations_changed, 9_998);
        assert!(!row_has_tag(&database, 2, "selected"));
        assert!(!row_has_tag(&database, 9_999, "selected"));
        assert!(row_has_tag(&database, 10_000, "selected"));
    }

    #[test]
    fn filtered_selection_uses_and_mode_and_exclusions() {
        let mut database = database_with_rows(5);
        database
            .add_tags_to_rows(&[1, 2, 3], &["A".into()])
            .unwrap();
        database
            .add_tags_to_rows(&[2, 3, 4], &["B".into()])
            .unwrap();
        let selection = RowSelection::Filtered {
            tags: vec![" A ".into(), "B".into()],
            tag_mode: TagMatchMode::And,
            dedupe: DedupeMode::None,
            excluded_row_ids: vec![3],
        };

        let result = database
            .add_tags_to_selection(&selection, &["matched".into()])
            .unwrap();

        assert_eq!(result.affected_rows, 1);
        assert!(row_has_tag(&database, 2, "matched"));
        assert!(!row_has_tag(&database, 3, "matched"));

        let removed = database
            .remove_tags_from_selection(&selection, &["A".into()])
            .unwrap();
        assert_eq!(removed.affected_rows, 1);
        assert_eq!(removed.associations_changed, 1);
        assert!(!row_has_tag(&database, 2, "A"));
        assert!(row_has_tag(&database, 3, "A"));
    }

    #[test]
    fn filtered_selection_uses_only_deduped_representatives() {
        let mut database = database_with_rows(4);
        database
            .connection
            .execute_batch(
                "UPDATE rows SET positive_prompt = CASE id
                     WHEN 1 THEN 'same'
                     WHEN 2 THEN ' same '
                     WHEN 3 THEN ''
                     ELSE 'other'
                 END;",
            )
            .unwrap();
        let selection = RowSelection::Filtered {
            tags: Vec::new(),
            tag_mode: TagMatchMode::And,
            dedupe: DedupeMode::PositivePrompt,
            excluded_row_ids: vec![1],
        };

        assert_eq!(database.count_selected_rows(&selection).unwrap(), 2);
        let result = database
            .add_tags_to_selection(&selection, &["visible".into()])
            .unwrap();
        assert_eq!(result.affected_rows, 2);

        let tagged_rows: Vec<i64> = database
            .connection
            .prepare(
                "SELECT row_tags.row_id FROM row_tags
                 JOIN tags ON tags.id = row_tags.tag_id
                 WHERE tags.name = 'visible'
                 ORDER BY row_tags.row_id",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(tagged_rows, vec![3, 4]);
    }

    #[test]
    fn lists_tag_coverage_for_explicit_and_filtered_selections() {
        let mut database = database_with_rows(3);
        database
            .add_tags_to_rows(&[1, 2], &["A".into()])
            .unwrap();
        database
            .add_tags_to_rows(&[2, 3], &["B".into()])
            .unwrap();
        database.create_tag("C").unwrap();

        let explicit = database
            .list_selection_tags(&RowSelection::Explicit {
                row_ids: vec![1, 2, 3],
            })
            .unwrap();
        assert_eq!(
            explicit,
            vec![
                TagSelectionSummary {
                    name: "A".into(),
                    selected_rows: 2,
                },
                TagSelectionSummary {
                    name: "B".into(),
                    selected_rows: 2,
                },
                TagSelectionSummary {
                    name: "C".into(),
                    selected_rows: 0,
                },
            ]
        );

        let filtered = database
            .list_selection_tags(&RowSelection::Filtered {
                tags: vec!["A".into()],
                tag_mode: TagMatchMode::And,
                dedupe: DedupeMode::None,
                excluded_row_ids: vec![2],
            })
            .unwrap();
        assert_eq!(filtered[0].selected_rows, 1);
        assert_eq!(filtered[1].selected_rows, 0);
        assert_eq!(filtered[2].selected_rows, 0);
    }

    #[test]
    fn empty_filtered_selection_does_not_create_orphan_tag() {
        let mut database = database_with_rows(2);
        let selection = RowSelection::Filtered {
            tags: vec!["missing".into()],
            tag_mode: TagMatchMode::And,
            dedupe: DedupeMode::None,
            excluded_row_ids: Vec::new(),
        };

        let result = database
            .add_tags_to_selection(&selection, &["unused".into()])
            .unwrap();

        assert_eq!(result.affected_rows, 0);
        assert_eq!(result.associations_changed, 0);
        assert!(stored_tags(&database).is_empty());
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

    fn row_has_tag(database: &Database, row_id: i64, tag: &str) -> bool {
        database
            .connection
            .query_row(
                "SELECT EXISTS (
                    SELECT 1
                    FROM row_tags
                    JOIN tags ON tags.id = row_tags.tag_id
                    WHERE row_tags.row_id = ?1
                      AND tags.name = ?2 COLLATE BINARY
                 )",
                params![row_id, tag],
                |row| row.get(0),
            )
            .unwrap()
    }
}
