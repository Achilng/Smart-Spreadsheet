use crate::automation::error::AutomationRuleError;
use crate::automation::model::{
    ArtistOperator, FileTextField, GenerationNumberField, GenerationTextField, GroupOperator,
    ImageDimensionField, ImageOrientation, NoteOperator, NumericComparison, NumericOperator,
    PromptOperator, PromptScope, RuleCondition, RuleConditionSet, RuleMatchMode, RuleRow,
    RuleSourceType, TagOperator, TextOperator, VibeOperator,
};
use crate::automation::text::{
    artist_set, normalize_artist, normalized_strings, parse_prompt_tokens, prompt_token_set,
};
use regex::{Regex, RegexBuilder};
use std::path::Path;

pub(crate) struct PreparedCondition<'a> {
    pub(crate) condition: &'a RuleCondition,
    pub(crate) tokens: Vec<String>,
    pub(crate) regex: Option<Regex>,
}

pub(crate) struct PreparedConditionSet<'a> {
    pub(crate) source: &'a RuleConditionSet,
    pub(crate) groups: Vec<Vec<PreparedCondition<'a>>>,
}

impl<'a> PreparedConditionSet<'a> {
    pub(crate) fn new(source: &'a RuleConditionSet) -> Result<Self, AutomationRuleError> {
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

    pub(crate) fn matches(&self, row: &RuleRow) -> bool {
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
    pub(crate) fn new(condition: &'a RuleCondition) -> Result<Self, AutomationRuleError> {
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

    pub(crate) fn matches(&self, row: &RuleRow) -> bool {
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

pub(crate) fn prompt_scope_text(row: &RuleRow, scope: PromptScope) -> String {
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

pub(crate) fn build_regex(value: &str, case_sensitive: bool) -> Result<Regex, AutomationRuleError> {
    RegexBuilder::new(value)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|error| AutomationRuleError::InvalidRegex(error.to_string()))
}

pub(crate) fn text_compare(
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

pub(crate) fn number_matches(value: f64, comparison: &NumericComparison) -> bool {
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
