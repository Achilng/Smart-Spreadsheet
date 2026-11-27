//! Read-only helpers the agent uses to inspect the library before proposing changes.
use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::{Value, json};

use super::changes::analyze_matches;
use super::model_badge::badge_label;
use super::target::{MainView, Target, local_time, matches_all, prepare_sets};
use super::{Res, s};
use crate::automation::{matching::build_regex, text::tag_core};
use crate::db::{Database, automation_rules::execution::load_rule_rows};
use crate::pipeline::prompt_text::normalize_prompt_token;

pub(crate) const DEFAULT_PROMPT_STATS_LIMIT: usize = 20;
pub(crate) const MAX_PROMPT_STATS_LIMIT: usize = 50;
pub(crate) const ROW_FIELDS: &[&str] = &[
    "positive", "character", "negative", "artists", "note", "group", "tags", "favorite", "file",
    "model", "modelBadge", "sampler", "steps", "seed", "scale", "cfgRescale", "noiseSchedule",
    "width", "height", "time", "vibeReferenceCount", "vibes", "metadataFailed", "artistRepresentative",
    "groupId", "batchId", "importedAt", "updatedAt", "fileModifiedAt", "sourceOrdinal", "sourceType",
    "sourcePath", "sourceSize", "imageFolder", "imagePath", "storedImagePath",
];
/// Fields left out of `find_rows` unless asked for: long paths and rarely needed timestamps.
const QUIET_FIELDS: &[&str] = &["imageFolder", "imagePath", "storedImagePath", "sourcePath", "fileModifiedAt"];
pub(crate) const COUNT_FIELDS: &[&str] = &[
    "model", "modelBadge", "sampler", "noiseSchedule", "steps", "scale", "cfgRescale", "width", "height",
    "size", "orientation", "vibeCount", "favorite", "group", "tag", "batch", "importDate", "updatedDate",
    "artistString", "folder", "sourceType", "metadataFailed", "hasNote", "hasCharacterPrompt",
];
const PROMPT_LISTS: &[&str] = &["positive", "character", "negative", "artists"];

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PromptStatsSort {
    #[default]
    Count,
    Enrichment,
}

/// Order of `find_rows` pages. Import order follows image IDs.
#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RowSort {
    #[default]
    Oldest,
    Newest,
    RecentlyUpdated,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CountSort {
    #[default]
    Count,
    Value,
}

#[derive(Default)]
struct PromptTagCount {
    last_row_id: Option<i64>,
    in_scope: usize,
    in_library: usize,
    samples: Vec<i64>,
}

#[derive(Default)]
struct PromptStats {
    lists: HashMap<&'static str, HashMap<String, PromptTagCount>>,
    scope_rows: usize,
    library_rows: usize,
}

struct PromptRow<'a> {
    id: i64,
    positive: Option<&'a str>,
    character: Option<&'a str>,
    negative: Option<&'a str>,
    artists: Option<&'a str>,
    llm_artists: bool,
}

impl PromptStats {
    fn new(lists: &[&'static str]) -> Self {
        Self { lists: lists.iter().map(|name| (*name, HashMap::new())).collect(), ..Self::default() }
    }

    fn add_row(&mut self, row: PromptRow<'_>, in_scope: bool) {
        self.library_rows += 1;
        self.scope_rows += usize::from(in_scope);
        for (name, counts) in &mut self.lists {
            let (text, is_artist) = match *name {
                "positive" => (row.positive, false),
                "character" => (row.character, false),
                "negative" => (row.negative, false),
                _ => (row.artists, true),
            };
            let is_prompt_fallback = is_artist && !row.llm_artists && text.map(str::trim) == row.positive.map(str::trim);
            for part in text.unwrap_or_default().split([',', '，', '\n', '\r']) {
                let core = tag_core(part);
                // Artist XML markers decorate prompt text; they are not TAGs themselves.
                let core = core.trim_start_matches("<artist>").trim_end_matches("</artist>");
                let key = normalize_prompt_token(core).replace('_', " ");
                let key = if is_artist {
                    match key.strip_prefix("artist:") {
                        Some(name) => name,
                        None if is_prompt_fallback => continue,
                        None => key.as_str(),
                    }
                } else {
                    key.as_str()
                };
                let key = key.split_whitespace().collect::<Vec<_>>().join(" ");
                if key.is_empty() {
                    continue;
                }
                let count = counts.entry(key).or_default();
                if count.last_row_id == Some(row.id) {
                    continue;
                }
                count.last_row_id = Some(row.id);
                count.in_library += 1;
                if in_scope {
                    count.in_scope += 1;
                    if count.samples.len() < 3 {
                        count.samples.push(row.id);
                    }
                }
            }
        }
    }
}

fn ranked_prompt_tags(
    counts: HashMap<String, PromptTagCount>,
    scope_rows: usize,
    library_rows: Option<usize>,
    limit: usize,
    sort_by: PromptStatsSort,
) -> Value {
    let mut items = counts.into_iter().filter(|(_, count)| count.in_scope > 0).collect::<Vec<_>>();
    let compare = |a: &(String, PromptTagCount), b: &(String, PromptTagCount)| {
        let by_enrichment = match sort_by {
            PromptStatsSort::Count => std::cmp::Ordering::Equal,
            PromptStatsSort::Enrichment => {
                // The denominators are shared. Compare the two ratios exactly.
                (b.1.in_scope as u128 * a.1.in_library as u128)
                    .cmp(&(a.1.in_scope as u128 * b.1.in_library as u128))
            }
        };
        by_enrichment.then(b.1.in_scope.cmp(&a.1.in_scope)).then(a.0.cmp(&b.0))
    };
    let distinct_tags = items.len();
    if items.len() > limit {
        items.select_nth_unstable_by(limit, compare);
        items.truncate(limit);
    }
    items.sort_unstable_by(compare);
    let items = items.into_iter().map(|(tag, count)| {
        let scope_share = count.in_scope as f64 / scope_rows as f64;
        let mut item = json!({
            "tag": tag, "imagesInScope": count.in_scope, "scopeShare": scope_share,
            "sampleRowIds": count.samples,
        });
        if let Some(total) = library_rows {
            let library_share = count.in_library as f64 / total as f64;
            item["imagesInLibrary"] = json!(count.in_library);
            item["libraryShare"] = json!(library_share);
            item["enrichment"] = json!(scope_share / library_share);
        }
        item
    }).collect::<Vec<_>>();
    json!({"distinctTags": distinct_tags, "items": items})
}

fn clip(text: Option<&String>, max: usize) -> Value {
    match text {
        None => Value::Null,
        Some(text) if text.chars().count() <= max => json!(text),
        Some(text) => {
            let head = text.chars().take(max).collect::<String>();
            json!(format!("{head}…（共 {} 字）", text.chars().count()))
        }
    }
}

/// File modification time is stored in nanoseconds since the Unix epoch.
fn local_from_nanos(nanos: Option<i64>) -> Option<String> {
    let nanos = nanos?;
    let time = chrono::DateTime::from_timestamp(nanos.div_euclid(1_000_000_000), nanos.rem_euclid(1_000_000_000) as u32)?;
    Some(time.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
}

fn local_day(text: Option<&str>) -> Option<String> {
    local_time(text).map(|time| time.chars().take(10).collect())
}

fn ids_json(ids: &[i64]) -> Res<String> {
    serde_json::to_string(ids).map_err(s)
}

/// Order values for `sortBy=value`: numbers by size, text alphabetically, missing values last.
fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Greater,
        (_, Value::Null) => Ordering::Less,
        (Value::Number(x), Value::Number(y)) => x.as_f64().unwrap_or(0.0).total_cmp(&y.as_f64().unwrap_or(0.0)),
        (Value::Array(x), Value::Array(y)) => x.iter().zip(y).map(|(x, y)| compare_values(x, y)).find(|o| o.is_ne()).unwrap_or(Ordering::Equal),
        _ => a.to_string().cmp(&b.to_string()),
    }
}

impl Database {
    pub fn agent_overview(&self, offset: usize, limit: usize) -> Res<Value> {
        let (total, favorites, without_artist, artist_groups, without_metadata, ai_artists): (i64, i64, i64, i64, i64, i64) = self
            .connection
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(favorite), 0), COALESCE(SUM(artist_match_key IS NULL), 0),
                        COUNT(DISTINCT artist_match_key), COALESCE(SUM(metadata_failed), 0),
                        COALESCE(SUM(artist_llm IS NOT NULL), 0) FROM rows",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .map_err(s)?;
        let (batches, latest_import): (i64, Option<String>) = self
            .connection
            .query_row("SELECT COUNT(*), MAX(imported_at) FROM import_batches", [], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(s)?;
        let rules = self.list_automation_rules().map_err(s)?;
        let groups = self.list_groups().map_err(s)?;
        let mut tags = self.list_tags().map_err(s)?;
        tags.sort_by(|a, b| b.row_count.cmp(&a.row_count).then(a.name.cmp(&b.name)));
        Ok(json!({
            "totalImages": total,
            "favorites": favorites,
            "artistStringGroups": artist_groups,
            "imagesWithoutArtistString": without_artist,
            "imagesWithAiExtractedArtists": ai_artists,
            "imagesWithoutMetadata": without_metadata,
            "importBatches": batches,
            "latestImportAt": local_time(latest_import.as_deref()),
            "automationRules": {"total": rules.len(), "enabled": rules.iter().filter(|rule| rule.enabled).count()},
            "groupCount": groups.len(),
            "emptyGroups": groups.iter().filter(|group| group.member_count == 0).count(),
            "offset": offset,
            "limit": limit,
            "groups": groups.iter().skip(offset).take(limit).map(|g| json!({"id": g.id, "name": g.name, "images": g.member_count})).collect::<Vec<_>>(),
            "tagCount": tags.len(),
            "tags": tags.iter().skip(offset).take(limit).map(|t| json!({"name": t.name, "images": t.row_count})).collect::<Vec<_>>(),
        }))
    }

    /// IDs in a target, without loading full rows unless conditions need them.
    pub(super) fn agent_scope_ids(&mut self, target: &Target, view: &MainView) -> Res<(Vec<i64>, Vec<String>)> {
        if target.has_row_filters() {
            let resolved = self.agent_resolve(target, view)?;
            Ok((resolved.rows.iter().map(|row| row.id).collect(), resolved.notes))
        } else {
            let index = self.agent_row_index()?;
            let (ids, notes) = self.agent_target_ids(target, view, &index)?;
            let mut ids = ids.into_iter().collect::<Vec<_>>();
            ids.sort_unstable();
            Ok((ids, notes))
        }
    }

    fn sort_ids(&self, mut ids: Vec<i64>, sort: RowSort) -> Res<Vec<i64>> {
        match sort {
            RowSort::Oldest => ids.sort_unstable(),
            RowSort::Newest => ids.sort_unstable_by(|a, b| b.cmp(a)),
            RowSort::RecentlyUpdated => {
                let mut statement = self
                    .connection
                    .prepare("SELECT id FROM rows WHERE id IN (SELECT value FROM json_each(?1)) ORDER BY updated_at DESC, id DESC")
                    .map_err(s)?;
                ids = statement
                    .query_map([ids_json(&ids)?], |row| row.get(0))
                    .map_err(s)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(s)?;
            }
        }
        Ok(ids)
    }

    pub(crate) fn agent_find_rows(
        &mut self,
        target: &Target,
        view: &MainView,
        fields: &[String],
        limit: usize,
        offset: usize,
        sort: RowSort,
    ) -> Res<Value> {
        for field in fields {
            if !ROW_FIELDS.contains(&field.as_str()) {
                return Err(format!("未知图片字段：{field}。可用字段：{}", ROW_FIELDS.join(", ")));
            }
        }
        let limit = limit.clamp(1, 50);
        let (ids, notes) = self.agent_scope_ids(target, view)?;
        let ids = self.sort_ids(ids, sort)?;
        let total = ids.len();
        let start = offset.min(total);
        let page_ids = &ids[start..start.saturating_add(limit).min(total)];
        let sources = load_rule_rows(&self.connection, Some(page_ids))
            .map_err(s)?
            .into_iter()
            .map(|row| (row.id, row))
            .collect::<HashMap<_, _>>();
        let times = {
            let mut statement = self.connection.prepare(
                "SELECT rows.id, rows.updated_at, import_batches.imported_at, rows.source_mtime
                 FROM rows JOIN import_batches ON import_batches.id = rows.batch_id
                 WHERE rows.id IN (SELECT value FROM json_each(?1))",
            ).map_err(s)?;
            statement
                .query_map([ids_json(page_ids)?], |row| {
                    Ok((row.get::<_, i64>(0)?, (row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, Option<i64>>(3)?)))
                })
                .map_err(s)?
                .collect::<Result<HashMap<_, _>, _>>()
                .map_err(s)?
        };
        let records = self.get_rows_by_ids(page_ids).map_err(s)?;
        let mut rows = Vec::with_capacity(records.len());
        for row in &records {
            let (Some(source), Some((updated, imported, modified))) = (sources.get(&row.id), times.get(&row.id)) else { continue };
            let mut value = json!({"id": row.id});
            for &field in ROW_FIELDS {
                let requested = if fields.is_empty() {
                    !QUIET_FIELDS.contains(&field)
                } else {
                    fields.iter().any(|name| name == field)
                };
                if !requested { continue; }
                value[field] = match field {
                    "positive" => clip(row.positive_prompt.as_ref(), 600),
                    "character" => clip(row.character_prompt.as_ref(), 400),
                    "negative" => clip(row.negative_prompt.as_ref(), 300),
                    "artists" => clip(row.artists.as_ref(), 200),
                    "note" => clip(row.note.as_ref(), 200),
                    "group" => json!(row.group_name),
                    "groupId" => json!(row.group_id),
                    "tags" => json!(row.tags),
                    "favorite" => json!(row.favorite),
                    "file" => json!(row.image_path.as_deref().and_then(|path| path.rsplit(['/', '\\']).next())),
                    "model" => json!(row.generation_model),
                    "modelBadge" => json!(badge_label(row.generation_model.as_deref())),
                    "sampler" => json!(row.generation_sampler),
                    "steps" => json!(row.generation_steps),
                    "seed" => json!(row.generation_seed),
                    "scale" => json!(source.generation_scale),
                    "cfgRescale" => json!(source.generation_cfg_rescale),
                    "noiseSchedule" => json!(row.generation_noise_schedule),
                    "width" => json!(row.image_width),
                    "height" => json!(row.image_height),
                    "time" => json!(row.time),
                    "vibeReferenceCount" => json!(row.vibe_reference_count),
                    "vibes" => super::vibes::vibes_json(row.vibe_details.as_deref(), row.vibe_reference_count),
                    "metadataFailed" => json!(row.metadata_failed),
                    "artistRepresentative" => json!(row.artist_representative),
                    "batchId" => json!(row.batch_id),
                    "importedAt" => json!(local_time(imported.as_deref())),
                    "updatedAt" => json!(local_time(updated.as_deref())),
                    "fileModifiedAt" => json!(local_from_nanos(*modified)),
                    "sourceOrdinal" => json!(row.source_ordinal),
                    "sourceType" => json!(source.source_type),
                    "sourcePath" => json!(source.source_path),
                    "sourceSize" => json!(source.source_size),
                    "imageFolder" => json!(row.image_folder),
                    "imagePath" => json!(row.image_path),
                    "storedImagePath" => json!(row.stored_image_path),
                    _ => unreachable!(),
                };
            }
            rows.push(value);
        }
        Ok(json!({
            "total": total,
            "offset": offset,
            "returned": rows.len(),
            "notes": notes,
            "rows": rows,
        }))
    }

    pub fn agent_generation_values(&mut self, target: &Target, view: &MainView, field: &str, offset: usize) -> Res<Value> {
        let column = match field {
            "model" | "modelBadge" => "generation_model",
            "sampler" => "generation_sampler",
            "noiseSchedule" => "generation_noise_schedule",
            _ => return Err(format!("不支持统计生成字段：{field}")),
        };
        // Aggregate only the requested column. A whole-library inventory needs no row IDs or prompts.
        let (ids, notes) = if !target.has_row_filters() && target.whole_library_scope() {
            (None, Vec::new())
        } else {
            let (ids, notes) = self.agent_scope_ids(target, view)?;
            (Some(ids_json(&ids)?), notes)
        };
        let predicate = if ids.is_some() { "WHERE id IN (SELECT value FROM json_each(?1))" } else { "" };
        let sql = format!("SELECT {column}, COUNT(*) FROM rows {predicate} GROUP BY {column}");
        let mut statement = self.connection.prepare(&sql).map_err(s)?;
        let mut groups = statement.query(rusqlite::params_from_iter(ids.iter())).map_err(s)?;
        let mut counts = HashMap::new();
        let mut missing = 0;
        let mut scanned = 0;
        while let Some(group) = groups.next().map_err(s)? {
            let raw: Option<String> = group.get(0).map_err(s)?;
            let images: i64 = group.get(1).map_err(s)?;
            scanned += images;
            let value = if field == "modelBadge" { badge_label(raw.as_deref()).map(str::to_owned) } else { raw };
            if let Some(value) = value.filter(|text| !text.trim().is_empty()) {
                *counts.entry(value).or_insert(0i64) += images;
            } else {
                missing += images;
            }
        }
        let mut counts: Vec<_> = counts.into_iter().collect();
        counts.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let total = counts.len();
        let items: Vec<_> = counts.into_iter().skip(offset).take(50)
            .map(|(value, count)| json!({"value":value,"images":count})).collect();
        Ok(json!({"scannedRows":scanned,"notes":notes,"missing":missing,
            "distinctValues":total,"offset":offset,"items":items}))
    }

    /// How many images fall under each value of one or two fields (a cross table for two).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn agent_count_rows(
        &mut self,
        target: &Target,
        view: &MainView,
        by: &[String],
        limit: usize,
        offset: usize,
        sort: CountSort,
    ) -> Res<Value> {
        if by.is_empty() || by.len() > 2 {
            return Err("by 需要 1~2 个字段".into());
        }
        for field in by {
            if !COUNT_FIELDS.contains(&field.as_str()) {
                return Err(format!("不支持按「{field}」统计。可用字段：{}", COUNT_FIELDS.join(", ")));
            }
        }
        let whole = !target.has_row_filters() && target.whole_library_scope();
        let (ids, notes) = if whole { (None, Vec::new()) } else {
            let (ids, notes) = self.agent_scope_ids(target, view)?;
            (Some(ids), notes)
        };
        let predicate = if ids.is_some() { "WHERE rows.id IN (SELECT value FROM json_each(?1))" } else { "" };
        let id_param = ids.as_deref().map(ids_json).transpose()?;
        let needs = |name: &str| by.iter().any(|field| field == name);
        let tags: HashMap<i64, Vec<String>> = if needs("tag") {
            let sql = format!("SELECT row_tags.row_id, tags.name FROM row_tags JOIN tags ON tags.id = row_tags.tag_id
                JOIN rows ON rows.id = row_tags.row_id {predicate}");
            let mut statement = self.connection.prepare(&sql).map_err(s)?;
            let mut map: HashMap<i64, Vec<String>> = HashMap::new();
            let mut rows = statement.query(rusqlite::params_from_iter(id_param.iter())).map_err(s)?;
            while let Some(row) = rows.next().map_err(s)? {
                map.entry(row.get(0).map_err(s)?).or_default().push(row.get(1).map_err(s)?);
            }
            map
        } else {
            HashMap::new()
        };
        let sql = format!(
            "SELECT rows.id, rows.generation_model, rows.generation_sampler, rows.generation_noise_schedule,
                    rows.generation_steps, rows.generation_scale, rows.generation_cfg_rescale, rows.image_width,
                    rows.image_height, COALESCE(rows.vibe_reference_count, 0), rows.favorite, groups.name,
                    rows.batch_id, import_batches.imported_at, rows.updated_at, rows.artist_match_key, rows.artists,
                    rows.image_folder, import_batches.source_type, rows.metadata_failed,
                    COALESCE(TRIM(rows.note), '') != '', COALESCE(TRIM(rows.character_prompt), '') != ''
             FROM rows JOIN import_batches ON import_batches.id = rows.batch_id
             LEFT JOIN groups ON groups.id = rows.group_id {predicate}"
        );
        let mut statement = self.connection.prepare(&sql).map_err(s)?;
        let mut cursor = statement.query(rusqlite::params_from_iter(id_param.iter())).map_err(s)?;
        let mut counts: HashMap<String, (Vec<Value>, usize, Vec<i64>)> = HashMap::new();
        let mut artist_labels: HashMap<String, String> = HashMap::new();
        let mut batches_seen = HashSet::new();
        let mut scanned = 0usize;
        while let Some(row) = cursor.next().map_err(s)? {
            scanned += 1;
            let id: i64 = row.get(0).map_err(s)?;
            let text = |index: usize| -> Res<Value> {
                Ok(row.get::<_, Option<String>>(index).map_err(s)?.filter(|v| !v.trim().is_empty()).map_or(Value::Null, Value::String))
            };
            let number = |index: usize| -> Res<Value> {
                Ok(row.get::<_, Option<f64>>(index).map_err(s)?.map_or(Value::Null, |v| json!(v)))
            };
            let mut values_per_field: Vec<Vec<Value>> = Vec::new();
            for field in by {
                let values = match field.as_str() {
                    "model" => vec![text(1)?],
                    "modelBadge" => vec![json!(badge_label(row.get::<_, Option<String>>(1).map_err(s)?.as_deref()))],
                    "sampler" => vec![text(2)?],
                    "noiseSchedule" => vec![text(3)?],
                    "steps" => vec![json!(row.get::<_, Option<i64>>(4).map_err(s)?)],
                    "scale" => vec![number(5)?],
                    "cfgRescale" => vec![number(6)?],
                    "width" => vec![json!(row.get::<_, Option<i64>>(7).map_err(s)?)],
                    "height" => vec![json!(row.get::<_, Option<i64>>(8).map_err(s)?)],
                    "size" | "orientation" => {
                        let (w, h) = (row.get::<_, Option<i64>>(7).map_err(s)?, row.get::<_, Option<i64>>(8).map_err(s)?);
                        vec![match (w, h) {
                            (Some(w), Some(h)) if field == "size" => json!(format!("{w}x{h}")),
                            (Some(w), Some(h)) => json!(if w > h { "横图" } else if w < h { "竖图" } else { "方图" }),
                            _ => Value::Null,
                        }]
                    }
                    "vibeCount" => vec![json!(row.get::<_, i64>(9).map_err(s)?)],
                    "favorite" => vec![json!(row.get::<_, bool>(10).map_err(s)?)],
                    "group" => vec![text(11)?],
                    "tag" => match tags.get(&id) {
                        Some(names) => names.iter().map(|name| json!(name)).collect(),
                        None => vec![Value::Null],
                    },
                    "batch" => {
                        let batch: i64 = row.get(12).map_err(s)?;
                        batches_seen.insert(batch);
                        vec![json!(batch)]
                    }
                    "importDate" => vec![json!(local_day(row.get::<_, Option<String>>(13).map_err(s)?.as_deref()))],
                    "updatedDate" => vec![json!(local_day(row.get::<_, Option<String>>(14).map_err(s)?.as_deref()))],
                    "artistString" => {
                        let key: Option<String> = row.get(15).map_err(s)?;
                        match key {
                            Some(key) => {
                                let label = artist_labels.entry(key).or_insert_with(|| row.get::<_, Option<String>>(16).ok().flatten().unwrap_or_default());
                                vec![json!(label.clone())]
                            }
                            None => vec![Value::Null],
                        }
                    }
                    "folder" => vec![text(17)?],
                    "sourceType" => vec![text(18)?],
                    "metadataFailed" => vec![json!(row.get::<_, bool>(19).map_err(s)?)],
                    "hasNote" => vec![json!(row.get::<_, bool>(20).map_err(s)?)],
                    "hasCharacterPrompt" => vec![json!(row.get::<_, bool>(21).map_err(s)?)],
                    _ => unreachable!(),
                };
                values_per_field.push(values);
            }
            let combos: Vec<Vec<Value>> = match values_per_field.as_slice() {
                [a] => a.iter().map(|v| vec![v.clone()]).collect(),
                [a, b] => a.iter().flat_map(|x| b.iter().map(move |y| vec![x.clone(), y.clone()])).collect(),
                _ => Vec::new(),
            };
            for combo in combos {
                let entry = counts.entry(Value::Array(combo.clone()).to_string()).or_insert_with(|| (combo, 0, Vec::new()));
                entry.1 += 1;
                if entry.2.len() < 3 {
                    entry.2.push(id);
                }
            }
        }
        drop(cursor);
        drop(statement);
        let mut items = counts.into_values().collect::<Vec<_>>();
        match sort {
            CountSort::Count => items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| compare_values(&json!(a.0), &json!(b.0)))),
            CountSort::Value => items.sort_by(|a, b| compare_values(&json!(a.0), &json!(b.0))),
        }
        let distinct = items.len();
        let limit = limit.clamp(1, 100);
        let page = items.into_iter().skip(offset).take(limit).map(|(values, images, samples)| {
            let share = if scanned == 0 { 0.0 } else { images as f64 / scanned as f64 };
            let mut item = if values.len() == 1 { json!({"value": values[0]}) } else { json!({"values": values}) };
            item["images"] = json!(images);
            item["share"] = json!((share * 10000.0).round() / 10000.0);
            item["sampleRowIds"] = json!(samples);
            item
        }).collect::<Vec<_>>();
        let mut result = json!({
            "by": by, "scannedRows": scanned, "notes": notes, "distinctValues": distinct,
            "offset": offset, "nextOffset": (offset + page.len() < distinct).then_some(offset + page.len()), "items": page,
        });
        if needs("tag") {
            result["tagNote"] = json!("一张图有多个 TAG 时会在每个 TAG 下各算一次；value 为 null 表示没有 TAG");
        }
        if needs("artistString") {
            let mut statement = self.connection
                .prepare("SELECT key, alias FROM dedupe_aliases WHERE mode = 'artists' AND alias != ''")
                .map_err(s)?;
            let aliases = statement
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(s)?
                .collect::<Result<HashMap<_, _>, _>>()
                .map_err(s)?;
            let labelled = artist_labels.iter()
                .filter_map(|(key, label)| aliases.get(key).map(|alias| (label.clone(), json!(alias))))
                .collect::<serde_json::Map<_, _>>();
            if !labelled.is_empty() {
                result["artistAliases"] = Value::Object(labelled);
            }
        }
        if needs("batch") {
            let batches = self.list_batches().map_err(s)?.into_iter()
                .filter(|batch| batches_seen.contains(&batch.id))
                .map(|batch| (batch.id.to_string(), json!({"sourcePath": batch.source_path, "importedAt": local_time(Some(&batch.imported_at))})))
                .collect::<serde_json::Map<_, _>>();
            result["batches"] = Value::Object(batches);
        }
        Ok(result)
    }

    pub fn agent_analyze_text(
        &mut self,
        target: &Target,
        view: &MainView,
        pattern: &str,
        is_regex: bool,
        case_sensitive: bool,
        fields: &[String],
    ) -> Res<Value> {
        let source = if is_regex { pattern.to_owned() } else { regex::escape(pattern) };
        let regex = build_regex(&source, case_sensitive).map_err(s)?;
        let resolved = self.agent_resolve(target, view)?;
        let fields = if fields.is_empty() {
            vec!["positive".to_owned(), "character".to_owned(), "negative".to_owned()]
        } else {
            fields.to_vec()
        };
        let results = fields
            .iter()
            .map(|field| analyze_matches(&regex, &resolved.rows, field))
            .collect::<Vec<_>>();
        Ok(json!({
            "scannedRows": resolved.rows.len(),
            "notes": resolved.notes,
            "fields": results,
        }))
    }

    pub(crate) fn agent_summarize_rows(
        &mut self,
        target: &Target,
        view: &MainView,
        limit: usize,
        compare_library: bool,
        sort_by: PromptStatsSort,
        lists: &[String],
    ) -> Res<Value> {
        let lists = if lists.is_empty() {
            vec!["positive", "character", "artists"]
        } else {
            let mut chosen = Vec::new();
            for name in lists {
                let known = PROMPT_LISTS.iter().find(|item| **item == name.as_str())
                    .ok_or_else(|| format!("未知榜单：{name}。可用：{}", PROMPT_LISTS.join(", ")))?;
                if !chosen.contains(known) {
                    chosen.push(*known);
                }
            }
            chosen
        };
        let compare_library = compare_library || matches!(sort_by, PromptStatsSort::Enrichment);
        let sets = self.agent_filter_sets(target)?;
        let prepared = prepare_sets(&sets)?;
        let unrestricted = target.whole_library_scope();
        let (ids, notes, index) = if unrestricted {
            (None, Vec::new(), None)
        } else {
            let index = self.agent_row_index()?;
            let (ids, notes) = self.agent_target_ids(target, view, &index)?;
            (Some(ids), notes, Some(index))
        };
        let includes = |id| ids.as_ref().is_none_or(|ids| ids.contains(&id));
        let mut stats = PromptStats::new(&lists);
        if prepared.is_empty() && (compare_library || unrestricted) {
            // No condition evaluation: stream just the necessary columns, not full RuleRows.
            let mut statement = self.connection.prepare(
                "SELECT id, positive_prompt, character_prompt, negative_prompt, artists, artist_llm IS NOT NULL
                 FROM rows ORDER BY id",
            ).map_err(s)?;
            let mut rows = statement.query([]).map_err(s)?;
            while let Some(row) = rows.next().map_err(s)? {
                let id = row.get(0).map_err(s)?;
                stats.add_row(PromptRow {
                    id,
                    positive: row.get_ref(1).map_err(s)?.as_str_or_null().map_err(s)?,
                    character: row.get_ref(2).map_err(s)?.as_str_or_null().map_err(s)?,
                    negative: row.get_ref(3).map_err(s)?.as_str_or_null().map_err(s)?,
                    artists: row.get_ref(4).map_err(s)?.as_str_or_null().map_err(s)?,
                    llm_artists: row.get(5).map_err(s)?,
                }, includes(id));
            }
        } else if compare_library || ids.as_ref().is_none_or(|ids| !ids.is_empty()) {
            // When conditions are used, resolve and aggregate in one pass over the same inputs.
            // For a library comparison, each scoped row also contributes to the baseline here.
            let candidates = if compare_library {
                None
            } else {
                ids.as_ref().map(|ids| ids.iter().copied().collect::<Vec<_>>())
            };
            let index = match index {
                Some(index) => index,
                None => self.agent_row_index()?,
            };
            for row in load_rule_rows(&self.connection, candidates.as_deref()).map_err(s)? {
                let in_scope = includes(row.id) && matches_all(&prepared, &row);
                if !compare_library && !in_scope {
                    continue;
                }
                stats.add_row(PromptRow {
                    id: row.id,
                    positive: row.positive_prompt.as_deref(),
                    character: row.character_prompt.as_deref(),
                    negative: row.negative_prompt.as_deref(),
                    artists: row.artists.as_deref(),
                    llm_artists: index.get(&row.id).is_some_and(|entry| entry.llm_artists.is_some()),
                }, in_scope);
            }
        }
        let limit = limit.clamp(1, MAX_PROMPT_STATS_LIMIT);
        let library_rows = compare_library.then_some(stats.library_rows);
        let scope_rows = stats.scope_rows;
        let mut result = json!({"scannedRows": scope_rows, "notes": notes});
        for (name, counts) in stats.lists {
            result[name] = ranked_prompt_tags(counts, scope_rows, library_rows, limit, sort_by);
        }
        if let Some(total) = library_rows {
            result["libraryRows"] = json!(total);
        }
        Ok(result)
    }

    pub fn agent_artist_strings(
        &mut self,
        target: &Target,
        view: &MainView,
        contains: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Res<Value> {
        let resolved = self.agent_resolve(target, view)?;
        let mut library_counts: HashMap<String, usize> = HashMap::new();
        for entry in resolved.index.values() {
            if let Some(key) = &entry.artist_key {
                *library_counts.entry(key.clone()).or_default() += 1;
            }
        }
        let aliases = {
            let mut statement = self
                .connection
                .prepare("SELECT key, alias FROM dedupe_aliases WHERE mode = 'artists' AND alias != ''")
                .map_err(s)?;
            statement
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(s)?
                .collect::<Result<HashMap<_, _>, _>>()
                .map_err(s)?
        };
        let representatives = {
            let mut statement = self.connection.prepare("SELECT artist_key, row_id FROM artist_representatives").map_err(s)?;
            statement
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
                .map_err(s)?
                .collect::<Result<HashMap<_, _>, _>>()
                .map_err(s)?
        };
        // `_` and a space are interchangeable in NovelAI prompts.
        let fold = |text: &str| text.to_lowercase().replace('_', " ");
        let needle = contains.map(fold).filter(|value| !value.trim().is_empty());
        let mut groups: HashMap<String, (usize, String, i64)> = HashMap::new();
        let mut without = 0;
        for row in &resolved.rows {
            let Some(key) = resolved.index.get(&row.id).and_then(|entry| entry.artist_key.clone()) else {
                without += 1;
                continue;
            };
            let entry = groups
                .entry(key)
                .or_insert_with(|| (0, row.artists.clone().unwrap_or_default(), row.id));
            entry.0 += 1;
        }
        let mut list = groups
            .into_iter()
            .filter(|(key, (_, text, _))| {
                needle.as_ref().is_none_or(|needle| {
                    fold(text).contains(needle)
                        || aliases.get(key).is_some_and(|alias| fold(alias).contains(needle))
                })
            })
            .collect::<Vec<_>>();
        list.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(&b.0)));
        let total = list.len();
        let items = list
            .into_iter()
            .skip(offset)
            .take(limit.clamp(1, 100))
            .map(|(key, (count, text, sample))| {
                json!({
                    "artistString": text,
                    "alias": aliases.get(&key),
                    "imagesInScope": count,
                    "imagesInLibrary": library_counts.get(&key).copied().unwrap_or(0),
                    "sampleRowId": sample,
                    "representativeRowId": representatives.get(&key),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "scannedRows": resolved.rows.len(),
            "rowsWithoutArtistString": without,
            "matchingGroups": total,
            "offset": offset,
            "groups": items,
            "notes": resolved.notes,
        }))
    }
}
