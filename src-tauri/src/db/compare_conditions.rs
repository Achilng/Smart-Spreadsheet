use std::collections::HashMap;

use rusqlite::{Connection, functions::FunctionFlags, params};
use serde::Deserialize;

use super::{Database, DatabaseError, prompt_matching};
use crate::pipeline::{extract_artist_tags, without_artist_blocks};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareConditions {
    observe: Field,
    dedupe: bool,
    allowed: Vec<Field>,
    model_keys: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Field {
    Artists,
    Positive,
    Character,
    Negative,
    Vibe,
    Model,
    Sampler,
    Steps,
    Seed,
    Scale,
    CfgRescale,
    NoiseSchedule,
    Dimensions,
}

impl Field {
    const ALL: [Self; 13] = [
        Self::Artists,
        Self::Positive,
        Self::Character,
        Self::Negative,
        Self::Vibe,
        Self::Model,
        Self::Sampler,
        Self::Steps,
        Self::Seed,
        Self::Scale,
        Self::CfgRescale,
        Self::NoiseSchedule,
        Self::Dimensions,
    ];

    fn expression(self, options: &CompareConditions) -> String {
        let artists_can_change =
            options.observe == Self::Artists || options.allowed.contains(&Self::Artists);
        let sql = match self {
            // Older rows use the whole positive prompt as a fallback artist string.
            // A derived mirror is not a separate artist constraint.
            Self::Artists if options.observe != Self::Artists => {
                "CASE WHEN artist_llm IS NULL AND trim(artists)=trim(positive_prompt)
                 AND instr(lower(artists),'artist:')=0 AND instr(lower(positive_prompt),'<artist')=0
                 THEN '' ELSE coalesce(artist_match_key,'') END"
            }
            Self::Artists => "coalesce(artist_match_key,'')",
            Self::Positive | Self::Character | Self::Negative => {
                let col = match self {
                    Self::Positive => "positive_prompt",
                    Self::Character => "character_prompt",
                    _ => "negative_prompt",
                };
                let expr = if artists_can_change
                    && self != options.observe
                    && self != Self::Negative
                {
                    format!(
                        "compare_without_artists({col}, CASE WHEN artist_llm IS NOT NULL OR trim(artists) IS NOT trim(positive_prompt) THEN artists END)"
                    )
                } else {
                    col.to_owned()
                };
                return format!(
                    "CASE WHEN {col} IS NOT NULL OR metadata_failed=0 THEN compare_prompt_key({expr}) END"
                );
            }
            Self::Vibe => {
                "CASE WHEN vibe_signature IS NULL THEN 'none' ELSE NULLIF(vibe_signature,'') END"
            }
            Self::Model => {
                "coalesce((SELECT value FROM model_keys WHERE key=rows.generation_model), nullif(rows.generation_model,''))"
            }
            Self::Sampler => "nullif(generation_sampler,'')",
            Self::Steps => "generation_steps",
            Self::Seed => "nullif(generation_seed,'')",
            Self::Scale => "generation_scale",
            Self::CfgRescale => "generation_cfg_rescale",
            Self::NoiseSchedule => "nullif(generation_noise_schedule,'')",
            Self::Dimensions => "image_width || 'x' || image_height",
        };
        sql.to_owned()
    }
}

pub(super) fn register(connection: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    connection.create_scalar_function("compare_prompt_key", 1, flags, |ctx| {
        let source = ctx.get::<Option<String>>(0)?.unwrap_or_default();
        let mut words: Vec<_> = source
            .split([',', '\n', '\r'])
            .map(|s| {
                s.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase()
            })
            .filter(|s| !s.is_empty())
            .collect();
        words.sort_unstable();
        Ok(words.join(","))
    })?;
    connection.create_scalar_function("compare_without_artists", 2, flags, |ctx| {
        let prompt = ctx.get::<Option<String>>(0)?.unwrap_or_default();
        let known = ctx.get::<Option<String>>(1)?.unwrap_or_default();
        let text = without_artist_blocks(&prompt);
        let mut names = prompt_matching::tag_names(&known);
        for tag in extract_artist_tags(&text) {
            names.extend(
                prompt_matching::tag_names(&tag)
                    .into_iter()
                    .filter(|name| name.starts_with("artist:")),
            );
        }
        // Remove both spellings of confirmed artist names; keep weights on other tags.
        let bare: Vec<_> = names
            .iter()
            .filter_map(|name| {
                name.strip_prefix("artist:")
                    .map(str::trim)
                    .map(str::to_owned)
            })
            .collect();
        names.extend(bare);
        Ok(prompt_matching::artist_key(&text, &names).unwrap_or_default())
    })
}

impl Database {
    /// Order groups and their members newest first, across the entire candidate set.
    /// Only IDs cross IPC; the UI loads image metadata one visible page at a time.
    pub fn query_compare_custom(
        &self,
        row_id: i64,
        options: &CompareConditions,
    ) -> Result<Vec<Vec<i64>>, DatabaseError> {
        let mut keys = vec![format!(
            "{} AS observed",
            options.observe.expression(options)
        )];
        let mut predicates = vec!["b.observed != a.observed".to_owned()];
        if options.dedupe {
            for (i, field) in Field::ALL.into_iter().enumerate() {
                if field == options.observe || options.allowed.contains(&field) {
                    continue;
                }
                keys.push(format!("{} AS k{i}", field.expression(options)));
                // Two absent optional parameters match; a missing and a recorded one do not.
                predicates.push(format!("b.k{i} IS a.k{i}"));
                if field == Field::Vibe {
                    predicates.push(format!("a.k{i} IS NOT NULL"));
                }
            }
        }
        let columns = keys.join(", ");
        let predicate = predicates.join(" AND ");
        let base = format!(
            "WITH model_keys AS MATERIALIZED (SELECT key,value FROM json_each(?2)),
            a AS MATERIALIZED (SELECT {columns} FROM rows WHERE id=?1),
            b AS (SELECT id,{columns} FROM rows WHERE id!=?1)
            SELECT b.id,b.observed FROM a CROSS JOIN b WHERE {predicate}"
        );
        let models = serde_json::json!(options.model_keys).to_string();
        if !options.dedupe {
            let mut stmt = self
                .connection
                .prepare(&format!("{base} ORDER BY b.id DESC"))?;
            return Ok(stmt
                .query_map(params![row_id, models], |row| Ok(vec![row.get(0)?]))?
                .collect::<rusqlite::Result<_>>()?);
        }
        let sql = format!(
            "SELECT json_group_array(id ORDER BY id DESC) FROM ({base}) GROUP BY observed ORDER BY max(id) DESC"
        );
        let mut stmt = self.connection.prepare(&sql)?;
        let groups = stmt
            .query_map(params![row_id, models], |row| {
                let json: String = row.get(0)?;
                serde_json::from_str(&json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(groups)
    }
}
