use std::collections::HashSet;

use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;

use super::{Database, DatabaseError};

/// 前端操作历史保存的行可变字段快照。
/// 图片来源/身份键等不可编辑字段不在撤销范围内。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutableRowState {
    pub row_id: i64,
    pub positive_prompt: Option<String>,
    pub character_prompt: Option<String>,
    pub negative_prompt: Option<String>,
    pub note: Option<String>,
    pub artists: Option<String>,
    pub tags: Vec<String>,
    pub group_id: Option<i64>,
}

impl Database {
    /// 在单个事务中恢复一批行的全部可变状态，供撤销/重做使用。
    pub fn restore_mutable_row_states(
        &mut self,
        states: &[MutableRowState],
    ) -> Result<u64, DatabaseError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut seen = HashSet::with_capacity(states.len());
        let mut restored = 0_u64;

        for state in states {
            if state.row_id <= 0 || !seen.insert(state.row_id) {
                return Err(DatabaseError::RowNotFound(state.row_id));
            }
            let updated = transaction.execute(
                "UPDATE rows SET
                    positive_prompt = ?2,
                    character_prompt = ?3,
                    negative_prompt = ?4,
                    note = ?5,
                    artists = ?6,
                    group_id = ?7
                 WHERE id = ?1",
                params![
                    state.row_id,
                    state.positive_prompt,
                    state.character_prompt,
                    state.negative_prompt,
                    state.note,
                    state.artists,
                    state.group_id,
                ],
            )?;
            if updated == 0 {
                return Err(DatabaseError::RowNotFound(state.row_id));
            }

            transaction.execute("DELETE FROM row_tags WHERE row_id = ?1", [state.row_id])?;
            let mut unique_tags = HashSet::with_capacity(state.tags.len());
            for raw_tag in &state.tags {
                let tag = raw_tag.trim();
                if tag.is_empty() || !unique_tags.insert(tag.to_owned()) {
                    continue;
                }
                transaction.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [tag])?;
                let tag_id = transaction
                    .query_row(
                        "SELECT id FROM tags WHERE name = ?1 COLLATE BINARY",
                        [tag],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?
                    .ok_or_else(|| {
                        DatabaseError::IntegrityCheckFailed(format!("撤销时无法恢复 Tag：{tag}"))
                    })?;
                transaction.execute(
                    "INSERT INTO row_tags(row_id, tag_id) VALUES (?1, ?2)",
                    params![state.row_id, tag_id],
                )?;
            }
            restored += 1;
        }

        transaction.commit()?;
        Ok(restored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{RowSelection, test_support::database_with_rows};

    #[test]
    fn restores_fields_tags_and_group_in_one_transaction() {
        let mut database = database_with_rows(2);
        let old_group = database.create_group("old group").unwrap();
        let new_group = database.create_group("new group").unwrap();
        database.create_tag("old tag").unwrap();
        database.create_tag("new tag").unwrap();
        database.set_tags_for_row(1, &["new tag".into()]).unwrap();
        database
            .assign_rows_to_group(&RowSelection::Explicit { row_ids: vec![1] }, new_group.id)
            .unwrap();

        let restored = database
            .restore_mutable_row_states(&[MutableRowState {
                row_id: 1,
                positive_prompt: Some("old positive".into()),
                character_prompt: Some("old character".into()),
                negative_prompt: Some("old negative".into()),
                note: Some("old note".into()),
                artists: Some("old artist".into()),
                tags: vec!["old tag".into()],
                group_id: Some(old_group.id),
            }])
            .unwrap();

        assert_eq!(restored, 1);
        let row = database.get_rows_by_ids(&[1]).unwrap().remove(0);
        assert_eq!(row.positive_prompt.as_deref(), Some("old positive"));
        assert_eq!(row.character_prompt.as_deref(), Some("old character"));
        assert_eq!(row.negative_prompt.as_deref(), Some("old negative"));
        assert_eq!(row.note.as_deref(), Some("old note"));
        assert_eq!(row.artists.as_deref(), Some("old artist"));
        assert_eq!(row.tags, vec!["old tag"]);
        assert_eq!(row.group_id, Some(old_group.id));
    }

    #[test]
    fn rolls_back_all_states_when_any_row_is_missing() {
        let mut database = database_with_rows(1);
        let original = database.get_rows_by_ids(&[1]).unwrap().remove(0);
        let state = |row_id, prompt: &str| MutableRowState {
            row_id,
            positive_prompt: Some(prompt.into()),
            character_prompt: None,
            negative_prompt: None,
            note: None,
            artists: None,
            tags: Vec::new(),
            group_id: None,
        };

        let result = database
            .restore_mutable_row_states(&[state(1, "must roll back"), state(999, "missing")]);

        assert!(matches!(result, Err(DatabaseError::RowNotFound(999))));
        let after = database.get_rows_by_ids(&[1]).unwrap().remove(0);
        assert_eq!(after.positive_prompt, original.positive_prompt);
    }
}
