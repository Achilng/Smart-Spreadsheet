use rusqlite::types::Value;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterTagOperator {
    HasAll,
    HasAny,
    HasNone,
    IsEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterGroupOperator {
    Is,
    IsNot,
    IsEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterArtistOperator {
    ContainsAny,
    ContainsNone,
    IsSingle,
    IsMultiple,
    IsEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterVibeOperator {
    HasAny,
    HasNone,
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterNoteOperator {
    Contains,
    IsEmpty,
    IsNotEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterNumericOperator {
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
pub struct FilterNumericComparison {
    pub operator: FilterNumericOperator,
    pub value: f64,
    pub second_value: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterImageDimensionField {
    Width,
    Height,
    AspectRatio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOrientation {
    Landscape,
    Portrait,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterGenerationTextField {
    Model,
    Sampler,
    NoiseSchedule,
    Seed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterTextOperator {
    Contains,
    Equals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterGenerationNumberField {
    Steps,
    Scale,
    CfgRescale,
}

/// 资料库浏览使用的临时筛选条件。所有条件之间固定为 AND；单个条件内部的
/// “全部/任意/不包含”由对应 operator 表达。该结构不落库，避免与自动规则耦合。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LibraryFilter {
    Tag {
        operator: FilterTagOperator,
        #[serde(default)]
        values: Vec<String>,
    },
    Group {
        operator: FilterGroupOperator,
        group_id: Option<i64>,
    },
    Artist {
        operator: FilterArtistOperator,
        #[serde(default)]
        values: Vec<String>,
    },
    Vibe {
        operator: FilterVibeOperator,
        comparison: Option<FilterNumericComparison>,
    },
    Note {
        operator: FilterNoteOperator,
        #[serde(default)]
        value: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    Metadata {
        parsed: bool,
    },
    ImageDimension {
        field: FilterImageDimensionField,
        comparison: FilterNumericComparison,
    },
    Orientation {
        orientation: FilterOrientation,
    },
    GenerationText {
        field: FilterGenerationTextField,
        operator: FilterTextOperator,
        value: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    GenerationNumber {
        field: FilterGenerationNumberField,
        comparison: FilterNumericComparison,
    },
}

fn normalized_values(values: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values
        .iter()
        .flat_map(|value| value.split([',', '，', '\n', '\r']))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !normalized.iter().any(|existing| existing == value) {
            normalized.push(value.to_owned());
        }
    }
    normalized
}

fn push_text(params: &mut Vec<Value>, value: String) -> String {
    params.push(Value::Text(value));
    format!("?{}", params.len())
}

fn push_real(params: &mut Vec<Value>, value: f64) -> String {
    params.push(Value::Real(value));
    format!("?{}", params.len())
}

fn numeric_predicate(
    expression: &str,
    comparison: &FilterNumericComparison,
    params: &mut Vec<Value>,
) -> String {
    let first = push_real(params, comparison.value);
    match comparison.operator {
        FilterNumericOperator::Equal => format!("{expression} = {first}"),
        FilterNumericOperator::NotEqual => format!("{expression} != {first}"),
        FilterNumericOperator::GreaterThan => format!("{expression} > {first}"),
        FilterNumericOperator::GreaterOrEqual => format!("{expression} >= {first}"),
        FilterNumericOperator::LessThan => format!("{expression} < {first}"),
        FilterNumericOperator::LessOrEqual => format!("{expression} <= {first}"),
        FilterNumericOperator::Between => {
            let second = push_real(params, comparison.second_value.unwrap_or(comparison.value));
            format!("{expression} BETWEEN MIN({first}, {second}) AND MAX({first}, {second})")
        }
    }
}

fn text_predicate(
    expression: &str,
    operator: FilterTextOperator,
    value: &str,
    case_sensitive: bool,
    params: &mut Vec<Value>,
) -> String {
    let value = if case_sensitive {
        value.to_owned()
    } else {
        value.to_lowercase()
    };
    let parameter = push_text(params, value);
    let expression = if case_sensitive {
        format!("COALESCE({expression}, '')")
    } else {
        format!("LOWER(COALESCE({expression}, ''))")
    };
    match operator {
        FilterTextOperator::Contains => format!("INSTR({expression}, {parameter}) > 0"),
        FilterTextOperator::Equals => format!("{expression} = {parameter}"),
    }
}

fn artist_token_expression() -> &'static str {
    // 新格式逐行保存；REPLACE 同时防御尚未完成重建的逗号分隔旧格式。
    "CHAR(10) || LOWER(REPLACE(REPLACE(REPLACE(REPLACE(TRIM(COALESCE(rows.artists, '')), 'artist:', ''), CHAR(13), CHAR(10)), ',', CHAR(10)), '，', CHAR(10))) || CHAR(10)"
}

fn artist_contains(value: &str, params: &mut Vec<Value>) -> String {
    let lowered = value.trim().to_lowercase();
    let normalized = lowered
        .strip_prefix("artist:")
        .unwrap_or(&lowered)
        .trim()
        .to_owned();
    let parameter = push_text(params, format!("\n{normalized}\n"));
    format!("INSTR({}, {parameter}) > 0", artist_token_expression())
}

fn compile_filter(filter: &LibraryFilter, params: &mut Vec<Value>) -> String {
    match filter {
        LibraryFilter::Tag { operator, values } => {
            let values = normalized_values(values);
            match operator {
                FilterTagOperator::IsEmpty =>
                    "NOT EXISTS (SELECT 1 FROM row_tags WHERE row_tags.row_id = rows.id)".into(),
                FilterTagOperator::HasAll => {
                    if values.is_empty() {
                        return "1".into();
                    }
                    values
                        .into_iter()
                        .map(|value| {
                            let parameter = push_text(params, value);
                            format!(
                                "EXISTS (SELECT 1 FROM row_tags JOIN tags ON tags.id = row_tags.tag_id WHERE row_tags.row_id = rows.id AND tags.name = {parameter} COLLATE BINARY)"
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" AND ")
                }
                FilterTagOperator::HasAny | FilterTagOperator::HasNone => {
                    if values.is_empty() {
                        return if matches!(operator, FilterTagOperator::HasAny) { "0" } else { "1" }.into();
                    }
                    let parameters = values
                        .into_iter()
                        .map(|value| push_text(params, value))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let exists = format!(
                        "EXISTS (SELECT 1 FROM row_tags JOIN tags ON tags.id = row_tags.tag_id WHERE row_tags.row_id = rows.id AND tags.name COLLATE BINARY IN ({parameters}))"
                    );
                    if matches!(operator, FilterTagOperator::HasNone) {
                        format!("NOT ({exists})")
                    } else {
                        exists
                    }
                }
            }
        }
        LibraryFilter::Group { operator, group_id } => match operator {
            FilterGroupOperator::IsEmpty => "rows.group_id IS NULL".into(),
            FilterGroupOperator::Is => group_id.map_or_else(
                || "0".into(),
                |id| {
                    params.push(Value::Integer(id));
                    format!("rows.group_id = ?{}", params.len())
                },
            ),
            FilterGroupOperator::IsNot => group_id.map_or_else(
                || "1".into(),
                |id| {
                    params.push(Value::Integer(id));
                    format!("(rows.group_id IS NULL OR rows.group_id != ?{})", params.len())
                },
            ),
        },
        LibraryFilter::Artist { operator, values } => {
            let values = normalized_values(values);
            match operator {
                FilterArtistOperator::IsEmpty => "TRIM(COALESCE(rows.artists, '')) = ''".into(),
                FilterArtistOperator::IsSingle =>
                    "TRIM(COALESCE(rows.artists, '')) != '' AND INSTR(rows.artists, CHAR(10)) = 0 AND INSTR(rows.artists, ',') = 0 AND INSTR(rows.artists, '，') = 0".into(),
                FilterArtistOperator::IsMultiple =>
                    "TRIM(COALESCE(rows.artists, '')) != '' AND (INSTR(rows.artists, CHAR(10)) > 0 OR INSTR(rows.artists, ',') > 0 OR INSTR(rows.artists, '，') > 0)".into(),
                FilterArtistOperator::ContainsAny | FilterArtistOperator::ContainsNone => {
                    if values.is_empty() {
                        return if matches!(operator, FilterArtistOperator::ContainsAny) { "0" } else { "1" }.into();
                    }
                    let joined = values
                        .iter()
                        .map(|value| artist_contains(value, params))
                        .collect::<Vec<_>>()
                        .join(" OR ");
                    if matches!(operator, FilterArtistOperator::ContainsNone) {
                        format!("NOT ({joined})")
                    } else {
                        format!("({joined})")
                    }
                }
            }
        }
        LibraryFilter::Vibe { operator, comparison } => match operator {
            FilterVibeOperator::HasAny => "COALESCE(rows.vibe_reference_count, 0) > 0".into(),
            FilterVibeOperator::HasNone => "COALESCE(rows.vibe_reference_count, 0) = 0".into(),
            FilterVibeOperator::Count => comparison.as_ref().map_or_else(
                || "0".into(),
                |comparison| numeric_predicate("COALESCE(rows.vibe_reference_count, 0)", comparison, params),
            ),
        },
        LibraryFilter::Note { operator, value, case_sensitive } => match operator {
            FilterNoteOperator::IsEmpty => "TRIM(COALESCE(rows.note, '')) = ''".into(),
            FilterNoteOperator::IsNotEmpty => "TRIM(COALESCE(rows.note, '')) != ''".into(),
            FilterNoteOperator::Contains => text_predicate(
                "rows.note",
                FilterTextOperator::Contains,
                value.trim(),
                *case_sensitive,
                params,
            ),
        },
        LibraryFilter::Metadata { parsed } => {
            if *parsed { "rows.metadata_failed = 0" } else { "rows.metadata_failed = 1" }.into()
        }
        LibraryFilter::ImageDimension { field, comparison } => {
            let expression = match field {
                FilterImageDimensionField::Width => "rows.image_width",
                FilterImageDimensionField::Height => "rows.image_height",
                FilterImageDimensionField::AspectRatio =>
                    "(CAST(rows.image_width AS REAL) / NULLIF(rows.image_height, 0))",
            };
            numeric_predicate(expression, comparison, params)
        }
        LibraryFilter::Orientation { orientation } => match orientation {
            FilterOrientation::Landscape => "rows.image_width > rows.image_height".into(),
            FilterOrientation::Portrait => "rows.image_height > rows.image_width".into(),
            FilterOrientation::Square => "rows.image_width = rows.image_height".into(),
        },
        LibraryFilter::GenerationText { field, operator, value, case_sensitive } => {
            let expression = match field {
                FilterGenerationTextField::Model => "rows.generation_model",
                FilterGenerationTextField::Sampler => "rows.generation_sampler",
                FilterGenerationTextField::NoiseSchedule => "rows.generation_noise_schedule",
                FilterGenerationTextField::Seed => "rows.generation_seed",
            };
            text_predicate(expression, *operator, value.trim(), *case_sensitive, params)
        }
        LibraryFilter::GenerationNumber { field, comparison } => {
            let expression = match field {
                FilterGenerationNumberField::Steps => "rows.generation_steps",
                FilterGenerationNumberField::Scale => "rows.generation_scale",
                FilterGenerationNumberField::CfgRescale => "rows.generation_cfg_rescale",
            };
            numeric_predicate(expression, comparison, params)
        }
    }
}

pub(super) fn append_library_filters(
    mut predicate: String,
    filters: &[LibraryFilter],
    params: &mut Vec<Value>,
) -> String {
    for filter in filters {
        let compiled = compile_filter(filter, params);
        predicate = format!("({predicate}) AND ({compiled})");
    }
    predicate
}
