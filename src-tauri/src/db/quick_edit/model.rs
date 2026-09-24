use crate::db::DatabaseError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
    #[serde(default)]
    pub previous_artist_llm: Option<String>,
    #[serde(default)]
    pub new_artist_llm: Option<String>,
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
