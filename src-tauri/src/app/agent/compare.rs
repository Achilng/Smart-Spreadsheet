//! Image comparison for the agent: read the same results the compare window shows (four
//! relations and custom conditions) without touching any window, or drive the compare window.
use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use super::tools::ToolOutput;
use crate::app::AppRuntime;
use crate::app::commands::windows::{COMPARE_FIELDS, CompareCustom, CompareView};
use crate::db::agent::{comparison_tier, model_label, vibe_diff, vibes_json};
use crate::db::{CompareConditions, Database, RowRecord};
use crate::pipeline::prompt_text::normalize_prompt_token;

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 50;
const MAX_TAGS: usize = 12;
const MAX_MEMBERS: usize = 10;

pub const RELATIONS: [&str; 5] = ["artists", "vibeDiffStyle", "styleDiffVibe", "models", "custom"];

fn parse<T: for<'de> Deserialize<'de>>(arguments: &Value) -> Result<T, String> {
    serde_json::from_value(arguments.clone()).map_err(|error| format!("参数格式不对：{error}"))
}

pub fn run(app: &AppHandle, name: &str, arguments: &Value) -> Result<ToolOutput, String> {
    match name {
        "compare_images" => compare_images(&app.state::<AppRuntime>(), arguments),
        "control_compare_window" => control_window(app, arguments),
        other => Err(format!("没有名为 {other} 的工具")),
    }
}

// ---- 读取对比结果 ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompareArgs {
    row_id: i64,
    relation: String,
    #[serde(default)]
    custom: Option<CompareCustom>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

fn compare_images(runtime: &AppRuntime, arguments: &Value) -> Result<ToolOutput, String> {
    let args: CompareArgs = parse(arguments)?;
    runtime.agent_db(false, |db| compare_rows(db, &args))
}

fn compare_rows(db: &mut Database, args: &CompareArgs) -> Result<ToolOutput, String> {
    if !RELATIONS.contains(&args.relation.as_str()) {
        return Err(format!("relation 只能是 {}", RELATIONS.join("、")));
    }
    if (args.relation == "custom") != args.custom.is_some() {
        return Err("relation=custom 时必须给 custom 条件；其它关系不要给 custom".into());
    }
    if let Some(custom) = &args.custom {
        CompareView { tab: None, custom: Some(custom.clone()) }.validate()?;
    }
    let offset = args.offset.unwrap_or(0);
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let row_id = args.row_id;
    let sample = db.get_compare_sample(row_id).map_err(|error| match error {
        crate::db::DatabaseError::RowNotFound(_) => format!("图片 #{row_id} 不存在"),
        other => other.to_string(),
    })?;
    let a = sample.row.clone();
    let mut notes = Vec::new();
    let (mut output, ids, total) = match args.relation.as_str() {
        "models" => {
            if !sample.has_style_signature { notes.push("样本没有可比较的提示词，查不到不同模型的图。"); }
            let section = db.query_compare_same_style_all_models(row_id).map_err(|e| e.to_string())?;
            if section.truncated { notes.push("只比较了最新 500 张同提示词图片，更早的图里可能还有其它模型。"); }
            let own = comparison_tier(a.generation_model.as_deref());
            let mut groups: Vec<(String, Vec<&RowRecord>)> = Vec::new();
            for row in section.rows.iter().filter(|row| comparison_tier(row.generation_model.as_deref()) != own) {
                let label = model_label(row.generation_model.as_deref());
                match groups.iter_mut().find(|(name, _)| *name == label) {
                    Some((_, rows)) => rows.push(row),
                    None => groups.push((label, vec![row])),
                }
            }
            let ids = groups.iter().flat_map(|(_, rows)| rows.iter().take(MAX_MEMBERS).map(|row| row.id)).collect::<Vec<_>>();
            let total = groups.iter().map(|(_, rows)| rows.len()).sum::<usize>();
            let list = groups.iter().map(|(label, rows)| json!({
                "model": label, "count": rows.len(),
                "rowIds": rows.iter().take(MAX_MEMBERS).map(|row| row.id).collect::<Vec<_>>(),
                "firstDifferences": differences(&a, rows[0], &["model"]),
            })).collect::<Vec<_>>();
            (json!({"sampleModel": model_label(a.generation_model.as_deref()), "groups": list, "totalRows": total}), ids, total)
        }
        "custom" => {
            let custom = args.custom.as_ref().expect("checked above");
            let groups = custom_groups(db, &sample.model_names, row_id, custom)?;
            let total = groups.len();
            let page = groups.iter().skip(offset).take(limit).collect::<Vec<_>>();
            let ids = page.iter().map(|group| group[0]).collect::<Vec<_>>();
            let rows = rows_by_id(db, &ids)?;
            let items = page.iter().filter_map(|group| rows.get(&group[0]).map(|row| {
                let mut item = json!({"rowId": row.id, "differences": differences(&a, row, &[custom.observe.as_str()])});
                if custom.dedupe {
                    item["count"] = json!(group.len());
                    item["memberRowIds"] = json!(group.iter().take(MAX_MEMBERS).collect::<Vec<_>>());
                }
                item
            })).collect::<Vec<_>>();
            let unit = if custom.dedupe { "组" } else { "张" };
            (json!({"total": total, "unit": unit, "offset": offset, "items": items}), ids, total)
        }
        relation => {
            let (page, detail): (_, &[&str]) = match relation {
                "artists" => {
                    if a.artists.as_deref().is_none_or(|s| s.trim().is_empty()) { notes.push("样本没有画师串。"); }
                    (db.query_compare_same_artists(row_id, offset as u64, limit as u32), &["positive", "character"])
                }
                "vibeDiffStyle" => {
                    if !sample.has_vibe_signature {
                        notes.push(if sample.vibe_signature_unreadable { "样本原图不可读，读不到 VIBE 引用。" } else { "样本没有引用 VIBE。" });
                    }
                    (db.query_compare_same_vibe_diff_style(row_id, offset as u64, limit as u32), &["artists", "positive", "character", "vibe"])
                }
                _ => {
                    if !sample.has_style_signature { notes.push("样本没有可比较的提示词。"); }
                    (db.query_compare_same_style_diff_vibe(row_id, offset as u64, limit as u32), &["vibe"])
                }
            };
            let page = page.map_err(|e| e.to_string())?;
            let ids = page.rows.iter().map(|row| row.id).collect::<Vec<_>>();
            let items = page.rows.iter().map(|row| {
                let mut diff = differences(&a, row, detail);
                if relation == "styleDiffVibe" && diff.get("vibe").is_none() {
                    diff["vibe"] = json!("读不到两边的逐项明细，只知道引用组合不同");
                }
                json!({"rowId": row.id, "differences": diff})
            }).collect::<Vec<_>>();
            let total = page.total_count as usize;
            (json!({"total": total, "unit": "张", "offset": offset, "items": items}), ids, total)
        }
    };
    output["relation"] = json!(args.relation);
    output["sample"] = json!({"rowId": a.id, "artists": a.artists, "model": model_label(a.generation_model.as_deref()),
        "vibeReferences": a.vibe_reference_count, "vibes": vibes_json(a.vibe_details.as_deref(), a.vibe_reference_count)});
    if !notes.is_empty() { output["notes"] = json!(notes); }
    if args.relation != "models" && offset + ids.len() < total {
        output["nextOffset"] = json!(offset + ids.len());
    }
    Ok(ToolOutput { display: json!({"kind": "rows", "rowIds": ids, "total": total}), output })
}

fn custom_groups(db: &mut Database, model_names: &[String], row_id: i64, custom: &CompareCustom) -> Result<Vec<Vec<i64>>, String> {
    let model_keys = model_names.iter().map(|name| (name.clone(), comparison_tier(Some(name)))).collect::<HashMap<_, _>>();
    let allowed = custom.allowed.clone().unwrap_or_else(|| COMPARE_FIELDS.iter().map(|f| (*f).to_owned()).collect());
    let conditions: CompareConditions = serde_json::from_value(json!({
        "observe": custom.observe, "dedupe": custom.dedupe, "allowed": allowed, "modelKeys": model_keys,
    })).map_err(|error| format!("对比条件无效：{error}"))?;
    db.query_compare_custom(row_id, &conditions).map_err(|e| e.to_string())
}

fn rows_by_id(db: &mut Database, ids: &[i64]) -> Result<HashMap<i64, RowRecord>, String> {
    Ok(db.get_rows_by_ids(ids).map_err(|e| e.to_string())?.into_iter().map(|row| (row.id, row)).collect())
}

fn tokens(value: Option<&str>) -> Vec<String> {
    let mut seen = HashSet::new();
    value.unwrap_or_default()
        .split([',', '，', '\n', '\r'])
        .map(normalize_prompt_token)
        .filter(|token| !token.is_empty() && seen.insert(token.clone()))
        .collect()
}

fn tag_diff(a: Option<&str>, b: Option<&str>, skip_a: &[String], skip_b: &[String]) -> Option<Value> {
    let left = tokens(a).into_iter().filter(|t| !skip_a.contains(t)).collect::<Vec<_>>();
    let right = tokens(b).into_iter().filter(|t| !skip_b.contains(t)).collect::<Vec<_>>();
    let (left_set, right_set) = (left.iter().collect::<HashSet<_>>(), right.iter().collect::<HashSet<_>>());
    let added = right.iter().filter(|t| !left_set.contains(t)).collect::<Vec<_>>();
    let removed = left.iter().filter(|t| !right_set.contains(t)).collect::<Vec<_>>();
    if added.is_empty() && removed.is_empty() {
        return None;
    }
    let mut diff = json!({
        "added": added.iter().take(MAX_TAGS).collect::<Vec<_>>(),
        "removed": removed.iter().take(MAX_TAGS).collect::<Vec<_>>(),
    });
    let more = added.len().saturating_sub(MAX_TAGS) + removed.len().saturating_sub(MAX_TAGS);
    if more > 0 { diff["more"] = json!(more); }
    Some(diff)
}

/// What B changes relative to A. Fields in `detail` are spelled out (added/removed tags, or
/// [A, B] values); every other field that differs is only named in `alsoDifferent`.
fn differences(a: &RowRecord, b: &RowRecord, detail: &[&str]) -> Value {
    let mut out = serde_json::Map::new();
    let mut also = Vec::new();
    let mut note = |key: &str, diff: Option<Value>| {
        if let Some(diff) = diff {
            if detail.contains(&key) { out.insert(key.into(), diff); } else { also.push(key.to_owned()); }
        }
    };
    // The positive prompt already contains the artist string; leave those tags to `artists`.
    let (artists_a, artists_b) = (tokens(a.artists.as_deref()), tokens(b.artists.as_deref()));
    note("artists", tag_diff(a.artists.as_deref(), b.artists.as_deref(), &[], &[]));
    note("positive", tag_diff(a.positive_prompt.as_deref(), b.positive_prompt.as_deref(), &artists_a, &artists_b));
    note("character", tag_diff(a.character_prompt.as_deref(), b.character_prompt.as_deref(), &[], &[]));
    note("negative", tag_diff(a.negative_prompt.as_deref(), b.negative_prompt.as_deref(), &[], &[]));
    let pair = |left: Option<String>, right: Option<String>| (left != right).then(|| json!([left, right]));
    let model = |row: &RowRecord| Some(model_label(row.generation_model.as_deref()));
    note("model", (comparison_tier(a.generation_model.as_deref()) != comparison_tier(b.generation_model.as_deref()))
        .then(|| json!([model(a), model(b)])));
    note("sampler", pair(a.generation_sampler.clone(), b.generation_sampler.clone()));
    note("steps", pair(a.generation_steps.map(|v| v.to_string()), b.generation_steps.map(|v| v.to_string())));
    note("seed", pair(a.generation_seed.clone(), b.generation_seed.clone()));
    note("scale", pair(a.generation_scale.clone(), b.generation_scale.clone()));
    note("cfgRescale", pair(a.generation_cfg_rescale.clone(), b.generation_cfg_rescale.clone()));
    note("noiseSchedule", pair(a.generation_noise_schedule.clone(), b.generation_noise_schedule.clone()));
    let size = |row: &RowRecord| row.image_width.zip(row.image_height).map(|(w, h)| format!("{w}x{h}"));
    note("dimensions", pair(size(a), size(b)));
    fn vibes(row: &RowRecord) -> (Option<&str>, Option<u32>) { (row.vibe_details.as_deref(), row.vibe_reference_count) }
    note("vibe", vibe_diff(vibes(a), vibes(b)));
    if !also.is_empty() {
        out.insert("alsoDifferent".into(), json!(also));
    }
    Value::Object(out)
}

// ---- 操作对比窗口 ----

fn control_window(app: &AppHandle, arguments: &Value) -> Result<ToolOutput, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Args {
        row_id: i64,
        #[serde(default)]
        target_row_id: Option<i64>,
        #[serde(default)]
        relation: Option<String>,
        #[serde(default)]
        custom: Option<CompareCustom>,
    }
    let args: Args = parse(arguments)?;
    if args.target_row_id == Some(args.row_id) {
        return Err("A 和 B 不能是同一张图".into());
    }
    let view = match (args.relation.as_deref(), args.custom) {
        (Some("custom") | None, Some(custom)) => Some(CompareView { tab: None, custom: Some(custom) }),
        (Some("custom"), None) => return Err("relation=custom 时必须给 custom 条件".into()),
        (Some(_), Some(_)) => return Err("给了 custom 条件时 relation 只能是 custom 或不填".into()),
        (Some(tab), None) => Some(CompareView { tab: Some(tab.to_owned()), custom: None }),
        (None, None) => None,
    };
    if let Some(view) = &view {
        view.validate()?;
    }
    let rows = [Some(args.row_id), args.target_row_id].into_iter().flatten().collect::<Vec<_>>();
    app.state::<AppRuntime>().agent_db(false, |db| {
        let found = db.get_rows_by_ids(&rows).map_err(|e| e.to_string())?;
        match rows.iter().find(|id| !found.iter().any(|row| row.id == **id)) {
            Some(missing) => Err(format!("图片 #{missing} 不存在")),
            None => Ok(()),
        }
    })?;
    let compare = json!({"rowId": args.row_id, "targetRowId": args.target_row_id, "view": view});
    let payload = json!({"compare": compare});
    app.emit_to("main", "agent://main-control", payload.clone()).map_err(|e| e.to_string())?;
    Ok(ToolOutput {
        output: json!({"ok": true, "note": "已发送到对比窗口；窗口里显示的结果可再用 get_main_window_state 的 compareWindow 确认。要自己读结果用 compare_images。"}),
        display: json!({"kind": "control", "request": payload}),
    })
}
