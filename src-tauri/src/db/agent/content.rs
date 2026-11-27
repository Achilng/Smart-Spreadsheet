//! Bookkeeping for agent edits to materials and prompt docs. The edits themselves need image
//! decoding and doc files, so they run in the app layer; this keeps their previews and records.
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::Value;

use super::apply::{OperationRecord, prune_operations, store_preview_value};
use super::{Res, s};
use crate::db::Database;

/// Operation kinds whose undo the app layer handles.
pub fn is_content_kind(kind: &str) -> bool {
    kind.starts_with("material_") || kind.starts_with("doc_")
}

pub struct StoredPreview {
    pub conversation_id: String,
    pub plan: Value,
    pub report: Value,
    pub used: bool,
}

impl Database {
    pub fn agent_store_content_preview(&self, conversation_id: &str, preview_id: &str, plan: &Value, report: &Value) -> Res<()> {
        store_preview_value(self, conversation_id, preview_id, plan, "[]", report)
    }

    pub fn agent_stored_preview(&self, preview_id: &str) -> Res<Option<StoredPreview>> {
        self.connection
            .query_row(
                "SELECT conversation_id, plan, report, used FROM agent_previews WHERE id = ?1",
                [preview_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, bool>(3)?)),
            )
            .optional()
            .map_err(s)?
            .map(|(conversation_id, plan, report, used)| {
                Ok(StoredPreview {
                    conversation_id,
                    plan: serde_json::from_str(&plan).map_err(s)?,
                    report: serde_json::from_str(&report).map_err(s)?,
                    used,
                })
            })
            .transpose()
    }

    /// Record a finished content edit; `preview_id` (if any) is marked used in the same transaction.
    pub fn agent_record_content(&mut self, conversation_id: &str, kind: &str, title: &str, report: &Value, preview_id: Option<&str>) -> Res<OperationRecord> {
        let transaction = self.connection.transaction().map_err(s)?;
        transaction
            .execute(
                "INSERT INTO agent_operations(conversation_id, kind, title, report, changes) VALUES (?1, ?2, ?3, ?4, '[]')",
                params![conversation_id, kind, title, report.to_string()],
            )
            .map_err(s)?;
        let id = transaction.last_insert_rowid();
        if let Some(preview_id) = preview_id {
            transaction.execute("UPDATE agent_previews SET used = 1 WHERE id = ?1", [preview_id]).map_err(s)?;
        }
        prune_operations(&transaction)?;
        transaction.commit().map_err(s)?;
        self.agent_operation(id)?.ok_or_else(|| "读取操作记录失败".into())
    }

    pub fn agent_finish_content_undo(&self, operation_id: i64, message: &str) -> Res<()> {
        self.mark_undone(operation_id, message)
    }

    /// Of `tags` (as a material would store them), the ones no image or material uses yet.
    pub fn agent_new_tags(&self, tags: &[String]) -> Res<Vec<String>> {
        let mut missing = Vec::new();
        for tag in crate::db::tags::normalize_tags(tags) {
            let exists: Option<i64> = self.connection.query_row("SELECT id FROM tags WHERE name = ?1", [&tag], |row| row.get(0)).optional().map_err(s)?;
            if exists.is_none() {
                missing.push(tag);
            }
        }
        Ok(missing)
    }

    /// Remove tags an undone edit created, unless something else now uses them.
    pub fn agent_drop_unused_tags(&mut self, tags: &[String]) -> Res<()> {
        let transaction: Transaction<'_> = self.connection.transaction().map_err(s)?;
        for tag in tags {
            transaction
                .execute(
                    "DELETE FROM tags WHERE name = ?1
                     AND NOT EXISTS (SELECT 1 FROM row_tags WHERE row_tags.tag_id = tags.id)
                     AND NOT EXISTS (SELECT 1 FROM material_tags WHERE material_tags.tag_id = tags.id)",
                    [tag],
                )
                .map_err(s)?;
        }
        transaction.commit().map_err(s)?;
        Ok(())
    }
}
