use super::compare::{CompareSectionPage, validate_page};
use super::library_filters::append_library_filters;
use super::{Database, DatabaseError, LibraryFilter};
use rusqlite::{Connection, functions::FunctionFlags, types::Value};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareConditions {
    rules: Vec<Rule>,
    filters: Vec<LibraryFilter>,
    // The UI uses the same model-version mapping as the built-in model groups.
    equivalent_models: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rule {
    field: Field,
    relation: Relation,
    #[serde(default)]
    words: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Relation {
    Same,
    Different,
    OnlyWords,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Field {
    Artists,
    Style,
    Vibe,
    Positive,
    Character,
    Negative,
    Model,
    Sampler,
    Steps,
    Seed,
    Scale,
    CfgRescale,
    NoiseSchedule,
    Dimensions,
}

fn tokens(value: &str) -> Vec<String> {
    value
        .split([',', '\n', '\r'])
        .map(|s| {
            s.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

pub(super) fn register(connection: &Connection) -> rusqlite::Result<()> {
    connection.create_scalar_function(
        "compare_prompt_key",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let source = ctx.get::<Option<String>>(0)?.unwrap_or_default();
            let ignored: String = ctx.get(1)?;
            let mut words = tokens(&source);
            if !ignored.is_empty() {
                let excluded = tokens(&ignored);
                words.retain(|word| !excluded.contains(word));
            }
            words.sort_unstable();
            Ok(words.join(","))
        },
    )
}

// The sample subquery is uncorrelated, so SQLite computes its key once.
fn compare(expression: &str, operator: &str) -> String {
    format!("{expression} {operator} (SELECT {expression} FROM rows WHERE id = ?1)")
}

impl Database {
    pub fn query_compare_custom(
        &mut self,
        row_id: i64,
        conditions: &CompareConditions,
        offset: u64,
        limit: u32,
    ) -> Result<CompareSectionPage, DatabaseError> {
        let start = validate_page(offset, limit)?;
        let mut params = vec![Value::Integer(row_id)];
        let mut predicate =
            append_library_filters("rows.id != ?1".into(), &conditions.filters, &mut params);
        for rule in &conditions.rules {
            let same = matches!(rule.relation, Relation::Same);
            let op = if same { "=" } else { "!=" };
            let term = match rule.field {
                Field::Positive | Field::Character | Field::Negative => {
                    let col = match rule.field {
                        Field::Positive => "positive_prompt",
                        Field::Character => "character_prompt",
                        _ => "negative_prompt",
                    };
                    // A parsed image may legitimately have an empty prompt.
                    let expression = format!(
                        "CASE WHEN {col} IS NOT NULL OR metadata_failed = 0 THEN compare_prompt_key({col}, '') END"
                    );
                    let mut term = compare(&expression, op);
                    if matches!(rule.relation, Relation::OnlyWords) {
                        params.push(Value::Text(rule.words.clone()));
                        let remainder = format!("compare_prompt_key({col}, ?{})", params.len());
                        term.push_str(" AND ");
                        term.push_str(&compare(&remainder, "="));
                    }
                    term
                }
                Field::Artists => format!("artist_match_key {op} (SELECT NULLIF(artist_match_key,'') FROM rows WHERE id=?1) AND artist_match_key != ''"),
                Field::Model => {
                    params.push(Value::Text(
                        serde_json::json!(conditions.equivalent_models).to_string(),
                    ));
                    let list = format!("(SELECT value FROM json_each(?{}))", params.len());
                    let membership = if same { "IN" } else { "NOT IN" };
                    format!(
                        "NULLIF(TRIM(generation_model),'') IS NOT NULL AND (SELECT generation_model FROM rows WHERE id=?1) IN {list} AND generation_model {membership} {list}"
                    )
                }
                Field::Vibe => {
                    // NULL is no reference, an empty signature marks an unreadable source.
                    let expression = if same {
                        "NULLIF(vibe_signature,'')"
                    } else {
                        "CASE WHEN vibe_signature IS NULL THEN 'no-reference' ELSE NULLIF(vibe_signature,'') END"
                    };
                    compare(expression, op)
                }
                field => {
                    let expression = match field {
                        Field::Style if same => "style_signature",
                        Field::Style => "COALESCE(style_signature,'')",
                        Field::Steps => "generation_steps",
                        Field::Seed => "NULLIF(generation_seed,'')",
                        Field::Scale => "generation_scale",
                        Field::CfgRescale => "generation_cfg_rescale",
                        Field::NoiseSchedule => "NULLIF(generation_noise_schedule,'')",
                        Field::Sampler => "NULLIF(generation_sampler,'')",
                        Field::Dimensions => "image_width || 'x' || image_height",
                        _ => unreachable!(),
                    };
                    compare(expression, op)
                }
            };
            predicate.push_str(" AND (");
            predicate.push_str(&term);
            predicate.push(')');
        }
        self.query_compare_section(&predicate, &params, offset, limit, start)
    }
}
