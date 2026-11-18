//! Read-only helpers the agent uses to inspect the library before proposing changes.
use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::changes::analyze_matches;
use super::target::{MainView, Scope, Target, target_conditions};
use super::{Res, s};
use crate::automation::{matching::{PreparedConditionSet, build_regex}, text::tag_core};
use crate::db::{Database, automation_rules::execution::load_rule_rows};
use crate::pipeline::prompt_text::normalize_prompt_token;

pub(crate) const DEFAULT_PROMPT_STATS_LIMIT: usize = 20;
pub(crate) const MAX_PROMPT_STATS_LIMIT: usize = 50;

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PromptStatsSort {
    #[default]
    Count,
    Enrichment,
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
    positive: HashMap<String, PromptTagCount>,
    character: HashMap<String, PromptTagCount>,
    artists: HashMap<String, PromptTagCount>,
    scope_rows: usize,
    library_rows: usize,
}

impl PromptStats {
    fn add_row(
        &mut self,
        id: i64,
        positive: Option<&str>,
        character: Option<&str>,
        artists: Option<&str>,
        llm_artists: bool,
        in_scope: bool,
    ) {
        self.library_rows += 1;
        self.scope_rows += usize::from(in_scope);
        for (counts, text, is_artist) in [
            (&mut self.positive, positive, false),
            (&mut self.character, character, false),
            (&mut self.artists, artists, true),
        ] {
            let is_prompt_fallback = is_artist && !llm_artists && text.map(str::trim) == positive.map(str::trim);
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
                if count.last_row_id == Some(id) {
                    continue;
                }
                count.last_row_id = Some(id);
                count.in_library += 1;
                if in_scope {
                    count.in_scope += 1;
                    if count.samples.len() < 3 {
                        count.samples.push(id);
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

impl Database {
    pub fn agent_overview(&self) -> Res<Value> {
        let (total, favorites, without_artist, artist_groups): (i64, i64, i64, i64) = self
            .connection
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(favorite), 0),
                        SUM(artist_match_key IS NULL), COUNT(DISTINCT artist_match_key) FROM rows",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, Option<i64>>(2)?.unwrap_or(0), row.get(3)?)),
            )
            .map_err(s)?;
        let groups = self.list_groups().map_err(s)?;
        let mut tags = self.list_tags().map_err(s)?;
        tags.sort_by(|a, b| b.row_count.cmp(&a.row_count).then(a.name.cmp(&b.name)));
        Ok(json!({
            "totalImages": total,
            "favorites": favorites,
            "artistStringGroups": artist_groups,
            "imagesWithoutArtistString": without_artist,
            "groupCount": groups.len(),
            "groups": groups.iter().take(60).map(|g| json!({"id": g.id, "name": g.name, "images": g.member_count})).collect::<Vec<_>>(),
            "tagCount": tags.len(),
            "tags": tags.iter().take(80).map(|t| json!({"name": t.name, "images": t.row_count})).collect::<Vec<_>>(),
        }))
    }

    pub fn agent_find_rows(
        &mut self,
        target: &Target,
        view: &MainView,
        fields: &[String],
        limit: usize,
        offset: usize,
    ) -> Res<Value> {
        let resolved = self.agent_resolve(target, view)?;
        let group_names = self
            .list_groups()
            .map_err(s)?
            .into_iter()
            .map(|group| (group.id, group.name))
            .collect::<HashMap<_, _>>();
        let want = |name: &str| fields.is_empty() || fields.iter().any(|field| field == name);
        let limit = limit.clamp(1, 50);
        let rows = resolved
            .rows
            .iter()
            .skip(offset)
            .take(limit)
            .map(|row| {
                let mut value = json!({"id": row.id});
                if want("positive") {
                    value["positive"] = clip(row.positive_prompt.as_ref(), 600);
                }
                if want("character") {
                    value["character"] = clip(row.character_prompt.as_ref(), 400);
                }
                if want("negative") {
                    value["negative"] = clip(row.negative_prompt.as_ref(), 300);
                }
                if want("artists") {
                    value["artists"] = clip(row.artists.as_ref(), 200);
                }
                if want("note") {
                    value["note"] = clip(row.note.as_ref(), 200);
                }
                if want("group") {
                    value["group"] = json!(row.group_id.and_then(|id| group_names.get(&id)));
                }
                if want("tags") {
                    let mut tags = row.tags.iter().collect::<Vec<_>>();
                    tags.sort();
                    value["tags"] = json!(tags);
                }
                if want("favorite") {
                    value["favorite"] = json!(resolved.favorite(row.id));
                }
                if want("file") {
                    value["file"] = json!(row.image_path.as_deref().map(|path| {
                        path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
                    }));
                }
                value
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "total": resolved.rows.len(),
            "offset": offset,
            "returned": rows.len(),
            "notes": resolved.notes,
            "rows": rows,
        }))
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
    ) -> Res<Value> {
        let compare_library = compare_library || matches!(sort_by, PromptStatsSort::Enrichment);
        let conditions = target_conditions(target)?;
        let prepared = conditions.as_ref().map(PreparedConditionSet::new).transpose().map_err(s)?;
        let unrestricted = target.scope == Scope::All
            && target.artist_strings.is_empty()
            && !target.expand_to_same_artist_string
            && target.favorite.is_none();
        let (ids, notes, index) = if unrestricted {
            (None, Vec::new(), None)
        } else {
            let index = self.agent_row_index()?;
            let (ids, notes) = self.agent_target_ids(target, view, &index)?;
            (Some(ids), notes, Some(index))
        };
        let includes = |id| ids.as_ref().is_none_or(|ids| ids.contains(&id));
        let mut stats = PromptStats::default();
        if prepared.is_none() && (compare_library || unrestricted) {
            // No condition evaluation: stream just the necessary columns, not full RuleRows.
            let mut statement = self.connection.prepare(
                "SELECT id, positive_prompt, character_prompt, artists, artist_llm IS NOT NULL
                 FROM rows ORDER BY id",
            ).map_err(s)?;
            let mut rows = statement.query([]).map_err(s)?;
            while let Some(row) = rows.next().map_err(s)? {
                let id = row.get(0).map_err(s)?;
                stats.add_row(
                    id,
                    row.get_ref(1).map_err(s)?.as_str_or_null().map_err(s)?,
                    row.get_ref(2).map_err(s)?.as_str_or_null().map_err(s)?,
                    row.get_ref(3).map_err(s)?.as_str_or_null().map_err(s)?,
                    row.get(4).map_err(s)?,
                    includes(id),
                );
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
                let in_scope = includes(row.id) && prepared.as_ref().is_none_or(|p| p.matches(&row));
                if !compare_library && !in_scope {
                    continue;
                }
                stats.add_row(
                    row.id,
                    row.positive_prompt.as_deref(),
                    row.character_prompt.as_deref(),
                    row.artists.as_deref(),
                    index.get(&row.id).is_some_and(|entry| entry.llm_artists.is_some()),
                    in_scope,
                );
            }
        }
        let limit = limit.clamp(1, MAX_PROMPT_STATS_LIMIT);
        let library_rows = compare_library.then_some(stats.library_rows);
        let rank = |counts| ranked_prompt_tags(counts, stats.scope_rows, library_rows, limit, sort_by);
        let mut result = json!({
            "scannedRows": stats.scope_rows,
            "notes": notes,
            "positive": rank(stats.positive),
            "character": rank(stats.character),
            "artists": rank(stats.artists),
        });
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
            .take(limit.clamp(1, 100))
            .map(|(key, (count, text, sample))| {
                json!({
                    "artistString": text,
                    "alias": aliases.get(&key),
                    "imagesInScope": count,
                    "imagesInLibrary": library_counts.get(&key).copied().unwrap_or(0),
                    "sampleRowId": sample,
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "scannedRows": resolved.rows.len(),
            "rowsWithoutArtistString": without,
            "matchingGroups": total,
            "groups": items,
            "notes": resolved.notes,
        }))
    }
}
