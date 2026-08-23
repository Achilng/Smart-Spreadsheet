use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::prompt_edit::{
    combined_artists, normalize_artist_name, prefix_artist_tag_in_prompt,
};
use super::tags::normalize_tags;
use super::{Database, DatabaseError};

const PREVIEW_SAMPLE_LIMIT: usize = 12;

/// 快速整理的文本匹配字段。当前前端固定使用全部资料文本区域，
/// 后续提示词替换等动作可以复用同一条件结构。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QuickEditTextField {
    PositivePrompt,
    CharacterPrompt,
    NegativePrompt,
    Artists,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickEditCondition {
    pub fields: Vec<QuickEditTextField>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickGroupPreview {
    pub scanned_rows: u64,
    pub matched_rows: u64,
    pub rows_needing_changes: u64,
    pub already_in_group_rows: u64,
    pub skipped_grouped_rows: u64,
    pub only_ungrouped: bool,
    pub sample_row_ids: Vec<i64>,
    pub normalized_tokens: Vec<String>,
    pub target_group_id: i64,
    pub target_group_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickGroupChange {
    pub row_id: i64,
    pub previous_group_id: Option<i64>,
    pub target_group_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickGroupApplyResult {
    pub scanned_rows: u64,
    pub matched_rows: u64,
    pub changed_rows: u64,
    pub skipped_grouped_rows: u64,
    pub only_ungrouped: bool,
    pub changes: Vec<QuickGroupChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickArtistPrefixPreview {
    pub scanned_rows: u64,
    pub matched_rows: u64,
    pub rows_needing_changes: u64,
    pub prompt_fields_needing_changes: u64,
    pub sample_row_ids: Vec<i64>,
    pub artist_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickArtistPrefixChange {
    pub row_id: i64,
    pub previous_positive_prompt: Option<String>,
    pub new_positive_prompt: Option<String>,
    pub previous_character_prompt: Option<String>,
    pub new_character_prompt: Option<String>,
    pub previous_negative_prompt: Option<String>,
    pub new_negative_prompt: Option<String>,
    pub previous_artists: Option<String>,
    pub new_artists: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickArtistPrefixApplyResult {
    pub scanned_rows: u64,
    pub matched_rows: u64,
    pub changed_rows: u64,
    pub prompt_fields_changed: u64,
    pub changes: Vec<QuickArtistPrefixChange>,
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
    #[error("分组 ID 必须为正整数: {0}")]
    InvalidGroupId(i64),
    #[error("请输入一个画师名")]
    EmptyArtistName,
    #[error("一次只能输入一个画师名，不能包含逗号或换行")]
    InvalidArtistName,
    #[error("至少需要选择一个自动识别出的画师名")]
    EmptyArtistSelection,
    #[error("库内没有明确 artist: 证据的名称: {0:?}")]
    UnknownArtistNames(Vec<String>),
}

impl From<rusqlite::Error> for QuickEditError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(DatabaseError::Sqlite(error))
    }
}

#[derive(Debug)]
struct EvaluatedCondition {
    fields: HashSet<QuickEditTextField>,
    tokens: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MatchingRow {
    id: i64,
    group_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArtistPromptRow {
    id: i64,
    positive_prompt: Option<String>,
    character_prompt: Option<String>,
    negative_prompt: Option<String>,
    artists: Option<String>,
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
        artists: Option<&str>,
        note: Option<&str>,
    ) -> bool {
        let mut available = HashSet::new();
        if self.fields.contains(&QuickEditTextField::PositivePrompt) {
            collect_prompt_tokens(positive, &mut available);
        }
        if self.fields.contains(&QuickEditTextField::CharacterPrompt) {
            collect_prompt_tokens(character, &mut available);
        }
        if self.fields.contains(&QuickEditTextField::NegativePrompt) {
            collect_prompt_tokens(negative, &mut available);
        }
        if self.fields.contains(&QuickEditTextField::Artists) {
            collect_prompt_tokens(artists, &mut available);
        }
        if self.fields.contains(&QuickEditTextField::Note) {
            collect_prompt_tokens(note, &mut available);
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
            changed_rows: u64::try_from(changes.len())
                .map_err(|_| DatabaseError::CountOverflow)?,
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

    pub fn preview_quick_artist_prefix(
        &self,
        artist_name: &str,
    ) -> Result<QuickArtistPrefixPreview, QuickEditError> {
        let artist_name = validated_artist_name(artist_name)?;
        let scanned_rows = row_count(&self.connection)?;
        let changes = artist_prefix_changes(&self.connection, &artist_name)?;
        let changed_rows =
            u64::try_from(changes.len()).map_err(|_| DatabaseError::CountOverflow)?;
        let prompt_fields_needing_changes = changes
            .iter()
            .map(changed_prompt_field_count)
            .sum::<u64>();

        Ok(QuickArtistPrefixPreview {
            scanned_rows,
            matched_rows: changed_rows,
            rows_needing_changes: changed_rows,
            prompt_fields_needing_changes,
            sample_row_ids: changes
                .iter()
                .map(|change| change.row_id)
                .take(PREVIEW_SAMPLE_LIMIT)
                .collect(),
            artist_name,
        })
    }

    pub fn apply_quick_artist_prefix(
        &mut self,
        artist_name: &str,
    ) -> Result<QuickArtistPrefixApplyResult, QuickEditError> {
        let artist_name = validated_artist_name(artist_name)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let scanned_rows = row_count(&transaction)?;
        let changes = artist_prefix_changes(&transaction, &artist_name)?;
        let prompt_fields_changed = changes
            .iter()
            .map(changed_prompt_field_count)
            .sum::<u64>();

        let mut update = transaction.prepare(
            "UPDATE rows
             SET positive_prompt = ?2,
                 character_prompt = ?3,
                 negative_prompt = ?4,
                 artists = ?5,
                 style_signature = ?6
             WHERE id = ?1",
        )?;
        for change in &changes {
            update.execute(params![
                change.row_id,
                &change.new_positive_prompt,
                &change.new_character_prompt,
                &change.new_negative_prompt,
                &change.new_artists,
                crate::pipeline::style_signature_of(change.new_positive_prompt.as_deref()),
            ])?;
        }
        drop(update);
        transaction.commit()?;
        self.bump_data_version();

        let changed_rows =
            u64::try_from(changes.len()).map_err(|_| DatabaseError::CountOverflow)?;
        Ok(QuickArtistPrefixApplyResult {
            scanned_rows,
            matched_rows: changed_rows,
            changed_rows,
            prompt_fields_changed,
            changes,
        })
    }

    pub fn revert_quick_artist_prefix_changes(
        &mut self,
        changes: &[QuickArtistPrefixChange],
    ) -> Result<u64, QuickEditError> {
        self.mutate_quick_artist_prefix_changes(changes, false)
    }

    pub fn reapply_quick_artist_prefix_changes(
        &mut self,
        changes: &[QuickArtistPrefixChange],
    ) -> Result<u64, QuickEditError> {
        self.mutate_quick_artist_prefix_changes(changes, true)
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

    fn mutate_quick_artist_prefix_changes(
        &mut self,
        changes: &[QuickArtistPrefixChange],
        reapply: bool,
    ) -> Result<u64, QuickEditError> {
        let changes = normalize_artist_prefix_changes(changes)?;
        if changes.is_empty() {
            return Ok(0);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut update = transaction.prepare(
            "UPDATE rows
             SET positive_prompt = ?2,
                 character_prompt = ?3,
                 negative_prompt = ?4,
                 artists = ?5,
                 style_signature = ?6
             WHERE id = ?1",
        )?;
        let mut changed = 0_u64;
        for change in &changes {
            let (positive, character, negative, artists) = if reapply {
                (
                    &change.new_positive_prompt,
                    &change.new_character_prompt,
                    &change.new_negative_prompt,
                    &change.new_artists,
                )
            } else {
                (
                    &change.previous_positive_prompt,
                    &change.previous_character_prompt,
                    &change.previous_negative_prompt,
                    &change.previous_artists,
                )
            };
            let updated = update.execute(params![
                change.row_id,
                positive,
                character,
                negative,
                artists,
                crate::pipeline::style_signature_of(positive.as_deref()),
            ])?;
            if updated == 0 {
                return Err(QuickEditError::UnknownRow(change.row_id));
            }
            changed +=
                u64::try_from(updated).map_err(|_| DatabaseError::CountOverflow)?;
        }
        drop(update);
        transaction.commit()?;
        self.bump_data_version();
        Ok(changed)
    }
}

fn validated_artist_name(artist_name: &str) -> Result<String, QuickEditError> {
    let artist_name = normalize_artist_name(artist_name);
    if artist_name.is_empty() {
        return Err(QuickEditError::EmptyArtistName);
    }
    if artist_name.contains([',', '\n', '\r']) {
        return Err(QuickEditError::InvalidArtistName);
    }
    Ok(artist_name.to_owned())
}

fn artist_prefix_changes(
    connection: &Connection,
    artist_name: &str,
) -> Result<Vec<QuickArtistPrefixChange>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT id, positive_prompt, character_prompt, negative_prompt, artists
         FROM rows
         ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(ArtistPromptRow {
            id: row.get(0)?,
            positive_prompt: row.get(1)?,
            character_prompt: row.get(2)?,
            negative_prompt: row.get(3)?,
            artists: row.get(4)?,
        })
    })?;
    let mut changes = Vec::new();
    for row in rows {
        let row = row?;
        if let Some(change) = artist_prefix_change(row, artist_name) {
            changes.push(change);
        }
    }
    Ok(changes)
}

fn artist_prefix_change(
    row: ArtistPromptRow,
    artist_name: &str,
) -> Option<QuickArtistPrefixChange> {
    let positive_rewrite = row
        .positive_prompt
        .as_deref()
        .and_then(|prompt| prefix_artist_tag_in_prompt(prompt, artist_name));
    let character_rewrite = row
        .character_prompt
        .as_deref()
        .and_then(|prompt| prefix_artist_tag_in_prompt(prompt, artist_name));
    let negative_rewrite = row
        .negative_prompt
        .as_deref()
        .and_then(|prompt| prefix_artist_tag_in_prompt(prompt, artist_name));
    let artist_source_changed = positive_rewrite.is_some() || character_rewrite.is_some();

    if positive_rewrite.is_none() && character_rewrite.is_none() && negative_rewrite.is_none() {
        return None;
    }

    let new_positive_prompt = positive_rewrite
        .or_else(|| row.positive_prompt.clone());
    let new_character_prompt = character_rewrite
        .or_else(|| row.character_prompt.clone());
    let new_negative_prompt = negative_rewrite
        .or_else(|| row.negative_prompt.clone());
    let new_artists = if artist_source_changed {
        combined_artists(
            new_positive_prompt.as_deref().unwrap_or(""),
            new_character_prompt.as_deref(),
        )
    } else {
        row.artists.clone()
    };

    Some(QuickArtistPrefixChange {
        row_id: row.id,
        previous_positive_prompt: row.positive_prompt,
        new_positive_prompt,
        previous_character_prompt: row.character_prompt,
        new_character_prompt,
        previous_negative_prompt: row.negative_prompt,
        new_negative_prompt,
        previous_artists: row.artists,
        new_artists,
    })
}

fn changed_prompt_field_count(change: &QuickArtistPrefixChange) -> u64 {
    u64::from(change.previous_positive_prompt != change.new_positive_prompt)
        + u64::from(change.previous_character_prompt != change.new_character_prompt)
        + u64::from(change.previous_negative_prompt != change.new_negative_prompt)
}

fn matching_rows(
    connection: &Connection,
    condition: &EvaluatedCondition,
) -> Result<Vec<MatchingRow>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT id, positive_prompt, character_prompt, negative_prompt, artists, note, group_id
         FROM rows
         ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<i64>>(6)?,
        ))
    })?;
    let mut matched = Vec::new();
    for row in rows {
        let (row_id, positive, character, negative, artists, note, group_id) = row?;
        if condition.matches(
            positive.as_deref(),
            character.as_deref(),
            negative.as_deref(),
            artists.as_deref(),
            note.as_deref(),
        ) {
            matched.push(MatchingRow {
                id: row_id,
                group_id,
            });
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

fn validated_group_name(
    connection: &Connection,
    group_id: i64,
) -> Result<String, QuickEditError> {
    if group_id <= 0 {
        return Err(QuickEditError::InvalidGroupId(group_id));
    }
    connection
        .query_row(
            "SELECT name FROM groups WHERE id = ?1",
            [group_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| DatabaseError::GroupNotFound(group_id).into())
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
        if change.previous_group_id.is_some_and(|group_id| group_id <= 0) {
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

fn normalize_artist_prefix_changes(
    changes: &[QuickArtistPrefixChange],
) -> Result<Vec<QuickArtistPrefixChange>, QuickEditError> {
    let mut seen = HashSet::with_capacity(changes.len());
    let mut normalized = Vec::with_capacity(changes.len());
    for change in changes {
        if change.row_id <= 0 {
            return Err(QuickEditError::InvalidRowId(change.row_id));
        }
        if seen.insert(change.row_id) {
            normalized.push(change.clone());
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

/// 严格提示词匹配会规范化大小写、常见 NovelAI 权重外壳和少量明确的泛用别名。
/// 除别名表外，内部空格、下划线及其它字符保持不变。
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
    let normalized = token.to_lowercase();
    match normalized.as_str() {
        "girl" | "1girl" | "1 girl" => "girl".to_owned(),
        _ => normalized,
    }
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
                QuickEditTextField::PositivePrompt,
                QuickEditTextField::CharacterPrompt,
                QuickEditTextField::NegativePrompt,
                QuickEditTextField::Artists,
                QuickEditTextField::Note,
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
        assert_eq!(preview.matched_rows, 3);
        assert_eq!(preview.sample_row_ids, vec![1, 4, 5]);
        assert_eq!(preview.normalized_tokens, vec!["genshin", "hutao"]);
    }

    #[test]
    fn matching_includes_artists_and_notes() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[
                NewRow {
                    source_ordinal: 2,
                    identity: "artist-and-note".into(),
                    artists: Some("GENSHIN".into()),
                    note: Some("1.1::hutao::".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 3,
                    identity: "note-only".into(),
                    note: Some("genshin, {HUTAO}".into()),
                    ..NewRow::default()
                },
            ],
        );
        database.create_tag("原神").unwrap();

        let preview = database
            .preview_quick_tag(&prompt_condition(&["genshin", "hutao"]), &["原神".into()])
            .unwrap();

        assert_eq!(preview.scanned_rows, 2);
        assert_eq!(preview.matched_rows, 2);
        assert_eq!(preview.sample_row_ids, vec![1, 2]);
    }

    #[test]
    fn girl_aliases_share_one_quick_edit_match_key() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[
                NewRow {
                    source_ordinal: 2,
                    identity: "girl".into(),
                    positive_prompt: Some("best quality, girl".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 3,
                    identity: "one-girl".into(),
                    character_prompt: Some("{1girl}".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 4,
                    identity: "spaced-one-girl".into(),
                    negative_prompt: Some("1.1::1 GIRL::".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 5,
                    identity: "plural".into(),
                    positive_prompt: Some("2girls".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 6,
                    identity: "similar".into(),
                    positive_prompt: Some("girlfriend".into()),
                    ..NewRow::default()
                },
            ],
        );
        database.create_tag("人物").unwrap();

        let preview = database
            .preview_quick_tag(
                &prompt_condition(&["girl", "1girl", "1 girl"]),
                &["人物".into()],
            )
            .unwrap();

        assert_eq!(preview.scanned_rows, 5);
        assert_eq!(preview.matched_rows, 3);
        assert_eq!(preview.sample_row_ids, vec![1, 2, 3]);
        assert_eq!(preview.normalized_tokens, vec!["girl"]);
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
    fn preview_apply_revert_and_reapply_group_restore_each_previous_group() {
        let mut database = database_with_rows(4);
        let previous_group = database.create_group("原分组").unwrap();
        let target_group = database.create_group("目标分组").unwrap();
        database
            .assign_rows_to_group(
                &crate::db::RowSelection::Explicit { row_ids: vec![1] },
                target_group.id,
            )
            .unwrap();
        database
            .assign_rows_to_group(
                &crate::db::RowSelection::Explicit { row_ids: vec![2] },
                previous_group.id,
            )
            .unwrap();
        database
            .connection
            .execute(
                "UPDATE rows SET positive_prompt = 'genshin, hutao' WHERE id IN (1, 2, 3)",
                [],
            )
            .unwrap();

        let condition = prompt_condition(&["genshin", "hutao"]);
        let preview = database
            .preview_quick_group(&condition, target_group.id, false)
            .unwrap();
        assert_eq!(preview.scanned_rows, 4);
        assert_eq!(preview.matched_rows, 3);
        assert_eq!(preview.rows_needing_changes, 2);
        assert_eq!(preview.already_in_group_rows, 1);
        assert_eq!(preview.skipped_grouped_rows, 0);
        assert!(!preview.only_ungrouped);
        assert_eq!(preview.sample_row_ids, vec![1, 2, 3]);

        let applied = database
            .apply_quick_group(&condition, target_group.id, false)
            .unwrap();
        assert_eq!(applied.changed_rows, 2);
        assert_eq!(
            applied.changes,
            vec![
                QuickGroupChange {
                    row_id: 2,
                    previous_group_id: Some(previous_group.id),
                    target_group_id: target_group.id,
                },
                QuickGroupChange {
                    row_id: 3,
                    previous_group_id: None,
                    target_group_id: target_group.id,
                },
            ]
        );
        assert_eq!(
            database
                .get_rows_by_ids(&[1, 2, 3])
                .unwrap()
                .iter()
                .map(|row| row.group_id)
                .collect::<Vec<_>>(),
            vec![
                Some(target_group.id),
                Some(target_group.id),
                Some(target_group.id)
            ]
        );

        assert_eq!(
            database
                .revert_quick_group_changes(&applied.changes)
                .unwrap(),
            2
        );
        assert_eq!(
            database
                .get_rows_by_ids(&[1, 2, 3])
                .unwrap()
                .iter()
                .map(|row| row.group_id)
                .collect::<Vec<_>>(),
            vec![Some(target_group.id), Some(previous_group.id), None]
        );

        assert_eq!(
            database
                .reapply_quick_group_changes(&applied.changes)
                .unwrap(),
            2
        );
        assert_eq!(
            database
                .get_rows_by_ids(&[2, 3])
                .unwrap()
                .iter()
                .map(|row| row.group_id)
                .collect::<Vec<_>>(),
            vec![Some(target_group.id), Some(target_group.id)]
        );
    }

    #[test]
    fn quick_group_can_skip_every_already_grouped_match() {
        let mut database = database_with_rows(4);
        let previous_group = database.create_group("原分组").unwrap();
        let target_group = database.create_group("目标分组").unwrap();
        database
            .assign_rows_to_group(
                &crate::db::RowSelection::Explicit { row_ids: vec![1] },
                previous_group.id,
            )
            .unwrap();
        database
            .assign_rows_to_group(
                &crate::db::RowSelection::Explicit { row_ids: vec![2] },
                target_group.id,
            )
            .unwrap();
        database
            .connection
            .execute(
                "UPDATE rows SET positive_prompt = 'genshin' WHERE id IN (1, 2, 3)",
                [],
            )
            .unwrap();

        let condition = prompt_condition(&["genshin"]);
        let preview = database
            .preview_quick_group(&condition, target_group.id, true)
            .unwrap();
        assert_eq!(preview.scanned_rows, 4);
        assert_eq!(preview.matched_rows, 3);
        assert_eq!(preview.rows_needing_changes, 1);
        assert_eq!(preview.already_in_group_rows, 0);
        assert_eq!(preview.skipped_grouped_rows, 2);
        assert!(preview.only_ungrouped);
        assert_eq!(preview.sample_row_ids, vec![3]);

        let applied = database
            .apply_quick_group(&condition, target_group.id, true)
            .unwrap();
        assert_eq!(applied.matched_rows, 3);
        assert_eq!(applied.changed_rows, 1);
        assert_eq!(applied.skipped_grouped_rows, 2);
        assert!(applied.only_ungrouped);
        assert_eq!(
            applied.changes,
            vec![QuickGroupChange {
                row_id: 3,
                previous_group_id: None,
                target_group_id: target_group.id,
            }]
        );
        assert_eq!(
            database
                .get_rows_by_ids(&[1, 2, 3])
                .unwrap()
                .iter()
                .map(|row| row.group_id)
                .collect::<Vec<_>>(),
            vec![
                Some(previous_group.id),
                Some(target_group.id),
                Some(target_group.id),
            ]
        );

        assert_eq!(
            database
                .revert_quick_group_changes(&applied.changes)
                .unwrap(),
            1
        );
        assert_eq!(database.get_rows_by_ids(&[3]).unwrap()[0].group_id, None);
    }

    #[test]
    fn quick_group_rejects_unknown_target_without_changes() {
        let mut database = database_with_rows(1);
        database
            .connection
            .execute("UPDATE rows SET positive_prompt = 'genshin, hutao'", [])
            .unwrap();

        let result = database.apply_quick_group(&prompt_condition(&["genshin"]), 999, false);

        assert!(matches!(
            result,
            Err(QuickEditError::Database(DatabaseError::GroupNotFound(999)))
        ));
        assert_eq!(database.get_rows_by_ids(&[1]).unwrap()[0].group_id, None);
    }

    #[test]
    fn quick_artist_prefix_covers_all_prompt_fields_and_supports_undo_redo() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[
                NewRow {
                    source_ordinal: 2,
                    identity: "all-fields".into(),
                    positive_prompt: Some("best quality, parsley_f".into()),
                    character_prompt: Some("(parsley_f:1.2), 1girl".into()),
                    negative_prompt: Some("0.7::parsley_f::, lowres".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 3,
                    identity: "negative-only".into(),
                    positive_prompt: Some("artist:existing".into()),
                    negative_prompt: Some("{parsley_f}".into()),
                    artists: Some("artist:existing".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 4,
                    identity: "already-or-similar".into(),
                    positive_prompt: Some("artist:parsley_f, parsley_fx".into()),
                    artists: Some("artist:parsley_f".into()),
                    ..NewRow::default()
                },
            ],
        );

        let preview = database
            .preview_quick_artist_prefix("artist:parsley_f")
            .unwrap();
        assert_eq!(preview.scanned_rows, 3);
        assert_eq!(preview.matched_rows, 2);
        assert_eq!(preview.rows_needing_changes, 2);
        assert_eq!(preview.prompt_fields_needing_changes, 4);
        assert_eq!(preview.sample_row_ids, vec![1, 2]);
        assert_eq!(preview.artist_name, "parsley_f");

        let applied = database.apply_quick_artist_prefix("parsley_f").unwrap();
        assert_eq!(applied.changed_rows, 2);
        assert_eq!(applied.prompt_fields_changed, 4);

        let first: (String, String, String, String) = database
            .connection
            .query_row(
                "SELECT positive_prompt, character_prompt, negative_prompt, artists
                 FROM rows WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            first,
            (
                "best quality, artist:parsley_f".into(),
                "(artist:parsley_f:1.2), 1girl".into(),
                "0.7::artist:parsley_f::, lowres".into(),
                "artist:parsley_f\n(artist:parsley_f:1.2)".into(),
            )
        );

        let second: (String, String) = database
            .connection
            .query_row(
                "SELECT negative_prompt, artists FROM rows WHERE id = 2",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            second,
            ("{artist:parsley_f}".into(), "artist:existing".into())
        );

        assert_eq!(
            database
                .revert_quick_artist_prefix_changes(&applied.changes)
                .unwrap(),
            2
        );
        let reverted: (String, String, String, Option<String>) = database
            .connection
            .query_row(
                "SELECT positive_prompt, character_prompt, negative_prompt, artists
                 FROM rows WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            reverted,
            (
                "best quality, parsley_f".into(),
                "(parsley_f:1.2), 1girl".into(),
                "0.7::parsley_f::, lowres".into(),
                None,
            )
        );

        assert_eq!(
            database
                .reapply_quick_artist_prefix_changes(&applied.changes)
                .unwrap(),
            2
        );
        let redone: String = database
            .connection
            .query_row("SELECT positive_prompt FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(redone, "best quality, artist:parsley_f");
    }

    #[test]
    fn quick_artist_prefix_handles_numerical_weight_closer_after_comma() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[NewRow {
                source_ordinal: 1,
                identity: "cross-comma-weight".into(),
                positive_prompt: Some("1::artist:huangdanlan, rourow::,".into()),
                ..NewRow::default()
            }],
        );

        let preview = database.preview_quick_artist_prefix("rourow").unwrap();
        assert_eq!(preview.scanned_rows, 1);
        assert_eq!(preview.matched_rows, 1);
        assert_eq!(preview.prompt_fields_needing_changes, 1);

        let applied = database.apply_quick_artist_prefix("rourow").unwrap();
        assert_eq!(applied.changed_rows, 1);
        assert_eq!(applied.prompt_fields_changed, 1);

        let row: (String, String) = database
            .connection
            .query_row(
                "SELECT positive_prompt, artists FROM rows WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "1::artist:huangdanlan, artist:rourow::,".into(),
                "1::artist:huangdanlan\nartist:rourow::".into(),
            )
        );
    }

    #[test]
    fn quick_artist_prefix_rejects_multiple_names() {
        let database = database_with_rows(1);
        assert!(matches!(
            database.preview_quick_artist_prefix("alice, bob"),
            Err(QuickEditError::InvalidArtistName)
        ));
    }

    #[test]
    fn normalization_keeps_spaces_and_underscores_distinct() {
        assert_eq!(normalize_prompt_token("Hu Tao"), "hu tao");
        assert_eq!(normalize_prompt_token("hu_tao"), "hu_tao");
        assert_eq!(normalize_prompt_token("1.2::{{HUTAO:1.1}}::"), "hutao");
        assert_eq!(normalize_prompt_token("girl"), "girl");
        assert_eq!(normalize_prompt_token("1girl"), "girl");
        assert_eq!(normalize_prompt_token("(1 GIRL:1.2)"), "girl");
    }
}
