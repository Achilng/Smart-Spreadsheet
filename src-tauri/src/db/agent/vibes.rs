//! Per-reference VIBE details for the agent: which references an image used and how strongly.
//!
//! Each reference gets a short ID (the first 8 hex digits of its own hash), so the same VIBE is
//! recognisable across images. The 「相同 VIBE」 grouping still ignores strength.
use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::target::{MainView, Target};
use super::{Res, s};
use crate::db::Database;

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 50;
const MAX_COMBINATIONS: usize = 10;

#[derive(Clone, Deserialize)]
pub(crate) struct VibeRef {
    pub key: String,
    pub strength: Option<f64>,
    pub info: Option<f64>,
}

/// None: the details are unknown (not scanned yet, or the original was unreadable).
pub(crate) fn parse_vibes(details: Option<&str>, count: Option<u32>) -> Option<Vec<VibeRef>> {
    match details {
        Some(text) if !text.is_empty() => serde_json::from_str(text).ok(),
        _ if count == Some(0) => Some(Vec::new()),
        _ => None,
    }
}

fn round(value: Option<f64>) -> Value {
    value.map_or(Value::Null, |v| json!((v * 1000.0).round() / 1000.0))
}

fn ref_json(item: &VibeRef) -> Value {
    json!({"id": item.key, "strength": round(item.strength), "info": round(item.info)})
}

/// The `vibes` field of `find_rows`: one entry per reference, in the image's own order.
pub(crate) fn vibes_json(details: Option<&str>, count: Option<u32>) -> Value {
    match parse_vibes(details, count) {
        Some(list) => Value::Array(list.iter().map(ref_json).collect()),
        None => json!(format!("读不到逐项明细（共 {} 个引用；原图可能已不可读）", count.unwrap_or(0))),
    }
}

/// How B's references differ from A's: added, removed, and the same reference used with
/// another strength or information-extracted value. None when nothing differs.
pub(crate) fn vibe_diff(a: (Option<&str>, Option<u32>), b: (Option<&str>, Option<u32>)) -> Option<Value> {
    let (Some(left), Some(right)) = (parse_vibes(a.0, a.1), parse_vibes(b.0, b.1)) else {
        return (a.1 != b.1).then(|| json!({
            "count": [a.1, b.1],
            "note": "有一边读不到逐项明细，只能比较引用数量",
        }));
    };
    let find = |list: &[VibeRef], key: &str| list.iter().find(|item| item.key == key).cloned();
    let added = right.iter().filter(|item| find(&left, &item.key).is_none()).map(ref_json).collect::<Vec<_>>();
    let removed = left.iter().filter(|item| find(&right, &item.key).is_none()).map(ref_json).collect::<Vec<_>>();
    let changed = left.iter().filter_map(|old| {
        let new = find(&right, &old.key)?;
        let differs = |x: Option<f64>, y: Option<f64>| match (x, y) {
            (Some(x), Some(y)) => (x - y).abs() > 1e-6,
            (x, y) => x.is_some() != y.is_some(),
        };
        (differs(old.strength, new.strength) || differs(old.info, new.info)).then(|| json!({
            "id": old.key,
            "strength": [round(old.strength), round(new.strength)],
            "info": [round(old.info), round(new.info)],
        }))
    }).collect::<Vec<_>>();
    if added.is_empty() && removed.is_empty() && changed.is_empty() {
        return None;
    }
    let mut diff = json!({});
    if !added.is_empty() { diff["added"] = json!(added); }
    if !removed.is_empty() { diff["removed"] = json!(removed); }
    if !changed.is_empty() { diff["strengthChanged"] = json!(changed); }
    Some(diff)
}

#[derive(Default)]
struct RefStats {
    images: usize,
    strength: Option<(f64, f64)>,
    info: Option<(f64, f64)>,
    samples: Vec<i64>,
}

fn widen(range: &mut Option<(f64, f64)>, value: Option<f64>) {
    if let Some(value) = value {
        *range = Some(range.map_or((value, value), |(low, high)| (low.min(value), high.max(value))));
    }
}

fn range_json(range: Option<(f64, f64)>) -> Value {
    range.map_or(Value::Null, |(low, high)| json!([round(Some(low)), round(Some(high))]))
}

impl Database {
    /// VIBE usage in a scope. Without `reference`: every reference with its image count, strength
    /// range and sample images, plus the most common combinations. With `reference`: the images
    /// that used that one reference and the strength each used.
    pub(crate) fn agent_vibe_stats(
        &mut self,
        target: &Target,
        view: &MainView,
        reference: Option<&str>,
        limit: Option<usize>,
        offset: usize,
    ) -> Res<Value> {
        let whole = !target.has_row_filters() && target.whole_library_scope();
        let (ids, notes) = if whole { (None, Vec::new()) } else {
            let (ids, notes) = self.agent_scope_ids(target, view)?;
            (Some(serde_json::to_string(&ids).map_err(s)?), notes)
        };
        let predicate = if ids.is_some() { "AND id IN (SELECT value FROM json_each(?1))" } else { "" };
        let sql = format!(
            "SELECT id, vibe_reference_count, vibe_details FROM rows
             WHERE COALESCE(vibe_reference_count, 0) > 0 {predicate} ORDER BY id"
        );
        let scanned: i64 = match &ids {
            Some(ids) => self.connection.query_row("SELECT COUNT(*) FROM json_each(?1)", [ids], |row| row.get(0)),
            None => self.connection.query_row("SELECT COUNT(*) FROM rows", [], |row| row.get(0)),
        }.map_err(s)?;
        let mut statement = self.connection.prepare(&sql).map_err(s)?;
        let mut cursor = statement.query(rusqlite::params_from_iter(ids.iter())).map_err(s)?;
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let mut with_vibes = 0usize;
        let mut unknown = Vec::new();
        let mut refs: HashMap<String, RefStats> = HashMap::new();
        let mut combos: HashMap<String, (usize, Vec<i64>)> = HashMap::new();
        let mut users = Vec::new();
        while let Some(row) = cursor.next().map_err(s)? {
            with_vibes += 1;
            let id: i64 = row.get(0).map_err(s)?;
            let count: Option<u32> = row.get(1).map_err(s)?;
            let details: Option<String> = row.get(2).map_err(s)?;
            let Some(list) = parse_vibes(details.as_deref(), count) else {
                unknown.push(id);
                continue;
            };
            if let Some(key) = reference {
                if let Some(item) = list.iter().find(|item| item.key == key) {
                    users.push((id, item.clone()));
                }
                continue;
            }
            let mut keys = list.iter().map(|item| item.key.clone()).collect::<Vec<_>>();
            keys.sort_unstable();
            keys.dedup();
            let combo = combos.entry(keys.join("+")).or_default();
            combo.0 += 1;
            if combo.1.len() < 3 { combo.1.push(id); }
            let mut seen = Vec::new();
            for item in &list {
                let stats = refs.entry(item.key.clone()).or_default();
                widen(&mut stats.strength, item.strength);
                widen(&mut stats.info, item.info);
                if seen.contains(&item.key) { continue; }
                seen.push(item.key.clone());
                stats.images += 1;
                if stats.samples.len() < 3 { stats.samples.push(id); }
            }
        }
        let mut result = json!({
            "scannedRows": scanned, "rowsWithVibes": with_vibes, "notes": notes,
        });
        if !unknown.is_empty() {
            result["rowsWithoutDetails"] = json!(unknown.len());
            result["rowsWithoutDetailsSample"] = json!(unknown.iter().take(5).collect::<Vec<_>>());
            result["rowsWithoutDetailsNote"] = json!("这些图有 VIBE，但原图读不到逐项明细（原图被移走或删掉）；用「更新现有图片」重新关联原图后能补上");
        }
        if let Some(key) = reference {
            let total = users.len();
            let page = users.iter().skip(offset).take(limit).map(|(id, item)| json!({
                "rowId": id, "strength": round(item.strength), "info": round(item.info),
            })).collect::<Vec<_>>();
            result["reference"] = json!(key);
            result["images"] = json!(total);
            result["firstRowId"] = json!(users.first().map(|(id, _)| id));
            result["offset"] = json!(offset);
            result["rows"] = json!(page);
            if offset + page.len() < total { result["nextOffset"] = json!(offset + page.len()); }
            if total == 0 { result["note"] = json!("范围内没有图用到这个 VIBE 引用编号；编号来自 find_rows 的 vibes 字段或不带 reference 的统计结果"); }
            return Ok(result);
        }
        let mut list = refs.into_iter().collect::<Vec<_>>();
        list.sort_by(|a, b| b.1.images.cmp(&a.1.images).then(a.1.samples.first().cmp(&b.1.samples.first())));
        let distinct = list.len();
        let items = list.iter().skip(offset).take(limit).map(|(key, stats)| json!({
            "id": key, "images": stats.images, "strengthRange": range_json(stats.strength),
            "infoRange": range_json(stats.info), "firstRowId": stats.samples.first(), "sampleRowIds": stats.samples,
        })).collect::<Vec<_>>();
        let mut combos = combos.into_iter().collect::<Vec<_>>();
        combos.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.1.1.first().cmp(&b.1.1.first())));
        let distinct_combos = combos.len();
        result["distinctReferences"] = json!(distinct);
        result["offset"] = json!(offset);
        result["references"] = json!(items);
        if offset + items.len() < distinct { result["nextOffset"] = json!(offset + items.len()); }
        result["distinctCombinations"] = json!(distinct_combos);
        result["topCombinations"] = json!(combos.iter().take(MAX_COMBINATIONS).map(|(keys, (images, samples))| json!({
            "references": keys.split('+').collect::<Vec<_>>(), "images": images, "sampleRowIds": samples,
        })).collect::<Vec<_>>());
        Ok(result)
    }
}

/// Row IDs a vibe stats result points at, for the thumbnail strip.
pub(crate) fn vibe_display_ids(result: &Value) -> Vec<i64> {
    let ids = if let Some(rows) = result["rows"].as_array() {
        rows.iter().filter_map(|row| row["rowId"].as_i64()).collect::<Vec<_>>()
    } else {
        result["references"].as_array().into_iter().flatten().filter_map(|item| item["firstRowId"].as_i64()).collect()
    };
    // One image often carries several references, so firstRowId repeats.
    let mut seen = std::collections::HashSet::new();
    ids.into_iter().filter(|id| seen.insert(*id)).take(16).collect()
}
