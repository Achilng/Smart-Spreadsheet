use std::collections::{HashMap, HashSet};
use std::path::Path;

use regex::{Regex, RegexBuilder};
use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::prompt_edit::{combined_artists, prefix_artist_tag_in_prompt};
use super::{Database, DatabaseError};

const SAMPLE_LIMIT: usize = 12;
const CANDIDATE_TABLE: &str = "temp.automation_rule_candidates";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleMatchMode {
    All,
    Any,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleConditionSet {
    pub mode: RuleMatchMode,
    #[serde(default)]
    pub negate: bool,
    pub groups: Vec<RuleConditionGroup>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleConditionGroup {
    pub mode: RuleMatchMode,
    pub conditions: Vec<RuleCondition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptScope {
    Positive,
    Character,
    Negative,
    PositiveAndCharacter,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptOperator {
    ContainsAll,
    ContainsAny,
    ContainsNone,
    TextContains,
    TextEquals,
    Regex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TagOperator {
    HasAll,
    HasAny,
    HasNone,
    IsEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupOperator {
    Is,
    IsNot,
    IsEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtistOperator {
    ContainsAny,
    ContainsNone,
    IsSingle,
    IsMultiple,
    IsEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteOperator {
    Contains,
    IsEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileTextField {
    FileName,
    OriginalPath,
    ImportSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextOperator {
    Contains,
    Equals,
    Regex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleSourceType {
    Folder,
    Archive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NumericOperator {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
    Between,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumericComparison {
    pub operator: NumericOperator,
    pub value: f64,
    pub second_value: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VibeOperator {
    HasAny,
    HasNone,
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageDimensionField {
    Width,
    Height,
    AspectRatio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageOrientation {
    Landscape,
    Portrait,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GenerationTextField {
    Model,
    Sampler,
    NoiseSchedule,
    Seed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GenerationNumberField {
    Steps,
    Scale,
    CfgRescale,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RuleCondition {
    Prompt {
        scope: PromptScope,
        operator: PromptOperator,
        value: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    Tag {
        operator: TagOperator,
        #[serde(default)]
        tags: Vec<String>,
    },
    Group {
        operator: GroupOperator,
        group_id: Option<i64>,
    },
    Artist {
        operator: ArtistOperator,
        #[serde(default)]
        artists: Vec<String>,
    },
    Note {
        operator: NoteOperator,
        #[serde(default)]
        value: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    FileText {
        field: FileTextField,
        operator: TextOperator,
        value: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    FileSize {
        comparison: NumericComparison,
    },
    SourceType {
        source_type: RuleSourceType,
        #[serde(default)]
        negate: bool,
    },
    Vibe {
        operator: VibeOperator,
        comparison: Option<NumericComparison>,
    },
    Metadata {
        parsed: bool,
    },
    ImageDimension {
        field: ImageDimensionField,
        comparison: NumericComparison,
    },
    Orientation {
        orientation: ImageOrientation,
        #[serde(default)]
        negate: bool,
    },
    GenerationText {
        field: GenerationTextField,
        operator: TextOperator,
        value: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    GenerationNumber {
        field: GenerationNumberField,
        comparison: NumericComparison,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptActionField {
    Positive,
    Character,
    Negative,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RuleAction {
    AddTags {
        tags: Vec<String>,
    },
    RemoveTags {
        tags: Vec<String>,
    },
    SetGroup {
        group_id: i64,
        #[serde(default)]
        only_if_ungrouped: bool,
    },
    ClearGroup,
    AppendPrompt {
        field: PromptActionField,
        value: String,
    },
    DeletePromptTags {
        field: PromptActionField,
        value: String,
    },
    ReplacePrompt {
        field: PromptActionField,
        find: String,
        replace: String,
        #[serde(default = "default_true")]
        case_sensitive: bool,
    },
    PrefixArtist {
        artists: Vec<String>,
    },
    SetNote {
        value: String,
    },
    AppendNote {
        value: String,
        #[serde(default = "default_note_separator")]
        separator: String,
    },
    ClearNote,
    StopProcessing,
}

fn default_true() -> bool {
    true
}

fn default_note_separator() -> String {
    "\n".to_owned()
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRuleDraft {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub run_on_import: bool,
    #[serde(default)]
    pub run_on_update: bool,
    pub conditions: RuleConditionSet,
    pub actions: Vec<RuleAction>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRule {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub position: u32,
    pub run_on_import: bool,
    pub run_on_update: bool,
    pub conditions: RuleConditionSet,
    pub actions: Vec<RuleAction>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleExecutionTrigger {
    Import,
    Update,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleExecutionReport {
    pub rule_id: i64,
    pub rule_name: String,
    pub scanned_rows: u64,
    pub matched_rows: u64,
    pub changed_rows: u64,
    pub actions_changed: u64,
    pub stopped_rows: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleExecutionSummary {
    pub trigger: RuleExecutionTrigger,
    pub input_rows: u64,
    pub changed_rows: u64,
    pub reports: Vec<RuleExecutionReport>,
    pub engine_error: Option<String>,
}

impl RuleExecutionSummary {
    pub fn failed(trigger: RuleExecutionTrigger, input_rows: usize, error: impl ToString) -> Self {
        Self {
            trigger,
            input_rows: u64::try_from(input_rows).unwrap_or(u64::MAX),
            changed_rows: 0,
            reports: Vec::new(),
            engine_error: Some(error.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePreview {
    pub scanned_rows: u64,
    pub matched_rows: u64,
    pub rows_needing_changes: u64,
    pub stopped_rows: u64,
    pub sample_row_ids: Vec<i64>,
}

#[derive(Debug, Error)]
pub enum AutomationRuleError {
    #[error("数据库操作失败: {0}")]
    Database(#[from] DatabaseError),
    #[error("规则数据读写失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("规则名称不能为空")]
    EmptyName,
    #[error("规则至少需要一个条件组")]
    EmptyConditionSet,
    #[error("第 {0} 个条件组没有条件")]
    EmptyConditionGroup(usize),
    #[error("规则至少需要一个执行任务")]
    EmptyActions,
    #[error("规则字段不能为空: {0}")]
    EmptyValue(&'static str),
    #[error("数值区间条件缺少结束值")]
    MissingRangeEnd,
    #[error("无效的正则表达式: {0}")]
    InvalidRegex(String),
    #[error("不存在的规则 ID: {0}")]
    RuleNotFound(i64),
    #[error("规则顺序必须包含当前全部规则且不能重复")]
    InvalidOrder,
    #[error("不存在的目标分组 ID: {0}")]
    MissingTargetGroup(i64),
    #[error("规则配置无效: {0}")]
    InvalidDefinition(String),
}

impl From<rusqlite::Error> for AutomationRuleError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(DatabaseError::Sqlite(value))
    }
}

#[derive(Debug, Clone)]
struct RuleRow {
    id: i64,
    positive_prompt: Option<String>,
    character_prompt: Option<String>,
    negative_prompt: Option<String>,
    artists: Option<String>,
    note: Option<String>,
    group_id: Option<i64>,
    tags: HashSet<String>,
    image_path: Option<String>,
    source_size: Option<i64>,
    metadata_failed: bool,
    vibe_count: u32,
    image_width: Option<u32>,
    image_height: Option<u32>,
    generation_model: Option<String>,
    generation_sampler: Option<String>,
    generation_steps: Option<u32>,
    generation_seed: Option<String>,
    generation_scale: Option<f64>,
    generation_cfg_rescale: Option<f64>,
    generation_noise_schedule: Option<String>,
    source_type: String,
    source_path: String,
}

struct PreparedCondition<'a> {
    condition: &'a RuleCondition,
    tokens: Vec<String>,
    regex: Option<Regex>,
}

struct PreparedConditionSet<'a> {
    source: &'a RuleConditionSet,
    groups: Vec<Vec<PreparedCondition<'a>>>,
}

struct RuleApplyOutcome {
    report: RuleExecutionReport,
    changed_row_ids: HashSet<i64>,
    stopped_row_ids: HashSet<i64>,
}

impl Database {
    pub fn list_automation_rules(&self) -> Result<Vec<AutomationRule>, AutomationRuleError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, description, enabled, position, run_on_import, run_on_update,
                    conditions_json, actions_json, created_at, updated_at
             FROM automation_rules ORDER BY position, id",
        )?;
        let stored = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, bool>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        stored
            .into_iter()
            .map(|row| automation_rule_from_stored(row).map_err(AutomationRuleError::from))
            .collect()
    }

    pub fn create_automation_rule(
        &mut self,
        draft: &AutomationRuleDraft,
    ) -> Result<AutomationRule, AutomationRuleError> {
        validate_draft(draft)?;
        validate_group_targets(&self.connection, &draft.actions)?;
        let conditions = serde_json::to_string(&draft.conditions)?;
        let actions = serde_json::to_string(&draft.actions)?;
        let position: u32 = self.connection.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM automation_rules",
            [],
            |row| row.get(0),
        )?;
        self.connection.execute(
            "INSERT INTO automation_rules
                (name, description, enabled, position, run_on_import, run_on_update,
                 conditions_json, actions_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                draft.name.trim(),
                draft.description.trim(),
                draft.enabled,
                position,
                draft.run_on_import,
                draft.run_on_update,
                conditions,
                actions,
            ],
        )?;
        let id = self.connection.last_insert_rowid();
        self.automation_rule(id)
    }

    pub fn update_automation_rule(
        &mut self,
        id: i64,
        draft: &AutomationRuleDraft,
    ) -> Result<AutomationRule, AutomationRuleError> {
        validate_draft(draft)?;
        validate_group_targets(&self.connection, &draft.actions)?;
        let conditions = serde_json::to_string(&draft.conditions)?;
        let actions = serde_json::to_string(&draft.actions)?;
        let changed = self.connection.execute(
            "UPDATE automation_rules SET
                name = ?2, description = ?3, enabled = ?4,
                run_on_import = ?5, run_on_update = ?6,
                conditions_json = ?7, actions_json = ?8,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
            params![
                id,
                draft.name.trim(),
                draft.description.trim(),
                draft.enabled,
                draft.run_on_import,
                draft.run_on_update,
                conditions,
                actions,
            ],
        )?;
        if changed == 0 {
            return Err(AutomationRuleError::RuleNotFound(id));
        }
        self.automation_rule(id)
    }

    pub fn set_automation_rule_enabled(
        &mut self,
        id: i64,
        enabled: bool,
    ) -> Result<(), AutomationRuleError> {
        if self.connection.execute(
            "UPDATE automation_rules SET enabled = ?2,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
            params![id, enabled],
        )? == 0
        {
            return Err(AutomationRuleError::RuleNotFound(id));
        }
        Ok(())
    }

    pub fn delete_automation_rule(&mut self, id: i64) -> Result<bool, AutomationRuleError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let deleted = transaction.execute("DELETE FROM automation_rules WHERE id = ?1", [id])?;
        if deleted > 0 {
            normalize_rule_positions(&transaction)?;
        }
        transaction.commit()?;
        Ok(deleted > 0)
    }

    pub fn reorder_automation_rules(&mut self, ids: &[i64]) -> Result<(), AutomationRuleError> {
        let existing = self
            .list_automation_rules()?
            .into_iter()
            .map(|rule| rule.id)
            .collect::<HashSet<_>>();
        let requested = ids.iter().copied().collect::<HashSet<_>>();
        if ids.len() != existing.len() || requested.len() != ids.len() || requested != existing {
            return Err(AutomationRuleError::InvalidOrder);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE automation_rules SET position = position + 1000000",
            [],
        )?;
        for (position, id) in ids.iter().enumerate() {
            transaction.execute(
                "UPDATE automation_rules SET position = ?2,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
                params![
                    id,
                    i64::try_from(position).map_err(|_| DatabaseError::CountOverflow)?
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn preview_automation_rule(&self, id: i64) -> Result<RulePreview, AutomationRuleError> {
        let rule = self.automation_rule(id)?;
        validate_draft(&draft_from_rule(&rule))?;
        validate_group_targets(&self.connection, &rule.actions)?;
        let prepared = PreparedConditionSet::new(&rule.conditions)?;
        let rows = load_rule_rows(&self.connection, None)?;
        let mut matched = Vec::new();
        let mut needing_changes = 0_u64;
        let mut stopped = 0_u64;
        for row in &rows {
            if !prepared.matches(row) {
                continue;
            }
            matched.push(row.id);
            let (_, changed_actions, should_stop) = simulate_actions(row, &rule.actions)?;
            if changed_actions > 0 {
                needing_changes += 1;
            }
            if should_stop {
                stopped += 1;
            }
        }
        Ok(RulePreview {
            scanned_rows: u64::try_from(rows.len()).map_err(|_| DatabaseError::CountOverflow)?,
            matched_rows: u64::try_from(matched.len()).map_err(|_| DatabaseError::CountOverflow)?,
            rows_needing_changes: needing_changes,
            stopped_rows: stopped,
            sample_row_ids: matched.into_iter().take(SAMPLE_LIMIT).collect(),
        })
    }

    pub fn run_automation_rule_on_library(
        &mut self,
        id: i64,
    ) -> Result<RuleExecutionSummary, AutomationRuleError> {
        let rule = self.automation_rule(id)?;
        let row_ids = self.all_row_ids()?;
        let outcome = self.apply_rule(&rule, &row_ids)?;
        let changed_rows = u64::try_from(outcome.changed_row_ids.len())
            .map_err(|_| DatabaseError::CountOverflow)?;
        if changed_rows > 0 {
            self.bump_data_version();
        }
        Ok(RuleExecutionSummary {
            trigger: RuleExecutionTrigger::Manual,
            input_rows: u64::try_from(row_ids.len()).map_err(|_| DatabaseError::CountOverflow)?,
            changed_rows,
            reports: vec![outcome.report],
            engine_error: None,
        })
    }

    pub fn execute_automation_rules(
        &mut self,
        trigger: RuleExecutionTrigger,
        row_ids: &[i64],
    ) -> Result<RuleExecutionSummary, AutomationRuleError> {
        let rules = self
            .list_automation_rules()?
            .into_iter()
            .filter(|rule| {
                rule.enabled
                    && match trigger {
                        RuleExecutionTrigger::Import => rule.run_on_import,
                        RuleExecutionTrigger::Update => rule.run_on_update,
                        RuleExecutionTrigger::Manual => true,
                    }
            })
            .collect::<Vec<_>>();
        let mut active = row_ids.iter().copied().collect::<HashSet<_>>();
        let mut changed = HashSet::new();
        let mut reports = Vec::new();
        for rule in rules {
            if active.is_empty() {
                break;
            }
            let candidates = active.iter().copied().collect::<Vec<_>>();
            match self.apply_rule(&rule, &candidates) {
                Ok(outcome) => {
                    changed.extend(outcome.changed_row_ids);
                    for row_id in &outcome.stopped_row_ids {
                        active.remove(row_id);
                    }
                    reports.push(outcome.report);
                }
                Err(error) => reports.push(RuleExecutionReport {
                    rule_id: rule.id,
                    rule_name: rule.name,
                    scanned_rows: u64::try_from(candidates.len()).unwrap_or(u64::MAX),
                    matched_rows: 0,
                    changed_rows: 0,
                    actions_changed: 0,
                    stopped_rows: 0,
                    error: Some(error.to_string()),
                }),
            }
        }
        if !changed.is_empty() {
            self.bump_data_version();
        }
        Ok(RuleExecutionSummary {
            trigger,
            input_rows: u64::try_from(row_ids.len()).map_err(|_| DatabaseError::CountOverflow)?,
            changed_rows: u64::try_from(changed.len()).map_err(|_| DatabaseError::CountOverflow)?,
            reports,
            engine_error: None,
        })
    }

    fn automation_rule(&self, id: i64) -> Result<AutomationRule, AutomationRuleError> {
        self.list_automation_rules()?
            .into_iter()
            .find(|rule| rule.id == id)
            .ok_or(AutomationRuleError::RuleNotFound(id))
    }

    fn all_row_ids(&self) -> Result<Vec<i64>, AutomationRuleError> {
        let mut statement = self.connection.prepare("SELECT id FROM rows ORDER BY id")?;
        Ok(statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    fn apply_rule(
        &mut self,
        rule: &AutomationRule,
        row_ids: &[i64],
    ) -> Result<RuleApplyOutcome, AutomationRuleError> {
        validate_draft(&draft_from_rule(rule))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_group_targets(&transaction, &rule.actions)?;
        ensure_action_tags(&transaction, &rule.actions)?;
        let prepared = PreparedConditionSet::new(&rule.conditions)?;
        let rows = load_rule_rows(&transaction, Some(row_ids))?;
        let scanned_rows = u64::try_from(rows.len()).map_err(|_| DatabaseError::CountOverflow)?;
        let mut matched_rows = 0_u64;
        let mut actions_changed = 0_u64;
        let mut changed_row_ids = HashSet::new();
        let mut stopped_row_ids = HashSet::new();
        for row in rows {
            if !prepared.matches(&row) {
                continue;
            }
            matched_rows += 1;
            let (updated, changed_actions, should_stop) = simulate_actions(&row, &rule.actions)?;
            if changed_actions > 0 {
                persist_rule_row(&transaction, &row, &updated)?;
                changed_row_ids.insert(row.id);
                actions_changed += changed_actions;
            }
            if should_stop {
                stopped_row_ids.insert(row.id);
            }
        }
        transaction.commit()?;
        Ok(RuleApplyOutcome {
            report: RuleExecutionReport {
                rule_id: rule.id,
                rule_name: rule.name.clone(),
                scanned_rows,
                matched_rows,
                changed_rows: u64::try_from(changed_row_ids.len())
                    .map_err(|_| DatabaseError::CountOverflow)?,
                actions_changed,
                stopped_rows: u64::try_from(stopped_row_ids.len())
                    .map_err(|_| DatabaseError::CountOverflow)?,
                error: None,
            },
            changed_row_ids,
            stopped_row_ids,
        })
    }
}

impl<'a> PreparedConditionSet<'a> {
    fn new(source: &'a RuleConditionSet) -> Result<Self, AutomationRuleError> {
        let groups = source
            .groups
            .iter()
            .map(|group| {
                group
                    .conditions
                    .iter()
                    .map(PreparedCondition::new)
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { source, groups })
    }

    fn matches(&self, row: &RuleRow) -> bool {
        let group_results =
            self.source
                .groups
                .iter()
                .zip(&self.groups)
                .map(|(group, conditions)| match group.mode {
                    RuleMatchMode::All => conditions.iter().all(|condition| condition.matches(row)),
                    RuleMatchMode::Any => conditions.iter().any(|condition| condition.matches(row)),
                });
        let matched = match self.source.mode {
            RuleMatchMode::All => group_results.clone().all(|value| value),
            RuleMatchMode::Any => group_results.into_iter().any(|value| value),
        };
        matched != self.source.negate
    }
}

impl<'a> PreparedCondition<'a> {
    fn new(condition: &'a RuleCondition) -> Result<Self, AutomationRuleError> {
        let (tokens, regex) = match condition {
            RuleCondition::Prompt {
                operator,
                value,
                case_sensitive,
                ..
            } => match operator {
                PromptOperator::ContainsAll
                | PromptOperator::ContainsAny
                | PromptOperator::ContainsNone => (parse_prompt_tokens(value), None),
                PromptOperator::Regex => (Vec::new(), Some(build_regex(value, *case_sensitive)?)),
                _ => (Vec::new(), None),
            },
            RuleCondition::FileText {
                operator: TextOperator::Regex,
                value,
                case_sensitive,
                ..
            }
            | RuleCondition::GenerationText {
                operator: TextOperator::Regex,
                value,
                case_sensitive,
                ..
            } => (Vec::new(), Some(build_regex(value, *case_sensitive)?)),
            _ => (Vec::new(), None),
        };
        Ok(Self {
            condition,
            tokens,
            regex,
        })
    }

    fn matches(&self, row: &RuleRow) -> bool {
        match self.condition {
            RuleCondition::Prompt {
                scope,
                operator,
                value,
                case_sensitive,
            } => {
                let text = prompt_scope_text(row, *scope);
                match operator {
                    PromptOperator::ContainsAll => {
                        let available = prompt_token_set(&text);
                        self.tokens.iter().all(|token| available.contains(token))
                    }
                    PromptOperator::ContainsAny => {
                        let available = prompt_token_set(&text);
                        self.tokens.iter().any(|token| available.contains(token))
                    }
                    PromptOperator::ContainsNone => {
                        let available = prompt_token_set(&text);
                        self.tokens.iter().all(|token| !available.contains(token))
                    }
                    PromptOperator::TextContains => {
                        text_compare(&text, value, TextOperator::Contains, *case_sensitive, None)
                    }
                    PromptOperator::TextEquals => {
                        text_compare(&text, value, TextOperator::Equals, *case_sensitive, None)
                    }
                    PromptOperator::Regex => self
                        .regex
                        .as_ref()
                        .is_some_and(|regex| regex.is_match(&text)),
                }
            }
            RuleCondition::Tag { operator, tags } => {
                let expected = normalized_strings(tags);
                match operator {
                    TagOperator::HasAll => expected.iter().all(|tag| row.tags.contains(tag)),
                    TagOperator::HasAny => expected.iter().any(|tag| row.tags.contains(tag)),
                    TagOperator::HasNone => expected.iter().all(|tag| !row.tags.contains(tag)),
                    TagOperator::IsEmpty => row.tags.is_empty(),
                }
            }
            RuleCondition::Group { operator, group_id } => match operator {
                GroupOperator::Is => row.group_id == *group_id,
                GroupOperator::IsNot => row.group_id != *group_id,
                GroupOperator::IsEmpty => row.group_id.is_none(),
            },
            RuleCondition::Artist { operator, artists } => {
                let available = artist_set(row.artists.as_deref());
                let expected = normalized_strings(artists);
                match operator {
                    ArtistOperator::ContainsAny => expected
                        .iter()
                        .map(|artist| normalize_artist(artist))
                        .any(|artist| available.contains(&artist)),
                    ArtistOperator::ContainsNone => expected
                        .iter()
                        .map(|artist| normalize_artist(artist))
                        .all(|artist| !available.contains(&artist)),
                    ArtistOperator::IsSingle => available.len() == 1,
                    ArtistOperator::IsMultiple => available.len() > 1,
                    ArtistOperator::IsEmpty => available.is_empty(),
                }
            }
            RuleCondition::Note {
                operator,
                value,
                case_sensitive,
            } => match operator {
                NoteOperator::Contains => text_compare(
                    row.note.as_deref().unwrap_or(""),
                    value,
                    TextOperator::Contains,
                    *case_sensitive,
                    None,
                ),
                NoteOperator::IsEmpty => row
                    .note
                    .as_deref()
                    .is_none_or(|note| note.trim().is_empty()),
            },
            RuleCondition::FileText {
                field,
                operator,
                value,
                case_sensitive,
            } => {
                let text = match field {
                    FileTextField::FileName => row
                        .image_path
                        .as_deref()
                        .and_then(|path| Path::new(path).file_name())
                        .and_then(|name| name.to_str())
                        .unwrap_or(""),
                    FileTextField::OriginalPath => row.image_path.as_deref().unwrap_or(""),
                    FileTextField::ImportSource => &row.source_path,
                };
                text_compare(text, value, *operator, *case_sensitive, self.regex.as_ref())
            }
            RuleCondition::FileSize { comparison } => row
                .source_size
                .is_some_and(|size| number_matches(size as f64, comparison)),
            RuleCondition::SourceType {
                source_type,
                negate,
            } => {
                let value = match source_type {
                    RuleSourceType::Folder => "folder",
                    RuleSourceType::Archive => "archive",
                };
                (row.source_type == value) != *negate
            }
            RuleCondition::Vibe {
                operator,
                comparison,
            } => match operator {
                VibeOperator::HasAny => row.vibe_count > 0,
                VibeOperator::HasNone => row.vibe_count == 0,
                VibeOperator::Count => comparison.as_ref().is_some_and(|comparison| {
                    number_matches(f64::from(row.vibe_count), comparison)
                }),
            },
            RuleCondition::Metadata { parsed } => row.metadata_failed != *parsed,
            RuleCondition::ImageDimension { field, comparison } => {
                let value = match field {
                    ImageDimensionField::Width => row.image_width.map(f64::from),
                    ImageDimensionField::Height => row.image_height.map(f64::from),
                    ImageDimensionField::AspectRatio => row
                        .image_width
                        .zip(row.image_height)
                        .map(|(width, height)| f64::from(width) / f64::from(height)),
                };
                value.is_some_and(|value| number_matches(value, comparison))
            }
            RuleCondition::Orientation {
                orientation,
                negate,
            } => row
                .image_width
                .zip(row.image_height)
                .is_some_and(|(width, height)| {
                    let matches = match orientation {
                        ImageOrientation::Landscape => width > height,
                        ImageOrientation::Portrait => height > width,
                        ImageOrientation::Square => width == height,
                    };
                    matches != *negate
                }),
            RuleCondition::GenerationText {
                field,
                operator,
                value,
                case_sensitive,
            } => {
                let text = match field {
                    GenerationTextField::Model => row.generation_model.as_deref(),
                    GenerationTextField::Sampler => row.generation_sampler.as_deref(),
                    GenerationTextField::NoiseSchedule => row.generation_noise_schedule.as_deref(),
                    GenerationTextField::Seed => row.generation_seed.as_deref(),
                };
                text.is_some_and(|text| {
                    text_compare(text, value, *operator, *case_sensitive, self.regex.as_ref())
                })
            }
            RuleCondition::GenerationNumber { field, comparison } => {
                let value = match field {
                    GenerationNumberField::Steps => row.generation_steps.map(f64::from),
                    GenerationNumberField::Scale => row.generation_scale,
                    GenerationNumberField::CfgRescale => row.generation_cfg_rescale,
                };
                value.is_some_and(|value| number_matches(value, comparison))
            }
        }
    }
}

fn automation_rule_from_stored(
    stored: (
        i64,
        String,
        String,
        bool,
        u32,
        bool,
        bool,
        String,
        String,
        String,
        String,
    ),
) -> Result<AutomationRule, serde_json::Error> {
    let (
        id,
        name,
        description,
        enabled,
        position,
        run_on_import,
        run_on_update,
        conditions,
        actions,
        created_at,
        updated_at,
    ) = stored;
    Ok(AutomationRule {
        id,
        name,
        description,
        enabled,
        position,
        run_on_import,
        run_on_update,
        conditions: serde_json::from_str(&conditions)?,
        actions: serde_json::from_str(&actions)?,
        created_at,
        updated_at,
    })
}

fn draft_from_rule(rule: &AutomationRule) -> AutomationRuleDraft {
    AutomationRuleDraft {
        name: rule.name.clone(),
        description: rule.description.clone(),
        enabled: rule.enabled,
        run_on_import: rule.run_on_import,
        run_on_update: rule.run_on_update,
        conditions: rule.conditions.clone(),
        actions: rule.actions.clone(),
    }
}

fn validate_draft(draft: &AutomationRuleDraft) -> Result<(), AutomationRuleError> {
    if draft.name.trim().is_empty() {
        return Err(AutomationRuleError::EmptyName);
    }
    if draft.conditions.groups.is_empty() {
        return Err(AutomationRuleError::EmptyConditionSet);
    }
    for (index, group) in draft.conditions.groups.iter().enumerate() {
        if group.conditions.is_empty() {
            return Err(AutomationRuleError::EmptyConditionGroup(index + 1));
        }
        for condition in &group.conditions {
            validate_condition(condition)?;
        }
    }
    if draft.actions.is_empty() {
        return Err(AutomationRuleError::EmptyActions);
    }
    for action in &draft.actions {
        validate_action(action)?;
    }
    PreparedConditionSet::new(&draft.conditions)?;
    Ok(())
}

fn validate_condition(condition: &RuleCondition) -> Result<(), AutomationRuleError> {
    match condition {
        RuleCondition::Prompt {
            operator, value, ..
        } => {
            if value.trim().is_empty() {
                return Err(AutomationRuleError::EmptyValue("提示词条件"));
            }
            if matches!(
                operator,
                PromptOperator::ContainsAll
                    | PromptOperator::ContainsAny
                    | PromptOperator::ContainsNone
            ) && parse_prompt_tokens(value).is_empty()
            {
                return Err(AutomationRuleError::EmptyValue("提示词条件"));
            }
        }
        RuleCondition::Tag { operator, tags } => {
            if !matches!(operator, TagOperator::IsEmpty) && normalized_strings(tags).is_empty() {
                return Err(AutomationRuleError::EmptyValue("Tag 条件"));
            }
        }
        RuleCondition::Group { operator, group_id } => {
            if !matches!(operator, GroupOperator::IsEmpty) && group_id.is_none_or(|id| id <= 0) {
                return Err(AutomationRuleError::EmptyValue("分组条件"));
            }
        }
        RuleCondition::Artist { operator, artists } => {
            if matches!(
                operator,
                ArtistOperator::ContainsAny | ArtistOperator::ContainsNone
            ) && normalized_strings(artists).is_empty()
            {
                return Err(AutomationRuleError::EmptyValue("画师条件"));
            }
        }
        RuleCondition::Note {
            operator: NoteOperator::Contains,
            value,
            ..
        }
        | RuleCondition::FileText { value, .. }
        | RuleCondition::GenerationText { value, .. }
            if value.is_empty() =>
        {
            return Err(AutomationRuleError::EmptyValue("文本条件"));
        }
        RuleCondition::FileSize { comparison }
        | RuleCondition::ImageDimension { comparison, .. }
        | RuleCondition::GenerationNumber { comparison, .. } => {
            validate_comparison(comparison)?;
        }
        RuleCondition::Vibe {
            operator: VibeOperator::Count,
            comparison,
        } => validate_comparison(
            comparison
                .as_ref()
                .ok_or(AutomationRuleError::EmptyValue("VIBE 数量条件"))?,
        )?,
        _ => {}
    }
    Ok(())
}

fn validate_action(action: &RuleAction) -> Result<(), AutomationRuleError> {
    match action {
        RuleAction::AddTags { tags } | RuleAction::RemoveTags { tags }
            if normalized_strings(tags).is_empty() =>
        {
            Err(AutomationRuleError::EmptyValue("Tag 任务"))
        }
        RuleAction::SetGroup { group_id, .. } if *group_id <= 0 => {
            Err(AutomationRuleError::EmptyValue("目标分组"))
        }
        RuleAction::AppendPrompt { value, .. } | RuleAction::DeletePromptTags { value, .. }
            if value.trim().is_empty() =>
        {
            Err(AutomationRuleError::EmptyValue("提示词任务"))
        }
        RuleAction::ReplacePrompt { find, .. } if find.is_empty() => {
            Err(AutomationRuleError::EmptyValue("查找内容"))
        }
        RuleAction::PrefixArtist { artists } if normalized_strings(artists).is_empty() => {
            Err(AutomationRuleError::EmptyValue("画师前缀任务"))
        }
        RuleAction::AppendNote { value, .. } if value.is_empty() => {
            Err(AutomationRuleError::EmptyValue("备注任务"))
        }
        _ => Ok(()),
    }
}

fn validate_comparison(comparison: &NumericComparison) -> Result<(), AutomationRuleError> {
    if !comparison.value.is_finite()
        || comparison
            .second_value
            .is_some_and(|value| !value.is_finite())
    {
        return Err(AutomationRuleError::InvalidDefinition(
            "数值条件必须是有限数字".into(),
        ));
    }
    if comparison.operator == NumericOperator::Between && comparison.second_value.is_none() {
        return Err(AutomationRuleError::MissingRangeEnd);
    }
    Ok(())
}

fn validate_group_targets(
    connection: &Connection,
    actions: &[RuleAction],
) -> Result<(), AutomationRuleError> {
    for action in actions {
        let RuleAction::SetGroup { group_id, .. } = action else {
            continue;
        };
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM groups WHERE id = ?1)",
            [group_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(AutomationRuleError::MissingTargetGroup(*group_id));
        }
    }
    Ok(())
}

fn normalize_rule_positions(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
    let ids = {
        let mut statement =
            transaction.prepare("SELECT id FROM automation_rules ORDER BY position, id")?;
        statement
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    transaction.execute(
        "UPDATE automation_rules SET position = position + 1000000",
        [],
    )?;
    for (position, id) in ids.into_iter().enumerate() {
        transaction.execute(
            "UPDATE automation_rules SET position = ?2 WHERE id = ?1",
            params![id, i64::try_from(position).unwrap_or(i64::MAX)],
        )?;
    }
    Ok(())
}

fn ensure_action_tags(
    transaction: &Transaction<'_>,
    actions: &[RuleAction],
) -> Result<(), AutomationRuleError> {
    let tags = actions
        .iter()
        .filter_map(|action| match action {
            RuleAction::AddTags { tags } => Some(tags.as_slice()),
            _ => None,
        })
        .flat_map(normalized_strings)
        .collect::<HashSet<_>>();
    for tag in tags {
        transaction.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [&tag])?;
    }
    Ok(())
}

fn load_rule_rows(
    connection: &Connection,
    candidate_ids: Option<&[i64]>,
) -> Result<Vec<RuleRow>, AutomationRuleError> {
    connection.execute_batch(&format!(
        "DROP TABLE IF EXISTS {CANDIDATE_TABLE};
         CREATE TEMP TABLE {CANDIDATE_TABLE} (id INTEGER PRIMARY KEY) STRICT;"
    ))?;
    if let Some(ids) = candidate_ids {
        let mut insert = connection.prepare(&format!(
            "INSERT OR IGNORE INTO {CANDIDATE_TABLE}(id) VALUES (?1)"
        ))?;
        for id in ids {
            if *id > 0 {
                insert.execute([id])?;
            }
        }
    } else {
        connection.execute(
            &format!("INSERT INTO {CANDIDATE_TABLE}(id) SELECT id FROM rows"),
            [],
        )?;
    }

    let mut rows = {
        let mut statement = connection.prepare(&format!(
            "SELECT rows.id, rows.positive_prompt, rows.character_prompt, rows.negative_prompt,
                    rows.artists, rows.note, rows.group_id, rows.image_path, rows.source_size,
                    rows.metadata_failed, COALESCE(rows.vibe_reference_count, 0),
                    rows.image_width, rows.image_height, rows.generation_model,
                    rows.generation_sampler, rows.generation_steps, rows.generation_seed,
                    rows.generation_scale, rows.generation_cfg_rescale,
                    rows.generation_noise_schedule, import_batches.source_type,
                    import_batches.source_path
             FROM {CANDIDATE_TABLE} AS candidates
             JOIN rows ON rows.id = candidates.id
             JOIN import_batches ON import_batches.id = rows.batch_id
             ORDER BY rows.id"
        ))?;
        statement
            .query_map([], |row| {
                Ok(RuleRow {
                    id: row.get(0)?,
                    positive_prompt: row.get(1)?,
                    character_prompt: row.get(2)?,
                    negative_prompt: row.get(3)?,
                    artists: row.get(4)?,
                    note: row.get(5)?,
                    group_id: row.get(6)?,
                    tags: HashSet::new(),
                    image_path: row.get(7)?,
                    source_size: row.get(8)?,
                    metadata_failed: row.get(9)?,
                    vibe_count: row.get(10)?,
                    image_width: row.get(11)?,
                    image_height: row.get(12)?,
                    generation_model: row.get(13)?,
                    generation_sampler: row.get(14)?,
                    generation_steps: row.get(15)?,
                    generation_seed: row.get(16)?,
                    generation_scale: row.get(17)?,
                    generation_cfg_rescale: row.get(18)?,
                    generation_noise_schedule: row.get(19)?,
                    source_type: row.get(20)?,
                    source_path: row.get(21)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    let row_indices = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id, index))
        .collect::<HashMap<_, _>>();
    {
        let mut statement = connection.prepare(&format!(
            "SELECT row_tags.row_id, tags.name
             FROM {CANDIDATE_TABLE} AS candidates
             JOIN row_tags ON row_tags.row_id = candidates.id
             JOIN tags ON tags.id = row_tags.tag_id"
        ))?;
        let pairs = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (row_id, tag) in pairs {
            if let Some(index) = row_indices.get(&row_id) {
                rows[*index].tags.insert(tag);
            }
        }
    }
    connection.execute_batch(&format!("DROP TABLE {CANDIDATE_TABLE};"))?;
    Ok(rows)
}

fn simulate_actions(
    original: &RuleRow,
    actions: &[RuleAction],
) -> Result<(RuleRow, u64, bool), AutomationRuleError> {
    let mut row = original.clone();
    let mut changed_actions = 0_u64;
    let mut stop = false;
    for action in actions {
        let changed = match action {
            RuleAction::AddTags { tags } => normalized_strings(tags)
                .into_iter()
                .fold(false, |changed, tag| row.tags.insert(tag) || changed),
            RuleAction::RemoveTags { tags } => normalized_strings(tags)
                .into_iter()
                .fold(false, |changed, tag| row.tags.remove(&tag) || changed),
            RuleAction::SetGroup {
                group_id,
                only_if_ungrouped,
            } => {
                if *only_if_ungrouped && row.group_id.is_some() {
                    false
                } else if row.group_id != Some(*group_id) {
                    row.group_id = Some(*group_id);
                    true
                } else {
                    false
                }
            }
            RuleAction::ClearGroup => row.group_id.take().is_some(),
            RuleAction::AppendPrompt { field, value } => {
                let prompt = prompt_mut(&mut row, *field);
                let updated = append_prompt(prompt.as_deref(), value);
                if *prompt != updated {
                    *prompt = updated;
                    refresh_artists(&mut row);
                    true
                } else {
                    false
                }
            }
            RuleAction::DeletePromptTags { field, value } => {
                let prompt = prompt_mut(&mut row, *field);
                let updated = delete_prompt_tags(prompt.as_deref(), value);
                if *prompt != updated {
                    *prompt = updated;
                    refresh_artists(&mut row);
                    true
                } else {
                    false
                }
            }
            RuleAction::ReplacePrompt {
                field,
                find,
                replace,
                case_sensitive,
            } => {
                let prompt = prompt_mut(&mut row, *field);
                let updated = prompt
                    .as_deref()
                    .map(|value| replace_text(value, find, replace, *case_sensitive));
                if *prompt != updated {
                    *prompt = updated.and_then(nonempty);
                    refresh_artists(&mut row);
                    true
                } else {
                    false
                }
            }
            RuleAction::PrefixArtist { artists } => {
                let mut changed = false;
                for artist in normalized_strings(artists) {
                    for field in [
                        PromptActionField::Positive,
                        PromptActionField::Character,
                        PromptActionField::Negative,
                    ] {
                        let prompt = prompt_mut(&mut row, field);
                        if let Some(updated) = prompt
                            .as_deref()
                            .and_then(|prompt| prefix_artist_tag_in_prompt(prompt, &artist))
                        {
                            *prompt = Some(updated);
                            changed = true;
                        }
                    }
                }
                if changed {
                    refresh_artists(&mut row);
                }
                changed
            }
            RuleAction::SetNote { value } => {
                let updated = nonempty(value.clone());
                if row.note != updated {
                    row.note = updated;
                    true
                } else {
                    false
                }
            }
            RuleAction::AppendNote { value, separator } => {
                let updated = match row.note.as_deref().filter(|note| !note.is_empty()) {
                    Some(note) => format!("{note}{separator}{value}"),
                    None => value.clone(),
                };
                if row.note.as_deref() != Some(updated.as_str()) {
                    row.note = nonempty(updated);
                    true
                } else {
                    false
                }
            }
            RuleAction::ClearNote => row.note.take().is_some(),
            RuleAction::StopProcessing => {
                stop = true;
                false
            }
        };
        if changed {
            changed_actions += 1;
        }
    }
    Ok((row, changed_actions, stop))
}

fn persist_rule_row(
    transaction: &Transaction<'_>,
    original: &RuleRow,
    updated: &RuleRow,
) -> Result<(), AutomationRuleError> {
    if original.positive_prompt != updated.positive_prompt
        || original.character_prompt != updated.character_prompt
        || original.negative_prompt != updated.negative_prompt
        || original.artists != updated.artists
        || original.note != updated.note
        || original.group_id != updated.group_id
    {
        transaction.execute(
            "UPDATE rows SET positive_prompt = ?2, character_prompt = ?3,
                    negative_prompt = ?4, artists = ?5, note = ?6, group_id = ?7
             WHERE id = ?1",
            params![
                updated.id,
                updated.positive_prompt,
                updated.character_prompt,
                updated.negative_prompt,
                updated.artists,
                updated.note,
                updated.group_id,
            ],
        )?;
    }
    for tag in original.tags.difference(&updated.tags) {
        transaction.execute(
            "DELETE FROM row_tags WHERE row_id = ?1
             AND tag_id = (SELECT id FROM tags WHERE name = ?2 COLLATE BINARY)",
            params![updated.id, tag],
        )?;
    }
    for tag in updated.tags.difference(&original.tags) {
        transaction.execute(
            "INSERT OR IGNORE INTO row_tags(row_id, tag_id)
             SELECT ?1, id FROM tags WHERE name = ?2 COLLATE BINARY",
            params![updated.id, tag],
        )?;
    }
    Ok(())
}

fn prompt_mut(row: &mut RuleRow, field: PromptActionField) -> &mut Option<String> {
    match field {
        PromptActionField::Positive => &mut row.positive_prompt,
        PromptActionField::Character => &mut row.character_prompt,
        PromptActionField::Negative => &mut row.negative_prompt,
    }
}

fn refresh_artists(row: &mut RuleRow) {
    row.artists = combined_artists(
        row.positive_prompt.as_deref().unwrap_or(""),
        row.character_prompt.as_deref(),
    );
}

fn prompt_scope_text(row: &RuleRow, scope: PromptScope) -> String {
    let fields = match scope {
        PromptScope::Positive => vec![row.positive_prompt.as_deref()],
        PromptScope::Character => vec![row.character_prompt.as_deref()],
        PromptScope::Negative => vec![row.negative_prompt.as_deref()],
        PromptScope::PositiveAndCharacter => vec![
            row.positive_prompt.as_deref(),
            row.character_prompt.as_deref(),
        ],
        PromptScope::All => vec![
            row.positive_prompt.as_deref(),
            row.character_prompt.as_deref(),
            row.negative_prompt.as_deref(),
        ],
    };
    fields
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_prompt_tokens(value: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    value
        .split([',', '，', '\n', '\r'])
        .map(normalize_prompt_token)
        .filter(|token| !token.is_empty() && seen.insert(token.clone()))
        .collect()
}

fn prompt_token_set(value: &str) -> HashSet<String> {
    value
        .split([',', '，', '\n', '\r'])
        .map(normalize_prompt_token)
        .filter(|token| !token.is_empty())
        .collect()
}

/// 规则匹配只移除权重外壳并忽略大小写，不内置角色或语义别名。
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
            token = token[open.len_utf8()..token.len() - close.len_utf8()].trim();
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

fn artist_set(value: Option<&str>) -> HashSet<String> {
    value
        .unwrap_or("")
        .split([',', '，', '\n', '\r'])
        .map(normalize_artist)
        .filter(|artist| !artist.is_empty())
        .collect()
}

fn normalize_artist(value: &str) -> String {
    let normalized = normalize_prompt_token(value);
    normalized
        .strip_prefix("artist:")
        .unwrap_or(&normalized)
        .trim()
        .to_owned()
}

fn normalized_strings(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .flat_map(|value| value.split([',', '，', '\n', '\r']))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .collect()
}

fn append_prompt(current: Option<&str>, value: &str) -> Option<String> {
    let additions = value
        .split([',', '，', '\n', '\r'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if additions.is_empty() {
        return current.map(str::to_owned);
    }
    let mut available = prompt_token_set(current.unwrap_or(""));
    let missing = additions
        .into_iter()
        .filter(|addition| available.insert(normalize_prompt_token(addition)))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return current.map(str::to_owned);
    }
    let addition = missing.join(", ");
    nonempty(
        match current.map(str::trim).filter(|value| !value.is_empty()) {
            Some(current) => format!("{current}, {addition}"),
            None => addition,
        },
    )
}

fn delete_prompt_tags(current: Option<&str>, targets: &str) -> Option<String> {
    let targets = parse_prompt_tokens(targets)
        .into_iter()
        .collect::<HashSet<_>>();
    let current = current?;
    let retained = current
        .split([',', '，', '\n', '\r'])
        .map(str::trim)
        .filter(|fragment| {
            !fragment.is_empty() && !targets.contains(&normalize_prompt_token(fragment))
        })
        .collect::<Vec<_>>();
    nonempty(retained.join(", "))
}

fn replace_text(value: &str, find: &str, replace: &str, case_sensitive: bool) -> String {
    if case_sensitive {
        return value.replace(find, replace);
    }
    RegexBuilder::new(&regex::escape(find))
        .case_insensitive(true)
        .build()
        .map(|regex| regex.replace_all(value, replace).into_owned())
        .unwrap_or_else(|_| value.to_owned())
}

fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn build_regex(value: &str, case_sensitive: bool) -> Result<Regex, AutomationRuleError> {
    RegexBuilder::new(value)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|error| AutomationRuleError::InvalidRegex(error.to_string()))
}

fn text_compare(
    text: &str,
    expected: &str,
    operator: TextOperator,
    case_sensitive: bool,
    regex: Option<&Regex>,
) -> bool {
    match operator {
        TextOperator::Contains if case_sensitive => text.contains(expected),
        TextOperator::Contains => text.to_lowercase().contains(&expected.to_lowercase()),
        TextOperator::Equals if case_sensitive => text == expected,
        TextOperator::Equals => {
            text.eq_ignore_ascii_case(expected) || text.to_lowercase() == expected.to_lowercase()
        }
        TextOperator::Regex => regex.is_some_and(|regex| regex.is_match(text)),
    }
}

fn number_matches(value: f64, comparison: &NumericComparison) -> bool {
    let equals = |left: f64, right: f64| (left - right).abs() <= 1e-9;
    match comparison.operator {
        NumericOperator::Equal => equals(value, comparison.value),
        NumericOperator::NotEqual => !equals(value, comparison.value),
        NumericOperator::GreaterThan => value > comparison.value,
        NumericOperator::GreaterOrEqual => {
            value > comparison.value || equals(value, comparison.value)
        }
        NumericOperator::LessThan => value < comparison.value,
        NumericOperator::LessOrEqual => value < comparison.value || equals(value, comparison.value),
        NumericOperator::Between => comparison.second_value.is_some_and(|end| {
            let (start, end) = if comparison.value <= end {
                (comparison.value, end)
            } else {
                (end, comparison.value)
            };
            (value > start || equals(value, start)) && (value < end || equals(value, end))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{NewRow, RowSelection, SourceType};

    fn comparison(operator: NumericOperator, value: f64, second: Option<f64>) -> NumericComparison {
        NumericComparison {
            operator,
            value,
            second_value: second,
        }
    }

    fn condition_set(condition: RuleCondition) -> RuleConditionSet {
        RuleConditionSet {
            mode: RuleMatchMode::Any,
            negate: false,
            groups: vec![RuleConditionGroup {
                mode: RuleMatchMode::All,
                conditions: vec![condition],
            }],
        }
    }

    fn draft(
        name: &str,
        condition: RuleCondition,
        actions: Vec<RuleAction>,
    ) -> AutomationRuleDraft {
        AutomationRuleDraft {
            name: name.into(),
            description: String::new(),
            enabled: true,
            run_on_import: true,
            run_on_update: false,
            conditions: condition_set(condition),
            actions,
        }
    }

    fn sample_row() -> RuleRow {
        RuleRow {
            id: 1,
            positive_prompt: Some("1.2::girl::, white long hair, blue eyes".into()),
            character_prompt: Some("hair flower(white flower), artist:alice".into()),
            negative_prompt: Some("low quality, bad hands".into()),
            artists: Some("artist:alice\nartist:bob".into()),
            note: Some("favorite portrait".into()),
            group_id: Some(7),
            tags: ["花绘".to_owned(), "OC".to_owned()].into_iter().collect(),
            image_path: Some(r"D:\Pictures\sample.png".into()),
            source_size: Some(2_048),
            metadata_failed: false,
            vibe_count: 2,
            image_width: Some(832),
            image_height: Some(1216),
            generation_model: Some("NovelAI Diffusion V4.5 Full".into()),
            generation_sampler: Some("k_euler_ancestral".into()),
            generation_steps: Some(28),
            generation_seed: Some("18446744073709551615".into()),
            generation_scale: Some(5.5),
            generation_cfg_rescale: Some(0.2),
            generation_noise_schedule: Some("karras".into()),
            source_type: "folder".into(),
            source_path: r"D:\Imports\July".into(),
        }
    }

    fn assert_matches(row: &RuleRow, condition: RuleCondition) {
        let set = condition_set(condition);
        assert!(PreparedConditionSet::new(&set).unwrap().matches(row));
    }

    fn append_row(database: &mut Database, identity: &str, prompt: &str) -> i64 {
        let outcome = database
            .append_batch(
                SourceType::Folder,
                r"D:\Imports",
                &[NewRow {
                    source_ordinal: 1,
                    identity: identity.into(),
                    positive_prompt: Some(prompt.into()),
                    character_prompt: Some("old character, bare_artist".into()),
                    negative_prompt: Some("remove me, keep me".into()),
                    image_path: Some(format!(r"D:\Imports\{identity}.png")),
                    source_size: Some(2048),
                    vibe_reference_count: 2,
                    image_width: Some(832),
                    image_height: Some(1216),
                    generation_model: Some("NovelAI V4.5".into()),
                    generation_sampler: Some("k_euler".into()),
                    generation_steps: Some(28),
                    generation_seed: Some("12345678901234567890".into()),
                    generation_scale: Some(5.0),
                    generation_cfg_rescale: Some(0.2),
                    generation_noise_schedule: Some("karras".into()),
                    ..NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();
        outcome.added_row_ids[0]
    }

    #[test]
    fn every_condition_family_matches_expected_row() {
        let row = sample_row();
        for condition in [
            RuleCondition::Prompt {
                scope: PromptScope::PositiveAndCharacter,
                operator: PromptOperator::ContainsAll,
                value: "girl, white long hair, hair flower(white flower)".into(),
                case_sensitive: false,
            },
            RuleCondition::Prompt {
                scope: PromptScope::Negative,
                operator: PromptOperator::ContainsAny,
                value: "missing, bad hands".into(),
                case_sensitive: false,
            },
            RuleCondition::Prompt {
                scope: PromptScope::Positive,
                operator: PromptOperator::ContainsNone,
                value: "red hair, green eyes".into(),
                case_sensitive: false,
            },
            RuleCondition::Prompt {
                scope: PromptScope::All,
                operator: PromptOperator::TextContains,
                value: "BAD HANDS".into(),
                case_sensitive: false,
            },
            RuleCondition::Prompt {
                scope: PromptScope::Character,
                operator: PromptOperator::TextEquals,
                value: "hair flower(white flower), artist:alice".into(),
                case_sensitive: true,
            },
            RuleCondition::Prompt {
                scope: PromptScope::Positive,
                operator: PromptOperator::Regex,
                value: r"white\s+long\s+hair".into(),
                case_sensitive: false,
            },
            RuleCondition::Tag {
                operator: TagOperator::HasAll,
                tags: vec!["花绘".into(), "OC".into()],
            },
            RuleCondition::Tag {
                operator: TagOperator::HasAny,
                tags: vec!["missing".into(), "OC".into()],
            },
            RuleCondition::Tag {
                operator: TagOperator::HasNone,
                tags: vec!["missing".into()],
            },
            RuleCondition::Group {
                operator: GroupOperator::Is,
                group_id: Some(7),
            },
            RuleCondition::Group {
                operator: GroupOperator::IsNot,
                group_id: Some(8),
            },
            RuleCondition::Artist {
                operator: ArtistOperator::ContainsAny,
                artists: vec!["alice".into()],
            },
            RuleCondition::Artist {
                operator: ArtistOperator::ContainsNone,
                artists: vec!["carol".into()],
            },
            RuleCondition::Artist {
                operator: ArtistOperator::IsMultiple,
                artists: vec![],
            },
            RuleCondition::Note {
                operator: NoteOperator::Contains,
                value: "PORTRAIT".into(),
                case_sensitive: false,
            },
            RuleCondition::FileText {
                field: FileTextField::FileName,
                operator: TextOperator::Equals,
                value: "sample.png".into(),
                case_sensitive: false,
            },
            RuleCondition::FileText {
                field: FileTextField::OriginalPath,
                operator: TextOperator::Regex,
                value: r"Pictures\\sample\.png$".into(),
                case_sensitive: false,
            },
            RuleCondition::FileText {
                field: FileTextField::ImportSource,
                operator: TextOperator::Contains,
                value: "imports".into(),
                case_sensitive: false,
            },
            RuleCondition::FileSize {
                comparison: comparison(NumericOperator::GreaterOrEqual, 2048.0, None),
            },
            RuleCondition::SourceType {
                source_type: RuleSourceType::Folder,
                negate: false,
            },
            RuleCondition::Vibe {
                operator: VibeOperator::HasAny,
                comparison: None,
            },
            RuleCondition::Vibe {
                operator: VibeOperator::Count,
                comparison: Some(comparison(NumericOperator::Equal, 2.0, None)),
            },
            RuleCondition::Metadata { parsed: true },
            RuleCondition::ImageDimension {
                field: ImageDimensionField::Width,
                comparison: comparison(NumericOperator::Equal, 832.0, None),
            },
            RuleCondition::ImageDimension {
                field: ImageDimensionField::Height,
                comparison: comparison(NumericOperator::GreaterThan, 1000.0, None),
            },
            RuleCondition::ImageDimension {
                field: ImageDimensionField::AspectRatio,
                comparison: comparison(NumericOperator::Between, 0.68, Some(0.69)),
            },
            RuleCondition::Orientation {
                orientation: ImageOrientation::Portrait,
                negate: false,
            },
            RuleCondition::GenerationText {
                field: GenerationTextField::Model,
                operator: TextOperator::Contains,
                value: "v4.5".into(),
                case_sensitive: false,
            },
            RuleCondition::GenerationText {
                field: GenerationTextField::Sampler,
                operator: TextOperator::Equals,
                value: "k_euler_ancestral".into(),
                case_sensitive: true,
            },
            RuleCondition::GenerationText {
                field: GenerationTextField::NoiseSchedule,
                operator: TextOperator::Regex,
                value: "^kar+as$".into(),
                case_sensitive: false,
            },
            RuleCondition::GenerationText {
                field: GenerationTextField::Seed,
                operator: TextOperator::Equals,
                value: "18446744073709551615".into(),
                case_sensitive: true,
            },
            RuleCondition::GenerationNumber {
                field: GenerationNumberField::Steps,
                comparison: comparison(NumericOperator::Equal, 28.0, None),
            },
            RuleCondition::GenerationNumber {
                field: GenerationNumberField::Scale,
                comparison: comparison(NumericOperator::LessOrEqual, 5.5, None),
            },
            RuleCondition::GenerationNumber {
                field: GenerationNumberField::CfgRescale,
                comparison: comparison(NumericOperator::NotEqual, 0.0, None),
            },
        ] {
            assert_matches(&row, condition);
        }

        let mut empty = row.clone();
        empty.tags.clear();
        empty.group_id = None;
        empty.artists = None;
        empty.note = Some("  ".into());
        empty.vibe_count = 0;
        empty.source_type = "archive".into();
        assert_matches(
            &empty,
            RuleCondition::Tag {
                operator: TagOperator::IsEmpty,
                tags: vec![],
            },
        );
        assert_matches(
            &empty,
            RuleCondition::Group {
                operator: GroupOperator::IsEmpty,
                group_id: None,
            },
        );
        assert_matches(
            &empty,
            RuleCondition::Artist {
                operator: ArtistOperator::IsEmpty,
                artists: vec![],
            },
        );
        assert_matches(
            &empty,
            RuleCondition::Note {
                operator: NoteOperator::IsEmpty,
                value: String::new(),
                case_sensitive: false,
            },
        );
        assert_matches(
            &empty,
            RuleCondition::Vibe {
                operator: VibeOperator::HasNone,
                comparison: None,
            },
        );
        assert_matches(
            &empty,
            RuleCondition::SourceType {
                source_type: RuleSourceType::Folder,
                negate: true,
            },
        );
        empty.artists = Some("artist:alice".into());
        assert_matches(
            &empty,
            RuleCondition::Artist {
                operator: ArtistOperator::IsSingle,
                artists: vec![],
            },
        );
        empty.metadata_failed = true;
        assert_matches(&empty, RuleCondition::Metadata { parsed: false });
        assert_matches(
            &empty,
            RuleCondition::Orientation {
                orientation: ImageOrientation::Landscape,
                negate: true,
            },
        );
    }

    #[test]
    fn condition_groups_support_and_or_and_whole_expression_negation() {
        let row = sample_row();
        let group_a = RuleConditionGroup {
            mode: RuleMatchMode::All,
            conditions: vec![
                RuleCondition::Prompt {
                    scope: PromptScope::PositiveAndCharacter,
                    operator: PromptOperator::ContainsAll,
                    value: "girl, white long hair, blue eyes, hair flower(white flower)".into(),
                    case_sensitive: false,
                },
                RuleCondition::Tag {
                    operator: TagOperator::HasAny,
                    tags: vec!["OC".into()],
                },
            ],
        };
        let group_b = RuleConditionGroup {
            mode: RuleMatchMode::All,
            conditions: vec![RuleCondition::Prompt {
                scope: PromptScope::Positive,
                operator: PromptOperator::ContainsAll,
                value: "red hair, green eyes".into(),
                case_sensitive: false,
            }],
        };
        let set = RuleConditionSet {
            mode: RuleMatchMode::Any,
            negate: false,
            groups: vec![group_a, group_b],
        };
        assert!(PreparedConditionSet::new(&set).unwrap().matches(&row));

        let negated = RuleConditionSet {
            negate: true,
            ..set
        };
        assert!(!PreparedConditionSet::new(&negated).unwrap().matches(&row));
    }

    #[test]
    fn prompt_matching_is_exact_per_tag_and_has_no_character_aliases() {
        let row = sample_row();
        for value in ["1girl", "long hair", "blue eye"] {
            let set = condition_set(RuleCondition::Prompt {
                scope: PromptScope::Positive,
                operator: PromptOperator::ContainsAll,
                value: value.into(),
                case_sensitive: false,
            });
            assert!(!PreparedConditionSet::new(&set).unwrap().matches(&row));
        }
    }

    #[test]
    fn full_width_comma_safely_splits_all_list_style_inputs() {
        assert_eq!(
            normalized_strings(&["alice，bob, carol\ndave".into()]),
            vec!["alice", "bob", "carol", "dave"]
        );

        let mut database = Database::open_in_memory().unwrap();
        let row_id = append_row(&mut database, "full-width-comma", "girl, blue eyes");
        let rule = database
            .create_automation_rule(&draft(
                "全角逗号防呆",
                RuleCondition::Prompt {
                    scope: PromptScope::Positive,
                    operator: PromptOperator::ContainsAll,
                    value: "girl，blue eyes".into(),
                    case_sensitive: false,
                },
                vec![RuleAction::AddTags {
                    tags: vec!["人物，蓝眼".into()],
                }],
            ))
            .unwrap();

        let result = database.run_automation_rule_on_library(rule.id).unwrap();
        assert_eq!(result.changed_rows, 1);
        let tags = {
            let mut statement = database
                .connection
                .prepare(
                    "SELECT tags.name FROM row_tags
                     JOIN tags ON tags.id = row_tags.tag_id
                     WHERE row_tags.row_id = ?1 ORDER BY tags.name",
                )
                .unwrap();
            statement
                .query_map([row_id], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(tags, vec!["人物", "蓝眼"]);
    }

    #[test]
    fn every_numeric_operator_has_defined_boundary_behavior() {
        assert!(number_matches(
            5.0,
            &comparison(NumericOperator::Equal, 5.0, None)
        ));
        assert!(number_matches(
            5.0,
            &comparison(NumericOperator::NotEqual, 4.0, None)
        ));
        assert!(number_matches(
            5.0,
            &comparison(NumericOperator::GreaterThan, 4.0, None)
        ));
        assert!(number_matches(
            5.0,
            &comparison(NumericOperator::GreaterOrEqual, 5.0, None)
        ));
        assert!(number_matches(
            5.0,
            &comparison(NumericOperator::LessThan, 6.0, None)
        ));
        assert!(number_matches(
            5.0,
            &comparison(NumericOperator::LessOrEqual, 5.0, None)
        ));
        assert!(number_matches(
            5.0,
            &comparison(NumericOperator::Between, 5.0, Some(9.0))
        ));
        assert!(number_matches(
            5.0,
            &comparison(NumericOperator::Between, 9.0, Some(5.0))
        ));
    }

    #[test]
    fn rule_crud_persists_definition_and_normalizes_order_after_delete() {
        let mut database = Database::open_in_memory().unwrap();
        assert!(database.list_automation_rules().unwrap().is_empty());
        let condition = RuleCondition::Metadata { parsed: true };
        let first = database
            .create_automation_rule(&draft(
                "第一条",
                condition.clone(),
                vec![RuleAction::AddTags {
                    tags: vec!["A".into()],
                }],
            ))
            .unwrap();
        let second = database
            .create_automation_rule(&draft(
                "第二条",
                condition,
                vec![RuleAction::AddTags {
                    tags: vec!["B".into()],
                }],
            ))
            .unwrap();

        database
            .reorder_automation_rules(&[second.id, first.id])
            .unwrap();
        assert_eq!(
            database
                .list_automation_rules()
                .unwrap()
                .iter()
                .map(|rule| rule.name.as_str())
                .collect::<Vec<_>>(),
            vec!["第二条", "第一条"]
        );
        database
            .set_automation_rule_enabled(second.id, false)
            .unwrap();
        assert!(!database.list_automation_rules().unwrap()[0].enabled);
        assert!(database.delete_automation_rule(second.id).unwrap());
        let remaining = database.list_automation_rules().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].position, 0);
    }

    #[test]
    fn all_mutating_actions_apply_in_sequence_and_manual_preview_is_non_mutating() {
        let mut database = Database::open_in_memory().unwrap();
        let row_id = append_row(&mut database, "actions", "target, bare_artist");
        database.create_tag("旧标签").unwrap();
        database
            .set_tags_for_row(row_id, &["旧标签".into()])
            .unwrap();
        let group = database.create_group("目标分组").unwrap();
        let rule = database
            .create_automation_rule(&draft(
                "动作全集",
                RuleCondition::Prompt {
                    scope: PromptScope::Positive,
                    operator: PromptOperator::ContainsAll,
                    value: "target".into(),
                    case_sensitive: false,
                },
                vec![
                    RuleAction::AddTags {
                        tags: vec!["新标签".into()],
                    },
                    RuleAction::RemoveTags {
                        tags: vec!["旧标签".into()],
                    },
                    RuleAction::SetGroup {
                        group_id: group.id,
                        only_if_ungrouped: false,
                    },
                    RuleAction::AppendPrompt {
                        field: PromptActionField::Positive,
                        value: "added prompt".into(),
                    },
                    RuleAction::DeletePromptTags {
                        field: PromptActionField::Negative,
                        value: "remove me".into(),
                    },
                    RuleAction::ReplacePrompt {
                        field: PromptActionField::Character,
                        find: "old character".into(),
                        replace: "new character".into(),
                        case_sensitive: true,
                    },
                    RuleAction::PrefixArtist {
                        artists: vec!["bare_artist".into()],
                    },
                    RuleAction::SetNote {
                        value: "base".into(),
                    },
                    RuleAction::AppendNote {
                        value: "tail".into(),
                        separator: " | ".into(),
                    },
                ],
            ))
            .unwrap();

        let preview = database.preview_automation_rule(rule.id).unwrap();
        assert_eq!(preview.matched_rows, 1);
        assert_eq!(preview.rows_needing_changes, 1);
        let before_note: Option<String> = database
            .connection
            .query_row("SELECT note FROM rows WHERE id = ?1", [row_id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(before_note, None);

        let result = database.run_automation_rule_on_library(rule.id).unwrap();
        assert_eq!(result.changed_rows, 1);
        assert_eq!(result.reports[0].actions_changed, 9);
        let stored: (String, String, String, Option<String>, Option<i64>) = database
            .connection
            .query_row(
                "SELECT positive_prompt, character_prompt, negative_prompt, note, group_id
                 FROM rows WHERE id = ?1",
                [row_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert!(stored.0.contains("added prompt"));
        assert!(stored.0.contains("artist:bare_artist"));
        assert!(stored.1.contains("new character"));
        assert!(stored.1.contains("artist:bare_artist"));
        assert_eq!(stored.2, "keep me");
        assert_eq!(stored.3.as_deref(), Some("base | tail"));
        assert_eq!(stored.4, Some(group.id));
        let tags = database
            .list_selection_tags(&RowSelection::Explicit {
                row_ids: vec![row_id],
            })
            .unwrap();
        assert_eq!(
            tags.iter()
                .filter(|tag| tag.selected_rows > 0)
                .map(|tag| tag.name.as_str())
                .collect::<Vec<_>>(),
            vec!["新标签"]
        );

        let clear = database
            .create_automation_rule(&draft(
                "清理",
                RuleCondition::Tag {
                    operator: TagOperator::HasAny,
                    tags: vec!["新标签".into()],
                },
                vec![RuleAction::ClearGroup, RuleAction::ClearNote],
            ))
            .unwrap();
        database.run_automation_rule_on_library(clear.id).unwrap();
        let cleared: (Option<i64>, Option<String>) = database
            .connection
            .query_row(
                "SELECT group_id, note FROM rows WHERE id = ?1",
                [row_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(cleared, (None, None));
    }

    #[test]
    fn only_ungrouped_action_skips_existing_group() {
        let mut database = Database::open_in_memory().unwrap();
        let row_id = append_row(&mut database, "grouped", "target");
        let original = database.create_group("原分组").unwrap();
        let target = database.create_group("目标分组").unwrap();
        database
            .assign_rows_to_group(
                &RowSelection::Explicit {
                    row_ids: vec![row_id],
                },
                original.id,
            )
            .unwrap();
        let rule = database
            .create_automation_rule(&draft(
                "不抢已有分组",
                RuleCondition::Metadata { parsed: true },
                vec![RuleAction::SetGroup {
                    group_id: target.id,
                    only_if_ungrouped: true,
                }],
            ))
            .unwrap();
        let result = database.run_automation_rule_on_library(rule.id).unwrap();
        assert_eq!(result.changed_rows, 0);
        let group_id: i64 = database
            .connection
            .query_row("SELECT group_id FROM rows WHERE id = ?1", [row_id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(group_id, original.id);
    }

    #[test]
    fn stop_processing_only_removes_matching_rows_from_later_rules() {
        let mut database = Database::open_in_memory().unwrap();
        let stopped = append_row(&mut database, "stopped", "stop, target");
        let continuing = append_row(&mut database, "continuing", "target");
        database
            .create_automation_rule(&draft(
                "停止一张",
                RuleCondition::Prompt {
                    scope: PromptScope::Positive,
                    operator: PromptOperator::ContainsAll,
                    value: "stop".into(),
                    case_sensitive: false,
                },
                vec![
                    RuleAction::AddTags {
                        tags: vec!["先执行".into()],
                    },
                    RuleAction::StopProcessing,
                ],
            ))
            .unwrap();
        database
            .create_automation_rule(&draft(
                "后续规则",
                RuleCondition::Prompt {
                    scope: PromptScope::Positive,
                    operator: PromptOperator::ContainsAll,
                    value: "target".into(),
                    case_sensitive: false,
                },
                vec![RuleAction::AddTags {
                    tags: vec!["后执行".into()],
                }],
            ))
            .unwrap();

        let result = database
            .execute_automation_rules(RuleExecutionTrigger::Import, &[stopped, continuing])
            .unwrap();
        assert_eq!(result.reports.len(), 2);
        assert_eq!(result.reports[0].stopped_rows, 1);
        assert_eq!(result.reports[1].scanned_rows, 1);
        assert!(row_has_tag(&database, stopped, "先执行"));
        assert!(!row_has_tag(&database, stopped, "后执行"));
        assert!(row_has_tag(&database, continuing, "后执行"));
    }

    #[test]
    fn broken_rule_is_reported_and_later_rules_continue() {
        let mut database = Database::open_in_memory().unwrap();
        let row_id = append_row(&mut database, "errors", "target");
        let group = database.create_group("即将删除").unwrap();
        let broken = database
            .create_automation_rule(&draft(
                "失效规则",
                RuleCondition::Metadata { parsed: true },
                vec![RuleAction::SetGroup {
                    group_id: group.id,
                    only_if_ungrouped: false,
                }],
            ))
            .unwrap();
        database.delete_group(group.id).unwrap();
        database
            .create_automation_rule(&draft(
                "仍然执行",
                RuleCondition::Metadata { parsed: true },
                vec![RuleAction::AddTags {
                    tags: vec!["成功".into()],
                }],
            ))
            .unwrap();

        let result = database
            .execute_automation_rules(RuleExecutionTrigger::Import, &[row_id])
            .unwrap();
        assert_eq!(result.reports.len(), 2);
        assert_eq!(result.reports[0].rule_id, broken.id);
        assert!(result.reports[0].error.is_some());
        assert_eq!(result.reports[1].changed_rows, 1);
        assert!(row_has_tag(&database, row_id, "成功"));
    }

    #[test]
    fn trigger_flags_are_independent() {
        let mut database = Database::open_in_memory().unwrap();
        let row_id = append_row(&mut database, "triggers", "target");
        let mut import_draft = draft(
            "仅导入",
            RuleCondition::Metadata { parsed: true },
            vec![RuleAction::AddTags {
                tags: vec!["import".into()],
            }],
        );
        import_draft.run_on_update = false;
        database.create_automation_rule(&import_draft).unwrap();
        let mut update_draft = draft(
            "仅更新",
            RuleCondition::Metadata { parsed: true },
            vec![RuleAction::AddTags {
                tags: vec!["update".into()],
            }],
        );
        update_draft.run_on_import = false;
        update_draft.run_on_update = true;
        database.create_automation_rule(&update_draft).unwrap();

        database
            .execute_automation_rules(RuleExecutionTrigger::Import, &[row_id])
            .unwrap();
        assert!(row_has_tag(&database, row_id, "import"));
        assert!(!row_has_tag(&database, row_id, "update"));
        database
            .execute_automation_rules(RuleExecutionTrigger::Update, &[row_id])
            .unwrap();
        assert!(row_has_tag(&database, row_id, "update"));
    }

    fn row_has_tag(database: &Database, row_id: i64, tag: &str) -> bool {
        database
            .connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM row_tags JOIN tags ON tags.id = row_tags.tag_id
                    WHERE row_tags.row_id = ?1 AND tags.name = ?2 COLLATE BINARY
                 )",
                params![row_id, tag],
                |row| row.get(0),
            )
            .unwrap()
    }
}
