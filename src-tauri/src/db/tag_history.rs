use rusqlite::TransactionBehavior;
use serde::{Deserialize, Serialize};

use super::tags::{
    TARGET_ROWS_TABLE, add_tags, create_selection_rows, drop_selection_tables, normalize_tags,
    remove_tags, target_row_count,
};
use super::{Database, RowSelection, TagMutationError};

/// Only associations that actually changed belong to this operation's history.
/// Grouping IDs by tag avoids repeating names or unrelated row text in IPC/history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagAssociationChange {
    pub tag: String,
    pub row_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagChanges {
    pub added: Vec<TagAssociationChange>,
    pub removed: Vec<TagAssociationChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagEditResult {
    pub affected_rows: u64,
    pub changes: TagChanges,
}

impl Database {
    /// Freeze the selection, capture the delta and apply both directions atomically.
    pub fn edit_selection_tags(
        &mut self,
        selection: &RowSelection,
        add: &[String],
        remove: &[String],
    ) -> Result<TagEditResult, TagMutationError> {
        let remove = normalize_tags(remove);
        let add: Vec<_> = normalize_tags(add)
            .into_iter()
            .filter(|tag| !remove.contains(tag))
            .collect();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        create_selection_rows(&transaction, selection)?;
        let affected_rows = target_row_count(&transaction)?;
        let mut changes = TagChanges::default();
        if affected_rows > 0 {
            for tag in &add {
                let row_ids = {
                    let mut statement = transaction.prepare(&format!(
                        "SELECT target.id FROM {TARGET_ROWS_TABLE} AS target
                         WHERE NOT EXISTS (
                             SELECT 1 FROM row_tags JOIN tags ON tags.id = row_tags.tag_id
                             WHERE row_tags.row_id = target.id AND tags.name = ?1 COLLATE BINARY
                         ) ORDER BY target.id"
                    ))?;
                    statement
                        .query_map([tag], |row| row.get(0))?
                        .collect::<Result<Vec<i64>, _>>()?
                };
                add_tags(&transaction, std::slice::from_ref(tag))?;
                if !row_ids.is_empty() {
                    changes.added.push(TagAssociationChange {
                        tag: tag.clone(),
                        row_ids,
                    });
                }
            }
            for tag in &remove {
                let row_ids = {
                    let mut statement = transaction.prepare(&format!(
                        "SELECT row_tags.row_id FROM row_tags
                         JOIN tags ON tags.id = row_tags.tag_id
                         JOIN {TARGET_ROWS_TABLE} AS target ON target.id = row_tags.row_id
                         WHERE tags.name = ?1 COLLATE BINARY ORDER BY row_tags.row_id"
                    ))?;
                    statement
                        .query_map([tag], |row| row.get(0))?
                        .collect::<Result<Vec<i64>, _>>()?
                };
                remove_tags(&transaction, std::slice::from_ref(tag))?;
                if !row_ids.is_empty() {
                    changes.removed.push(TagAssociationChange {
                        tag: tag.clone(),
                        row_ids,
                    });
                }
            }
        }
        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        Ok(TagEditResult {
            affected_rows,
            changes,
        })
    }

    /// Undo/redo only the recorded associations; unrelated tags/text/groups survive.
    /// Missing rows or any write failure roll back the entire history operation.
    pub fn apply_tag_changes(
        &mut self,
        changes: &TagChanges,
        reverse: bool,
    ) -> Result<usize, TagMutationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (remove, add) = if reverse {
            (&changes.added, &changes.removed)
        } else {
            (&changes.removed, &changes.added)
        };
        let mut changed = 0;
        for (items, adding) in [(remove, false), (add, true)] {
            for change in items {
                let tag = change.tag.trim();
                if tag.is_empty() {
                    return Err(TagMutationError::EmptyTagName);
                }
                if change.row_ids.is_empty() {
                    continue;
                }
                create_selection_rows(
                    &transaction,
                    &RowSelection::Explicit {
                        row_ids: change.row_ids.clone(),
                    },
                )?;
                let tags = [tag.to_owned()];
                changed += if adding {
                    add_tags(&transaction, &tags)?
                } else {
                    remove_tags(&transaction, &tags)?
                };
            }
        }
        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        Ok(changed)
    }
}
