//! Agent persistence: conversations, dry-run previews and undoable operations.
mod apply;
mod changes;
mod read;
mod target;

pub use apply::{OperationRecord, UndoOutcome};
pub use changes::{Action, RowChange};
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

    pub fn agent_messages(&self, conversation_id: &str) -> Res<Vec<Message>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, created_at, kind, content FROM agent_messages
                 WHERE conversation_id = ?1 ORDER BY id",
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

    pub fn agent_push_message(&self, conversation_id: &str, kind: &str, content: &Value) -> Res<i64> {
        self.connection
            .execute(
                "INSERT INTO agent_messages(conversation_id, kind, content) VALUES (?1, ?2, ?3)",
                params![conversation_id, kind, content.to_string()],
            )
            .map_err(s)?;
        let id = self.connection.last_insert_rowid();
        self.connection
            .execute(
                "UPDATE agent_conversations
                 SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
                [conversation_id],
            )
            .map_err(s)?;
        Ok(id)
    }
}
