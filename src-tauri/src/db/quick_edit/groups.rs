use super::matching::{EvaluatedCondition, matching_rows};
use super::model::{
    QuickEditCondition, QuickEditError, QuickGroupApplyResult, QuickGroupChange, QuickGroupPreview,
};
use super::{PREVIEW_SAMPLE_LIMIT, row_count};
use crate::db::{Database, DatabaseError};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::HashSet;

impl Database {
    pub fn preview_quick_group(
        &self,
        condition: &QuickEditCondition,
        group_id: i64,
        only_ungrouped: bool,
    ) -> Result<QuickGroupPreview, QuickEditError> {
        let condition = EvaluatedCondition::new(condition)?;
        let group_name = validated_group_name(&self.connection, group_id)?;
        let scanned_rows = row_count(&self.connection)?;
        let matched_rows = matching_rows(&self.connection, &condition)?;
        let skipped_grouped_rows = if only_ungrouped {
            u64::try_from(
                matched_rows
                    .iter()
                    .filter(|row| row.group_id.is_some())
                    .count(),
            )
            .map_err(|_| DatabaseError::CountOverflow)?
        } else {
            0
        };
        let eligible_rows = matched_rows
            .iter()
            .filter(|row| !only_ungrouped || row.group_id.is_none())
            .collect::<Vec<_>>();
        let rows_needing_changes = u64::try_from(
            eligible_rows
                .iter()
                .filter(|row| row.group_id != Some(group_id))
                .count(),
        )
        .map_err(|_| DatabaseError::CountOverflow)?;
        let matched_row_count =
            u64::try_from(matched_rows.len()).map_err(|_| DatabaseError::CountOverflow)?;
        let eligible_row_count =
            u64::try_from(eligible_rows.len()).map_err(|_| DatabaseError::CountOverflow)?;

        Ok(QuickGroupPreview {
            scanned_rows,
            matched_rows: matched_row_count,
            rows_needing_changes,
            already_in_group_rows: eligible_row_count - rows_needing_changes,
            skipped_grouped_rows,
            only_ungrouped,
            sample_row_ids: eligible_rows
                .into_iter()
                .map(|row| row.id)
                .take(PREVIEW_SAMPLE_LIMIT)
                .collect(),
            normalized_tokens: condition.tokens,
            target_group_id: group_id,
            target_group_name: group_name,
        })
    }

    pub fn apply_quick_group(
        &mut self,
        condition: &QuickEditCondition,
        group_id: i64,
        only_ungrouped: bool,
    ) -> Result<QuickGroupApplyResult, QuickEditError> {
        let condition = EvaluatedCondition::new(condition)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validated_group_name(&transaction, group_id)?;
        let scanned_rows = row_count(&transaction)?;
        let matched_rows = matching_rows(&transaction, &condition)?;
        let skipped_grouped_rows = if only_ungrouped {
            u64::try_from(
                matched_rows
                    .iter()
                    .filter(|row| row.group_id.is_some())
                    .count(),
            )
            .map_err(|_| DatabaseError::CountOverflow)?
        } else {
            0
        };
        let changes = matched_rows
            .iter()
            .filter(|row| {
                row.group_id != Some(group_id) && (!only_ungrouped || row.group_id.is_none())
            })
            .map(|row| QuickGroupChange {
                row_id: row.id,
                previous_group_id: row.group_id,
                target_group_id: group_id,
            })
            .collect::<Vec<_>>();

        let mut update = transaction.prepare("UPDATE rows SET group_id = ?2 WHERE id = ?1")?;
        for change in &changes {
            update.execute(params![change.row_id, group_id])?;
        }
        drop(update);
        transaction.commit()?;
        self.bump_data_version();

        Ok(QuickGroupApplyResult {
            scanned_rows,
            matched_rows: u64::try_from(matched_rows.len())
                .map_err(|_| DatabaseError::CountOverflow)?,
            changed_rows: u64::try_from(changes.len()).map_err(|_| DatabaseError::CountOverflow)?,
            skipped_grouped_rows,
            only_ungrouped,
            changes,
        })
    }

    pub fn revert_quick_group_changes(
        &mut self,
        changes: &[QuickGroupChange],
    ) -> Result<u64, QuickEditError> {
        self.mutate_quick_group_changes(changes, false)
    }

    pub fn reapply_quick_group_changes(
        &mut self,
        changes: &[QuickGroupChange],
    ) -> Result<u64, QuickEditError> {
        self.mutate_quick_group_changes(changes, true)
    }

    fn mutate_quick_group_changes(
        &mut self,
        changes: &[QuickGroupChange],
        reapply: bool,
    ) -> Result<u64, QuickEditError> {
        let changes = normalize_group_changes(changes)?;
        if changes.is_empty() {
            return Ok(0);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let mut group_ids = HashSet::new();
        for change in &changes {
            if reapply {
                group_ids.insert(change.target_group_id);
            } else if let Some(group_id) = change.previous_group_id {
                group_ids.insert(group_id);
            }
            let row_exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM rows WHERE id = ?1)",
                [change.row_id],
                |row| row.get(0),
            )?;
            if !row_exists {
                return Err(QuickEditError::UnknownRow(change.row_id));
            }
        }
        for group_id in group_ids {
            validated_group_name(&transaction, group_id)?;
        }

        let mut update = transaction.prepare(
            "UPDATE rows SET group_id = ?2
             WHERE id = ?1 AND group_id IS NOT ?2",
        )?;
        let mut changed = 0_u64;
        for change in &changes {
            let group_id = if reapply {
                Some(change.target_group_id)
            } else {
                change.previous_group_id
            };
            changed += u64::try_from(update.execute(params![change.row_id, group_id])?)
                .map_err(|_| DatabaseError::CountOverflow)?;
        }
        drop(update);
        transaction.commit()?;
        self.bump_data_version();
        Ok(changed)
    }
}

fn validated_group_name(connection: &Connection, group_id: i64) -> Result<String, QuickEditError> {
    if group_id <= 0 {
        return Err(QuickEditError::InvalidGroupId(group_id));
    }
    connection
        .query_row("SELECT name FROM groups WHERE id = ?1", [group_id], |row| {
            row.get(0)
        })
        .optional()?
        .ok_or_else(|| DatabaseError::GroupNotFound(group_id).into())
}
fn normalize_group_changes(
    changes: &[QuickGroupChange],
) -> Result<Vec<QuickGroupChange>, QuickEditError> {
    let mut seen = HashSet::with_capacity(changes.len());
    let mut normalized = Vec::with_capacity(changes.len());
    for change in changes {
        if change.row_id <= 0 {
            return Err(QuickEditError::InvalidRowId(change.row_id));
        }
        if change.target_group_id <= 0 {
            return Err(QuickEditError::InvalidGroupId(change.target_group_id));
        }
        if change
            .previous_group_id
            .is_some_and(|group_id| group_id <= 0)
        {
            return Err(QuickEditError::InvalidGroupId(
                change.previous_group_id.unwrap_or_default(),
            ));
        }
        if seen.insert(change.row_id) {
            normalized.push(change.clone());
        }
    }
    Ok(normalized)
}
