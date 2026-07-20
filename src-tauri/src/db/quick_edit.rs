use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::tags::normalize_tags;
use super::{Database, DatabaseError};

const PREVIEW_SAMPLE_LIMIT: usize = 12;

/// 快速编辑的匹配字段。当前前端固定使用正向提示词和角色提示词，
/// 后续提示词替换等动作可以复用同一条件结构。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QuickEditPromptField {
    PositivePrompt,
    CharacterPrompt,
    NegativePrompt,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickEditCondition {
    pub fields: Vec<QuickEditPromptField>,
    pub required_tokens: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickTagPreview {
    pub scanned_rows: u64,
    pub matched_rows: u64,
    pub rows_needing_changes: u64,
    pub already_tagged_rows: u64,
    pub associations_to_add: u64,
    pub sample_row_ids: Vec<i64>,
    pub normalized_tokens: Vec<String>,
    pub normalized_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickTagAssociation {
    pub row_id: i64,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickTagApplyResult {
    pub scanned_rows: u64,
    pub matched_rows: u64,
    pub changed_rows: u64,
    pub associations_changed: u64,
    pub changes: Vec<QuickTagAssociation>,
}

#[derive(Debug, Error)]
pub enum QuickEditError {
    #[error("数据库操作失败: {0}")]
    Database(#[from] DatabaseError),
    #[error("至少需要选择一个提示词字段")]
    EmptyFields,
    #[error("至少需要输入一个提示词条件")]
    EmptyCondition,
    #[error("提示词条件不能包含逗号或换行，请将每项条件分开输入: {0}")]
    InvalidConditionToken(String),
    #[error("至少需要选择一个目标 Tag")]
    EmptyTags,
    #[error("Tag 不存在: {0:?}")]
    UnknownTags(Vec<String>),
    #[error("图片记录不存在: {0}")]
    UnknownRow(i64),
    #[error("图片 ID 必须为正整数: {0}")]
    InvalidRowId(i64),
}

impl From<rusqlite::Error> for QuickEditError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(DatabaseError::Sqlite(error))
    }
}

#[derive(Debug)]
struct EvaluatedCondition {
    fields: HashSet<QuickEditPromptField>,
    tokens: Vec<String>,
}

impl EvaluatedCondition {
    fn new(condition: &QuickEditCondition) -> Result<Self, QuickEditError> {
        let fields = condition.fields.iter().copied().collect::<HashSet<_>>();
        if fields.is_empty() {
            return Err(QuickEditError::EmptyFields);
        }

        let mut seen = HashSet::with_capacity(condition.required_tokens.len());
        let mut tokens = Vec::with_capacity(condition.required_tokens.len());
        for raw in &condition.required_tokens {
            if raw.contains([',', '\n', '\r']) {
                return Err(QuickEditError::InvalidConditionToken(raw.clone()));
            }
            let token = normalize_prompt_token(raw);
            if !token.is_empty() && seen.insert(token.clone()) {
                tokens.push(token);
            }
        }
        if tokens.is_empty() {
            return Err(QuickEditError::EmptyCondition);
        }

        Ok(Self { fields, tokens })
    }

    fn matches(
        &self,
        positive: Option<&str>,
        character: Option<&str>,
        negative: Option<&str>,
    ) -> bool {
        let mut available = HashSet::new();
        if self.fields.contains(&QuickEditPromptField::PositivePrompt) {
            collect_prompt_tokens(positive, &mut available);
        }
        if self.fields.contains(&QuickEditPromptField::CharacterPrompt) {
            collect_prompt_tokens(character, &mut available);
        }
        if self.fields.contains(&QuickEditPromptField::NegativePrompt) {
            collect_prompt_tokens(negative, &mut available);
        }
        self.tokens.iter().all(|token| available.contains(token))
    }
}

impl Database {
    pub fn preview_quick_tag(
        &self,
        condition: &QuickEditCondition,
        tags: &[String],
    ) -> Result<QuickTagPreview, QuickEditError> {
        let condition = EvaluatedCondition::new(condition)?;
        let tags = validated_tags(&self.connection, tags)?;
        let scanned_rows = row_count(&self.connection)?;
        let matched_row_ids = matching_row_ids(&self.connection, &condition)?;
        let tag_ids = tag_ids(&self.connection, &tags)?;
        let existing = existing_associations(&self.connection, &tag_ids)?;

        let mut rows_needing_changes = 0_u64;
        let mut associations_to_add = 0_u64;
        for row_id in &matched_row_ids {
            let missing = tag_ids
                .iter()
                .filter(|tag_id| !existing.contains(&(*row_id, **tag_id)))
                .count();
            if missing > 0 {
                rows_needing_changes += 1;
                associations_to_add +=
                    u64::try_from(missing).map_err(|_| DatabaseError::CountOverflow)?;
            }
        }
        let matched_rows =
            u64::try_from(matched_row_ids.len()).map_err(|_| DatabaseError::CountOverflow)?;

        Ok(QuickTagPreview {
            scanned_rows,
            matched_rows,
            rows_needing_changes,
            already_tagged_rows: matched_rows - rows_needing_changes,
            associations_to_add,
            sample_row_ids: matched_row_ids
                .into_iter()
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
        let matched_row_ids = matching_row_ids(&transaction, &condition)?;
        let tag_ids = tag_ids(&transaction, &tags)?;

        let mut insert = transaction
            .prepare("INSERT OR IGNORE INTO row_tags(row_id, tag_id) VALUES (?1, ?2)")?;
        let mut changed_rows = HashSet::new();
        let mut changes = Vec::new();
        for row_id in &matched_row_ids {
            for (tag, tag_id) in tags.iter().zip(&tag_ids) {
                if insert.execute(params![row_id, tag_id])? > 0 {
                    changed_rows.insert(*row_id);
                    changes.push(QuickTagAssociation {
                        row_id: *row_id,
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
            matched_rows: u64::try_from(matched_row_ids.len())
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

fn matching_row_ids(
    connection: &Connection,
    condition: &EvaluatedCondition,
) -> Result<Vec<i64>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT id, positive_prompt, character_prompt, negative_prompt
         FROM rows
         ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut matched = Vec::new();
    for row in rows {
        let (row_id, positive, character, negative) = row?;
        if condition.matches(
            positive.as_deref(),
            character.as_deref(),
            negative.as_deref(),
        ) {
            matched.push(row_id);
        }
    }
    Ok(matched)
}

fn row_count(connection: &Connection) -> Result<u64, QuickEditError> {
    let count: i64 = connection.query_row("SELECT COUNT(*) FROM rows", [], |row| row.get(0))?;
    u64::try_from(count).map_err(|_| DatabaseError::CountOverflow.into())
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

fn collect_prompt_tokens(prompt: Option<&str>, output: &mut HashSet<String>) {
    let Some(prompt) = prompt else {
        return;
    };
    for fragment in prompt.split([',', '\n', '\r']) {
        let token = normalize_prompt_token(fragment);
        if !token.is_empty() {
            output.insert(token);
        }
    }
}

/// 严格提示词匹配只规范化大小写和常见 NovelAI 权重外壳。
/// 内部空格、下划线及其它字符保持不变。
fn normalize_prompt_token(raw: &str) -> String {
    let mut token = raw.trim();
    loop {
        let before = token;

        if let Some(stripped) = token.strip_suffix("::") {
            token = stripped.trim();
        }

        if let Some(index) = token.find("::") {
            let prefix = token[..index].trim();
            if prefix.is_empty() || prefix.parse::<f32>().is_ok() {
                token = token[index + 2..].trim();
            }
        }

        if let Some((open, close)) = token.chars().next().zip(token.chars().next_back())
            && matches!((open, close), ('(', ')') | ('{', '}') | ('[', ']'))
        {
            let start = open.len_utf8();
            let end = token.len() - close.len_utf8();
            token = token[start..end].trim();
        }

        if let Some(index) = token.rfind(':') {
            let weight = token[index + 1..].trim();
            if !weight.is_empty() && weight.parse::<f32>().is_ok() {
                token = token[..index].trim();
            }
        }

        if token == before {
            break;
        }
    }
    token.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        NewRow,
        test_support::{append_rows, database_with_rows},
    };

    fn prompt_condition(tokens: &[&str]) -> QuickEditCondition {
        QuickEditCondition {
            fields: vec![
                QuickEditPromptField::PositivePrompt,
                QuickEditPromptField::CharacterPrompt,
            ],
            required_tokens: tokens.iter().map(|token| (*token).to_owned()).collect(),
        }
    }

    #[test]
    fn strict_matching_ignores_only_case_and_weight_syntax() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[
                NewRow {
                    source_ordinal: 2,
                    identity: "one".into(),
                    positive_prompt: Some("genshin, masterpiece".into()),
                    character_prompt: Some("1.2::{HuTao}::".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 3,
                    identity: "two".into(),
                    positive_prompt: Some("genshin impact, hutao".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 4,
                    identity: "three".into(),
                    positive_prompt: Some("genshin, hu_tao".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 5,
                    identity: "four".into(),
                    positive_prompt: Some("[HUTAO], {GENSHIN:1.1}".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 6,
                    identity: "five".into(),
                    positive_prompt: Some("genshin".into()),
                    negative_prompt: Some("hutao".into()),
                    ..NewRow::default()
                },
            ],
        );
        database.create_tag("原神").unwrap();

        let preview = database
            .preview_quick_tag(&prompt_condition(&["genshin", "hutao"]), &["原神".into()])
            .unwrap();

        assert_eq!(preview.scanned_rows, 5);
        assert_eq!(preview.matched_rows, 2);
        assert_eq!(preview.sample_row_ids, vec![1, 4]);
        assert_eq!(preview.normalized_tokens, vec!["genshin", "hutao"]);
    }

    #[test]
    fn preview_apply_revert_and_reapply_preserve_preexisting_tags() {
        let mut database = database_with_rows(3);
        database.create_tag("原神").unwrap();
        database.create_tag("胡桃").unwrap();
        database
            .add_tags_to_rows(&[1], &["原神".into(), "胡桃".into()])
            .unwrap();
        database.add_tags_to_rows(&[2], &["原神".into()]).unwrap();
        database
            .connection
            .execute(
                "UPDATE rows SET positive_prompt = 'genshin', character_prompt = 'hutao'
                 WHERE id IN (1, 2)",
                [],
            )
            .unwrap();

        let condition = prompt_condition(&["Genshin", "1.1::HUTAO::"]);
        let tags = vec!["原神".into(), "胡桃".into()];
        let preview = database.preview_quick_tag(&condition, &tags).unwrap();
        assert_eq!(preview.matched_rows, 2);
        assert_eq!(preview.rows_needing_changes, 1);
        assert_eq!(preview.already_tagged_rows, 1);
        assert_eq!(preview.associations_to_add, 1);

        let applied = database.apply_quick_tag(&condition, &tags).unwrap();
        assert_eq!(applied.matched_rows, 2);
        assert_eq!(applied.changed_rows, 1);
        assert_eq!(applied.associations_changed, 1);
        assert_eq!(
            applied.changes,
            vec![QuickTagAssociation {
                row_id: 2,
                tag: "胡桃".into(),
            }]
        );

        assert_eq!(
            database.revert_quick_tag_changes(&applied.changes).unwrap(),
            1
        );
        assert_eq!(
            database.get_rows_by_ids(&[1]).unwrap()[0].tags,
            vec!["原神", "胡桃"]
        );
        assert_eq!(
            database.get_rows_by_ids(&[2]).unwrap()[0].tags,
            vec!["原神"]
        );

        assert_eq!(
            database
                .reapply_quick_tag_changes(&applied.changes)
                .unwrap(),
            1
        );
        assert_eq!(
            database.get_rows_by_ids(&[2]).unwrap()[0].tags,
            vec!["原神", "胡桃"]
        );
    }

    #[test]
    fn unknown_target_tag_rolls_back_without_changes() {
        let mut database = database_with_rows(1);
        database
            .connection
            .execute("UPDATE rows SET positive_prompt = 'genshin, hutao'", [])
            .unwrap();

        let result =
            database.apply_quick_tag(&prompt_condition(&["genshin", "hutao"]), &["不存在".into()]);

        assert!(matches!(result, Err(QuickEditError::UnknownTags(_))));
        assert!(database.get_rows_by_ids(&[1]).unwrap()[0].tags.is_empty());
    }

    #[test]
    fn normalization_keeps_spaces_and_underscores_distinct() {
        assert_eq!(normalize_prompt_token("Hu Tao"), "hu tao");
        assert_eq!(normalize_prompt_token("hu_tao"), "hu_tao");
        assert_eq!(normalize_prompt_token("1.2::{{HUTAO:1.1}}::"), "hutao");
    }
}
