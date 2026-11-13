//! Read-only helpers the agent uses to inspect the library before proposing changes.
use std::collections::HashMap;

use serde_json::{Value, json};

use super::changes::analyze_matches;
use super::target::{MainView, Target};
use super::{Res, s};
use crate::automation::matching::build_regex;
use crate::db::Database;

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
        let needle = contains.map(str::to_lowercase).filter(|value| !value.trim().is_empty());
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
                    text.to_lowercase().contains(needle)
                        || aliases.get(key).is_some_and(|alias| alias.to_lowercase().contains(needle))
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
