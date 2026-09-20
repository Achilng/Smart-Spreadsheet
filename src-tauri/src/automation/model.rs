use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
    SetNoteSequence {
        prefix: String,
    },
    AppendNote {
        value: String,
        #[serde(default = "default_note_separator")]
        separator: String,
    },
    ClearNote,
    StopProcessing,
}

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn default_note_separator() -> String {
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

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRuleImportPreview {
    pub name: String,
    pub imported_name: String,
    pub condition_count: u32,
    pub action_count: u32,
    pub run_on_import: bool,
    pub run_on_update: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRuleImportInspection {
    pub content_hash: String,
    pub version: u32,
    pub rule_count: u32,
    pub rules: Vec<AutomationRuleImportPreview>,
    pub missing_tags: Vec<String>,
    pub missing_groups: Vec<String>,
    pub renamed_rules: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRuleImportResult {
    pub imported_rules: u32,
    pub created_tags: u32,
    pub created_groups: u32,
    pub renamed_rules: u32,
    pub imported_rule_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRuleExportResult {
    pub path: String,
    pub exported_rules: u32,
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

#[derive(Debug, Clone)]
pub(crate) struct RuleRow {
    pub(crate) id: i64,
    pub(crate) positive_prompt: Option<String>,
    pub(crate) character_prompt: Option<String>,
    pub(crate) negative_prompt: Option<String>,
    pub(crate) artists: Option<String>,
    pub(crate) note: Option<String>,
    pub(crate) group_id: Option<i64>,
    pub(crate) tags: HashSet<String>,
    pub(crate) image_path: Option<String>,
    pub(crate) source_size: Option<i64>,
    pub(crate) metadata_failed: bool,
    pub(crate) vibe_count: u32,
    pub(crate) image_width: Option<u32>,
    pub(crate) image_height: Option<u32>,
    pub(crate) generation_model: Option<String>,
    pub(crate) generation_sampler: Option<String>,
    pub(crate) generation_steps: Option<u32>,
    pub(crate) generation_seed: Option<String>,
    pub(crate) generation_scale: Option<f64>,
    pub(crate) generation_cfg_rescale: Option<f64>,
    pub(crate) generation_noise_schedule: Option<String>,
    pub(crate) source_type: String,
    pub(crate) source_path: String,
}
