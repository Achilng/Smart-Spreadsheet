use super::matching::{EvaluatedCondition, matching_rows};
use super::model::{
    QuickEditCondition, QuickEditError, QuickTagApplyResult, QuickTagAssociation, QuickTagPreview,
};
use super::{PREVIEW_SAMPLE_LIMIT, row_count};
use crate::db::tags::normalize_tags;
use crate::db::{Database, DatabaseError};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::HashSet;

impl Database {
    pub fn preview_quick_tag(
        &self,
        condition: &QuickEditCondition,
        tags: &[String],
    ) -> Result<QuickTagPreview, QuickEditError> {
        let condition = EvaluatedCondition::new(condition)?;
        let tags = validated_tags(&self.connection, tags)?;
        let scanned_rows = row_count(&self.connection)?;
        let matched_rows = matching_rows(&self.connection, &condition)?;
        let tag_ids = tag_ids(&self.connection, &tags)?;
        let existing = existing_associations(&self.connection, &tag_ids)?;

        let mut rows_needing_changes = 0_u64;
        let mut associations_to_add = 0_u64;
        for row in &matched_rows {
            let missing = tag_ids
                .iter()
                .filter(|tag_id| !existing.contains(&(row.id, **tag_id)))
                .count();
            if missing > 0 {
                rows_needing_changes += 1;
                associations_to_add +=
                    u64::try_from(missing).map_err(|_| DatabaseError::CountOverflow)?;
            }
        }
        let matched_row_count =
            u64::try_from(matched_rows.len()).map_err(|_| DatabaseError::CountOverflow)?;

        Ok(QuickTagPreview {
            scanned_rows,
            matched_rows: matched_row_count,
            rows_needing_changes,
            already_tagged_rows: matched_row_count - rows_needing_changes,
            associations_to_add,
            sample_row_ids: matched_rows
                .into_iter()
                .map(|row| row.id)
                .take(PREVIEW_SAMPLE_LIMIT)
                .collect(),
            normalized_tokens: condition.tokens,
            normalized_tags: tags,
        })
    }

    pub fn apply_quick_tag(
        &mut self,
        condition: &QuickEditCondition,
        tags: &[String],
    ) -> Result<QuickTagApplyResult, QuickEditError> {
        let condition = EvaluatedCondition::new(condition)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let tags = validated_tags(&transaction, tags)?;
        let scanned_rows = row_count(&transaction)?;
        let matched_rows = matching_rows(&transaction, &condition)?;
        let tag_ids = tag_ids(&transaction, &tags)?;

        let mut insert = transaction
            .prepare("INSERT OR IGNORE INTO row_tags(row_id, tag_id) VALUES (?1, ?2)")?;
        let mut changed_rows = HashSet::new();
        let mut changes = Vec::new();
        for row in &matched_rows {
            for (tag, tag_id) in tags.iter().zip(&tag_ids) {
                if insert.execute(params![row.id, tag_id])? > 0 {
                    changed_rows.insert(row.id);
                    changes.push(QuickTagAssociation {
                        row_id: row.id,
                        tag: tag.clone(),
                    });
                }
            }
        }
        drop(insert);
        transaction.commit()?;
        self.bump_data_version();

        Ok(QuickTagApplyResult {
            scanned_rows,
            matched_rows: u64::try_from(matched_rows.len())
                .map_err(|_| DatabaseError::CountOverflow)?,
            changed_rows: u64::try_from(changed_rows.len())
                .map_err(|_| DatabaseError::CountOverflow)?,
            associations_changed: u64::try_from(changes.len())
                .map_err(|_| DatabaseError::CountOverflow)?,
            changes,
        })
    }

    pub fn revert_quick_tag_changes(
        &mut self,
        changes: &[QuickTagAssociation],
    ) -> Result<u64, QuickEditError> {
        self.mutate_quick_tag_changes(changes, false)
    }

    pub fn reapply_quick_tag_changes(
        &mut self,
        changes: &[QuickTagAssociation],
    ) -> Result<u64, QuickEditError> {
        self.mutate_quick_tag_changes(changes, true)
    }

    fn mutate_quick_tag_changes(
        &mut self,
        changes: &[QuickTagAssociation],
        add: bool,
    ) -> Result<u64, QuickEditError> {
        let changes = normalize_changes(changes)?;
        if changes.is_empty() {
            return Ok(0);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let tags = changes
            .iter()
            .map(|change| change.tag.clone())
            .collect::<Vec<_>>();
        let unique_tags = normalize_tags(&tags);
        let ids = tag_ids(&transaction, &validated_tags(&transaction, &unique_tags)?)?;
        let tag_id_by_name = unique_tags
            .into_iter()
            .zip(ids)
            .collect::<std::collections::HashMap<_, _>>();

        for change in &changes {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM rows WHERE id = ?1)",
                [change.row_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(QuickEditError::UnknownRow(change.row_id));
            }
        }

        let sql = if add {
            "INSERT OR IGNORE INTO row_tags(row_id, tag_id) VALUES (?1, ?2)"
        } else {
            "DELETE FROM row_tags WHERE row_id = ?1 AND tag_id = ?2"
        };
        let mut statement = transaction.prepare(sql)?;
        let mut changed = 0_u64;
        for change in &changes {
            let tag_id = tag_id_by_name
                .get(&change.tag)
                .expect("validated tag has an id");
            changed += u64::try_from(statement.execute(params![change.row_id, tag_id])?)
                .map_err(|_| DatabaseError::CountOverflow)?;
        }
        drop(statement);
        transaction.commit()?;
        self.bump_data_version();
        Ok(changed)
    }
}

fn validated_tags(connection: &Connection, tags: &[String]) -> Result<Vec<String>, QuickEditError> {
    let tags = normalize_tags(tags);
    if tags.is_empty() {
        return Err(QuickEditError::EmptyTags);
    }
    let mut unknown = Vec::new();
    for tag in &tags {
        let exists = connection
            .query_row(
                "SELECT 1 FROM tags WHERE name = ?1 COLLATE BINARY",
                [tag],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            unknown.push(tag.clone());
        }
    }
    if !unknown.is_empty() {
        return Err(QuickEditError::UnknownTags(unknown));
    }
    Ok(tags)
}
fn tag_ids(connection: &Connection, tags: &[String]) -> Result<Vec<i64>, rusqlite::Error> {
    tags.iter()
        .map(|tag| {
            connection.query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE BINARY",
                [tag],
                |row| row.get(0),
            )
        })
        .collect()
}

fn existing_associations(
    connection: &Connection,
    tag_ids: &[i64],
) -> Result<HashSet<(i64, i64)>, rusqlite::Error> {
    let mut existing = HashSet::new();
    let mut statement =
        connection.prepare("SELECT row_id FROM row_tags WHERE tag_id = ?1 ORDER BY row_id")?;
    for tag_id in tag_ids {
        let rows = statement.query_map([tag_id], |row| row.get::<_, i64>(0))?;
        for row_id in rows {
            existing.insert((row_id?, *tag_id));
        }
    }
    Ok(existing)
}

fn normalize_changes(
    changes: &[QuickTagAssociation],
) -> Result<Vec<QuickTagAssociation>, QuickEditError> {
    let mut seen = HashSet::with_capacity(changes.len());
    let mut normalized = Vec::with_capacity(changes.len());
    for change in changes {
        if change.row_id <= 0 {
            return Err(QuickEditError::InvalidRowId(change.row_id));
        }
        let tag = change.tag.trim();
        if tag.is_empty() {
            return Err(QuickEditError::EmptyTags);
        }
        if seen.insert((change.row_id, tag.to_owned())) {
            normalized.push(QuickTagAssociation {
                row_id: change.row_id,
                tag: tag.to_owned(),
            });
        }
    }
    Ok(normalized)
}
