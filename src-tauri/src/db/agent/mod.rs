//! Agent persistence: conversations, dry-run previews and undoable operations.
mod apply;
mod changes;
mod library;
mod model_badge;
mod read;
mod target;

pub use apply::{OperationRecord, RulePatch, UndoOutcome};
pub use changes::{Action, RowChange};
pub use library::LibraryOp;
pub use model_badge::{comparison_tier, model_label};
pub(crate) use read::{COUNT_FIELDS, CountSort, DEFAULT_PROMPT_STATS_LIMIT, MAX_PROMPT_STATS_LIMIT, PromptStatsSort, ROW_FIELDS, RowSort};
pub(crate) use changes::PLACEHOLDERS;
pub(crate) use target::local_time;
pub use target::{MainView, Target};

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

use super::Database;

pub type Res<T> = Result<T, String>;
pub(crate) fn s(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: i64,
    pub created_at: String,
    pub kind: String,
    pub content: Value,
}

impl Database {
    pub fn agent_conversations(&self) -> Res<Vec<Conversation>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, title, created_at, updated_at FROM agent_conversations
                 ORDER BY updated_at DESC, id DESC",
            )
            .map_err(s)?;
        let rows = statement
            .query_map([], |row| {
                Ok(Conversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })
            .map_err(s)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(s)?;
        Ok(rows)
    }

    pub fn agent_conversation(&self, id: &str) -> Res<Option<Conversation>> {
        self.connection
            .query_row(
                "SELECT id, title, created_at, updated_at FROM agent_conversations WHERE id = ?1",
                [id],
                |row| {
                    Ok(Conversation {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        created_at: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(s)
    }

    pub fn agent_create_conversation(&self, id: &str, title: &str) -> Res<Conversation> {
        self.connection
            .execute(
                "INSERT INTO agent_conversations(id, title) VALUES (?1, ?2)",
                params![id, title],
            )
            .map_err(s)?;
        self.agent_conversation(id)?.ok_or_else(|| "创建对话失败".into())
    }

    pub fn agent_rename_conversation(&self, id: &str, title: &str) -> Res<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err("对话名称不能为空".into());
        }
        self.connection
            .execute(
                "UPDATE agent_conversations SET title = ?2 WHERE id = ?1",
                params![id, title],
            )
            .map_err(s)?;
        Ok(())
    }

    pub fn agent_delete_conversation(&self, id: &str) -> Res<()> {
        self.connection
            .execute("DELETE FROM agent_conversations WHERE id = ?1", [id])
            .map_err(s)?;
        Ok(())
    }

    /// Remove an error notice the user closed. Errors are never sent to the model, so nothing else changes.
    pub fn agent_dismiss_error(&self, conversation_id: &str, message_id: i64) -> Res<()> {
        self.connection
            .execute(
                "DELETE FROM agent_messages WHERE id = ?1 AND conversation_id = ?2 AND kind = 'error'
                   AND COALESCE(json_extract(content, '$.stopped'), 0) = 0",
                params![message_id, conversation_id],
            )
            .map_err(s)?;
        Ok(())
    }

    pub fn agent_messages(&self, conversation_id: &str) -> Res<Vec<Message>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, created_at, kind,
                        CASE WHEN kind = 'user' AND json_valid(content)
                             THEN json_remove(content, '$.attachmentData') ELSE content END
                 FROM agent_messages WHERE conversation_id = ?1 ORDER BY id",
            )
            .map_err(s)?;
        let rows = statement
            .query_map([conversation_id], |row| {
                let content: String = row.get(3)?;
                Ok(Message {
                    id: row.get(0)?,
                    created_at: row.get(1)?,
                    kind: row.get(2)?,
                    content: serde_json::from_str(&content).unwrap_or(Value::Null),
                })
            })
            .map_err(s)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(s)?;
        Ok(rows)
    }

    /// Fetch only replayable slots, even in a mixed legacy message with inline images.
    pub(crate) fn agent_document_attachment(&self, conversation_id: &str, message_id: i64, slot: usize) -> Res<String> {
        self.connection.query_row(
            "SELECT json_extract(content, ?3) FROM agent_messages
             WHERE conversation_id = ?1 AND id = ?2 AND kind = 'user'
               AND json_extract(content, ?4) IN ('text/plain','application/pdf')",
            params![conversation_id, message_id, format!("$.attachmentData[{slot}]"), format!("$.attachments[{slot}].mime")],
            |row| row.get(0),
        ).map_err(s)
    }

    /// Only return the requested image, never another conversation's bytes or a filesystem path.
    pub(crate) fn agent_image_attachment(&self, conversation_id: &str, message_id: i64, number: u8) -> Res<(String, String)> {
        if !(1..=4).contains(&number) { return Err("附件序号应在 1 至 4 之间".into()); }
        let slot = number - 1;
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let image = self.connection.query_row(
            "SELECT json_extract(m.content, ?4), a.bytes FROM agent_image_attachments a
             JOIN agent_messages m ON m.id = a.message_id
             WHERE m.conversation_id = ?1 AND m.kind = 'user' AND m.id = ?2 AND a.number = ?3",
            params![conversation_id, message_id, number, format!("$.attachments[{slot}].name")],
            |row| Ok((row.get(0)?, STANDARD.encode(row.get_ref(1)?.as_blob()?))),
        ).optional().map_err(s)?;
        if let Some(image) = image { return Ok(image); }
        // Pre-v30 messages stored original images as inline base64.
        self.connection.query_row(
            "SELECT json_extract(content, ?3 || '.name'), json_extract(content, ?4)
             FROM agent_messages WHERE conversation_id = ?1 AND id = ?2 AND kind = 'user'
             AND json_extract(content, ?3 || '.mime') IN ('image/png','image/jpeg','image/webp')
             AND json_type(content, ?4) = 'text'",
            params![conversation_id, message_id, format!("$.attachments[{slot}]"), format!("$.attachmentData[{slot}]")],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(s)?.ok_or_else(|| "当前对话中找不到这个图片附件，请核对消息编号和附件序号".into())
    }

    pub fn agent_push_message(&self, conversation_id: &str, kind: &str, content: &Value) -> Res<i64> {
        self.agent_push_with_images(conversation_id, kind, content, &[])
    }

    pub(crate) fn agent_push_with_images(&self, conversation_id: &str, kind: &str, content: &Value, images: &[(u8, Vec<u8>)]) -> Res<i64> {
        let tx = self.connection.unchecked_transaction().map_err(s)?;
        tx.execute(
                "INSERT INTO agent_messages(conversation_id, kind, content) VALUES (?1, ?2, ?3)",
                params![conversation_id, kind, content.to_string()],
            )
            .map_err(s)?;
        let id = tx.last_insert_rowid();
        for (number, bytes) in images {
            tx.execute("INSERT INTO agent_image_attachments(message_id, number, bytes) VALUES (?1, ?2, ?3)",
                params![id, number, bytes]).map_err(s)?;
        }
        tx.execute(
                "UPDATE agent_conversations
                 SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
                [conversation_id],
            )
            .map_err(s)?;
        tx.commit().map_err(s)?;
        Ok(id)
    }
}
