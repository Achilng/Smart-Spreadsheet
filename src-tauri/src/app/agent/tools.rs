//! Tool schemas exposed to the model and their dispatch onto the library database.
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use super::{Engine, vision};
use crate::app::AppRuntime;
use crate::automation::model::AutomationRuleDraft;
use crate::db::agent::{Action, DEFAULT_PROMPT_STATS_LIMIT, MAX_PROMPT_STATS_LIMIT, PromptStatsSort, Target};

pub struct ToolOutput {
    /// Sent back to the model.
    pub output: Value,
    /// Rendered by the agent window (cards, counts).
    pub display: Value,
}

fn target_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "properties": {
            "scope": {"type": "string", "enum": ["selection", "filteredView", "all", "rowIds"],
                "description": "selection=主窗口当前选中；filteredView=主窗口当前筛选结果；all=全库；rowIds=指定 id"},
            "rowIds": {"type": "array", "items": {"type": "integer"}},
            "artistStrings": {"type": "array", "items": {"type": "string"},
                "description": "只保留属于这些画师串分组的图（按画师串去重的分组键匹配，写法差异会被忽略）"},
            "expandToSameArtistString": {"type": "boolean",
                "description": "先把范围扩展到与范围内图片同画师串的全库所有图"},
            "favorite": {"type": "boolean", "description": "只保留收藏/未收藏的图"},
            "conditions": {"type": "array", "items": {"type": "object"},
                "description": "额外条件，格式同自动规则条件。常用：{type:'prompt',scope:'positive'|'character'|'negative'|'positiveAndCharacter'|'all',operator:'containsAll'|'containsAny'|'containsNone'|'textContains'|'textEquals'|'regex',value,caseSensitive}；{type:'tag',operator:'hasAll'|'hasAny'|'hasNone'|'isEmpty',tags:[]}；{type:'group',operator:'is'|'isNot'|'isEmpty',groupId}；{type:'note',operator:'contains'|'isEmpty',value}；{type:'artist',operator:'containsAny'|'containsNone'|'isSingle'|'isMultiple'|'isEmpty',artists:[]}"},
            "conditionMatch": {"type": "string", "enum": ["all", "any"], "description": "多个条件是全部满足还是任一满足，默认 all"}
        },
        "required": ["scope"]
    })
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "function",
        "name": name,
        "description": description,
        "parameters": {"type": "object", "properties": properties, "required": required},
    })
}

pub fn definitions() -> Value {
    let actions = json!({
        "type": "array",
        "description": "按顺序执行的修改动作。每个动作是一个对象，type 取值：\
addTags{tags:[...]}；removeTags{tags:[...]}；setGroup{groupName, onlyIfUngrouped?}（分组不存在会新建）；clearGroup{}；\
appendPrompt{field,value}（在末尾追加 TAG）；deletePromptTags{field,value}（删除逗号分隔的整个 TAG，value 可写多个，用逗号分隔；忽略大小写和权重写法，masterpiece 也会删掉 {masterpiece}、1.2::masterpiece::；其余内容、分隔符和换行原样保留；跨多个 TAG 的权重组如 2::masterpiece, best quality:: 只删这个 TAG，保留 2:: 和 ::）；\
replacePrompt{field,find,replace,caseSensitive}（普通文本替换，替换所有出现）；\
regexReplacePrompt{field,pattern,replace,caseSensitive}（正则替换所有匹配，Rust regex 语法，replace 可用 $1）；\
setNote{value}；appendNote{value,separator?}；clearNote{}；setNoteSequence{prefix}；setFavorite{favorite}。\
field 取 positive / character / negative。caseSensitive 必须明确给出。改正向和角色要分别写两个动作。",
        "items": {"type": "object", "properties": {"type": {"type": "string"}}, "required": ["type"]}
    });
    let fields = json!({"type": "array", "items": {"type": "string",
        "enum": ["positive", "character", "negative", "artists", "note", "group", "tags", "favorite", "file"]}});
    Value::Array(vec![
        tool("get_library_overview", "图库概况：总数、收藏数、画师串分组数、全部分组（含 id）和常用 TAG。", json!({}), &[]),
        tool("get_main_window_state", "读取主窗口当前选中了多少张、当前筛选条件和筛选结果数量。用户说「选中的」「现在这些」时先调用。", json!({}), &[]),
        tool("find_rows", "按范围读取图片的文字信息：提示词、画师串、TAG、分组等（分页，每页最多 50 张）。只读文字，看不到画面。", json!({
            "target": target_schema("要查看的图片范围"),
            "fields": fields,
            "limit": {"type": "integer"},
            "offset": {"type": "integer"}
        }), &["target"]),
        tool("view_images", "真正查看图片画面（需要模型支持图片输入）。一次最多 16 张，按 rowIds 顺序展示。1~2 张时逐张原样发给你；3 张及以上时会拼成一张总览图，从左到右、从上到下排列，每格下方标着 #图片编号，单格分辨率较低。需要看清细节时请少看几张（1~2 张）。图片会在工具结果之后作为一条单独的消息附上。", json!({
            "rowIds": {"type": "array", "items": {"type": "integer"}, "description": "要查看的图片 id，最多 16 个"}
        }), &["rowIds"]),
        tool("analyze_text", "统计一段文字或正则在范围内各字段的真实命中情况：命中多少张、出现几次、不同写法、出现在开头/中间/结尾、一张图多次命中。改提示词前必须先用它看边界情况。", json!({
            "target": target_schema("统计范围"),
            "pattern": {"type": "string"},
            "isRegex": {"type": "boolean"},
            "caseSensitive": {"type": "boolean"},
            "fields": {"type": "array", "items": {"type": "string", "enum": ["positive", "character", "negative", "artists", "note"]},
                "description": "默认 positive、character、negative"}
        }), &["target", "pattern", "isRegex", "caseSensitive"]),
        tool("summarize_rows", "统计范围内最常见的正面提示词、角色提示词和独立画师项。每榜独立，同一项在同一张图出现多次只计一次；返回范围内张数、占比（0~1）和最多 3 张样例图片 ID。用它直接聚合，不要用 find_rows 翻页做统计。", json!({
            "target": target_schema("要统计的图片范围"),
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_PROMPT_STATS_LIMIT, "default": DEFAULT_PROMPT_STATS_LIMIT, "description": "每个榜单最多返回多少项"},
            "compareLibrary": {"type": "boolean", "default": false, "description": "是否同时返回全库占比及富集倍数（范围内占比除以全库占比）"},
            "sortBy": {"type": "string", "enum": ["count", "enrichment"], "default": "count", "description": "按范围内张数或富集倍数降序；enrichment 会自动开启全库对比"}
        }), &["target"]),
        tool("list_artist_strings", "列出范围内的画师串分组（与主窗口画师串去重视角一致），可按文字或别名搜索。返回每组在范围内和全库的图片数。", json!({
            "target": target_schema("统计范围，一般用 all"),
            "contains": {"type": "string", "description": "画师串或别名包含的文字（不区分大小写）"},
            "limit": {"type": "integer"}
        }), &["target"]),
        tool("preview_changes", "预演一次修改（不会改数据）。返回 previewId、会改多少张、每个字段改多少、匹配分析、边界提醒和修改前后样例。用户会在界面上看到这份预演。", json!({
            "title": {"type": "string", "description": "一句话说明这次修改，例如「删除正向里 Text: 及之后的内容」"},
            "target": target_schema("要修改的图片范围"),
            "actions": actions
        }), &["title", "target", "actions"]),
        tool("apply_changes", "执行一个预演。只有需求完全明确、预演结果符合用户预期时才能调用。", json!({
            "previewId": {"type": "string"}
        }), &["previewId"]),
        tool("undo_operation", "撤销一次已执行的修改。之后又被改过的图会保留现状并计入跳过数。", json!({
            "operationId": {"type": "integer"}
        }), &["operationId"]),
        tool("list_operations", "列出最近执行过的修改记录（含 id、状态）。", json!({
            "limit": {"type": "integer"}
        }), &[]),
        tool("set_main_selection", "在主窗口中选中范围内的所有图片（不改数据）。", json!({
            "target": target_schema("要选中的图片")
        }), &["target"]),
        tool("show_selected_images", "在主窗口选中并仅显示目标图片。筛图给用户查看时优先调用，可直接传完整筛选条件；查看已勾选的图片用 scope=selection。", json!({
            "target": target_schema("要单独展示的图片范围")
        }), &["target"]),
        tool("set_main_filters", "设置主窗口的筛选（不改数据）。clear=true 先清空所有筛选。", json!({
            "clear": {"type": "boolean"},
            "search": {"type": "string", "description": "搜索框文字"},
            "tags": {"type": "array", "items": {"type": "string"}},
            "tagMode": {"type": "string", "enum": ["and", "or"]},
            "artistFilter": {"type": "string", "description": "只看这个画师串"},
            "dedupe": {"type": "string", "enum": ["none", "artists", "positivePrompt", "vibes"],
                "description": "去重视角：artists=画师串去重"},
            "untaggedOnly": {"type": "boolean"},
            "hideGrouped": {"type": "boolean"}
        }), &[]),
        tool("create_automation_rule", "创建一条自动规则（以后导入新图时自动执行）。只有用户明确同意建立自动规则后才能调用。", json!({
            "rule": {"type": "object", "description": "自动规则：{name, description?, enabled?, runOnImport?(默认 true), runOnUpdate?, conditions:{mode:'all'|'any', groups:[{mode, conditions:[...]}]}, actions:[...]}。actions 格式同 preview_changes，但 setGroup 用 {groupId}（必须是已有分组 id），不支持 setFavorite。"}
        }), &["rule"]),
    ])
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetArgs {
    target: Target,
    #[serde(default)]
    fields: Vec<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

fn parse<T: for<'de> Deserialize<'de>>(arguments: &Value) -> Result<T, String> {
    serde_json::from_value(arguments.clone()).map_err(|error| format!("参数格式不对：{error}"))
}

fn changed(app: &AppHandle) {
    let _ = app.emit("agent://library-changed", ());
    let _ = app.emit("agent://operations-changed", ());
}

/// The preview as the model sees it: groups keep a few example IDs instead of every row.
fn preview_for_model(report: &Value) -> Value {
    let mut value = report.clone();
    for group in value["changeGroups"].as_array_mut().into_iter().flatten() {
        if let Some(ids) = group["rowIds"].as_array_mut() {
            ids.truncate(5);
        }
    }
    value
}

pub fn run(app: &AppHandle, conversation_id: &str, call_id: &str, name: &str, arguments: &Value) -> Result<ToolOutput, String> {
    let runtime = app.state::<AppRuntime>();
    let engine = app.state::<Engine>();
    let view = engine.main_view();
    let plain = |output: Value| Ok(ToolOutput { display: json!({"kind": "info"}), output });
    match name {
        "get_library_overview" => plain(runtime.agent_db(false, |db| db.agent_overview())?),
        "get_main_window_state" => {
            let info = engine.main_info();
            let counts = runtime.agent_db(false, |db| {
                let mut count = |selection: &Option<crate::db::RowSelection>| -> Result<Option<usize>, String> {
                    match selection {
                        Some(selection) => Ok(Some(db.selected_row_ids(selection).map_err(|e| e.to_string())?.len())),
                        None => Ok(None),
                    }
                };
                Ok((count(&view.selection)?, count(&view.filtered)?))
            })?;
            if info.is_null() {
                return plain(json!({"reported": false, "message": "主窗口还没有上报状态（可能没打开资料库）"}));
            }
            plain(json!({"selectedCount": counts.0.unwrap_or(0), "filteredCount": counts.1, "mainWindow": info}))
        }
        "find_rows" => {
            let args: TargetArgs = parse(arguments)?;
            let output = runtime.agent_db(false, |db| {
                db.agent_find_rows(&args.target, &view, &args.fields, args.limit.unwrap_or(20), args.offset.unwrap_or(0))
            })?;
            let ids = output["rows"].as_array().into_iter().flatten().filter_map(|row| row["id"].as_i64()).collect::<Vec<_>>();
            Ok(ToolOutput { display: json!({"kind": "rows", "rowIds": ids, "total": output["total"]}), output })
        }
        "view_images" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                row_ids: Vec<i64>,
            }
            let args: Args = parse(arguments)?;
            let mut ids = Vec::new();
            for id in args.row_ids {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            if ids.is_empty() {
                return Err("rowIds 不能为空".into());
            }
            if ids.len() > vision::MAX_IMAGES {
                return Err(format!("一次最多查看 {} 张，请分批查看", vision::MAX_IMAGES));
            }
            let rendered = vision::render(&runtime, &ids)?;
            app.state::<Engine>().vision.put(call_id, rendered.images.clone());
            let layout = if ids.len() > vision::SEPARATE_LIMIT { "拼成一张总览图，每格下方标着 #图片编号" } else { "逐张附上" };
            Ok(ToolOutput {
                output: json!({"viewed": ids, "missing": rendered.missing, "layout": layout, "note": "图片在下一条消息里"}),
                display: json!({"kind": "images", "rowIds": ids, "missing": rendered.missing}),
            })
        }
        "analyze_text" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                target: Target,
                pattern: String,
                #[serde(default)]
                is_regex: bool,
                #[serde(default)]
                case_sensitive: bool,
                #[serde(default)]
                fields: Vec<String>,
            }
            let args: Args = parse(arguments)?;
            plain(runtime.agent_db(false, |db| {
                db.agent_analyze_text(&args.target, &view, &args.pattern, args.is_regex, args.case_sensitive, &args.fields)
            })?)
        }
        "summarize_rows" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                target: Target,
                limit: Option<usize>,
                #[serde(default)]
                compare_library: bool,
                #[serde(default)]
                sort_by: PromptStatsSort,
            }
            let args: Args = parse(arguments)?;
            plain(runtime.agent_db(false, |db| {
                db.agent_summarize_rows(
                    &args.target,
                    &view,
                    args.limit.unwrap_or(DEFAULT_PROMPT_STATS_LIMIT),
                    args.compare_library,
                    args.sort_by,
                )
            })?)
        }
        "list_artist_strings" => {
            #[derive(Deserialize)]
            struct Args {
                target: Target,
                #[serde(default)]
                contains: Option<String>,
                #[serde(default)]
                limit: Option<usize>,
            }
            let args: Args = parse(arguments)?;
            plain(runtime.agent_db(false, |db| {
                db.agent_artist_strings(&args.target, &view, args.contains.as_deref(), args.limit.unwrap_or(30))
            })?)
        }
        "preview_changes" => {
            #[derive(Deserialize)]
            struct Args {
                title: String,
                target: Target,
                actions: Vec<Action>,
            }
            let args: Args = parse(arguments)?;
            let report = runtime.agent_db(false, |db| {
                db.agent_preview(conversation_id, &args.title, &args.target, &args.actions, &view)
            })?;
            let value = serde_json::to_value(&report).map_err(|e| e.to_string())?;
            Ok(ToolOutput { output: preview_for_model(&value), display: json!({"kind": "preview", "report": value}) })
        }
        "apply_changes" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                preview_id: String,
            }
            let args: Args = parse(arguments)?;
            let record = runtime.agent_db(true, |db| db.agent_apply(conversation_id, &args.preview_id))?;
            changed(app);
            let value = serde_json::to_value(&record).map_err(|e| e.to_string())?;
            Ok(ToolOutput {
                output: json!({"operationId": record.id, "changedRows": record.changed_rows, "title": record.title, "state": "applied"}),
                display: json!({"kind": "operation", "operation": value}),
            })
        }
        "undo_operation" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                operation_id: i64,
            }
            let args: Args = parse(arguments)?;
            let outcome = runtime.agent_db(true, |db| db.agent_undo(args.operation_id))?;
            changed(app);
            let value = serde_json::to_value(&outcome).map_err(|e| e.to_string())?;
            Ok(ToolOutput { display: json!({"kind": "undo", "outcome": value}), output: value })
        }
        "list_operations" => {
            let limit = arguments["limit"].as_u64().unwrap_or(10).clamp(1, 100) as usize;
            let list = runtime.agent_db(false, |db| db.agent_operations(limit))?;
            plain(json!(list.iter().map(|item| json!({
                "id": item.id, "title": item.title, "kind": item.kind, "changedRows": item.changed_rows,
                "state": item.state, "createdAt": item.created_at, "undoMessage": item.undo_message,
            })).collect::<Vec<_>>()))
        }
        "set_main_selection" | "show_selected_images" => {
            let show_only = name == "show_selected_images";
            let args: TargetArgs = parse(arguments)?;
            let (ids, notes) = runtime.agent_db(false, |db| {
                let resolved = db.agent_resolve(&args.target, &view)?;
                Ok((resolved.rows.iter().map(|row| row.id).collect::<Vec<_>>(), resolved.notes))
            })?;
            if ids.is_empty() && !show_only {
                return Err("范围内没有图片，主窗口的选择没有改变".into());
            }
            app.emit_to("main", "agent://main-selection", json!({"rowIds": ids, "showOnly": show_only})).map_err(|e| e.to_string())?;
            Ok(ToolOutput {
                output: json!({"selected": ids.len(), "notes": notes}),
                display: json!({"kind": "selection", "count": ids.len(), "notes": notes}),
            })
        }
        "set_main_filters" => {
            let mut patch = serde_json::Map::new();
            for key in ["search", "tags", "tagMode", "artistFilter", "dedupe", "untaggedOnly", "hideGrouped"] {
                if let Some(value) = arguments.get(key) {
                    patch.insert(key.into(), value.clone());
                }
            }
            let clear = arguments["clear"].as_bool().unwrap_or(false);
            if patch.is_empty() && !clear {
                return Err("没有提供任何筛选条件".into());
            }
            app.emit("agent://main-filters", json!({"clear": clear, "patch": patch})).map_err(|e| e.to_string())?;
            Ok(ToolOutput {
                output: json!({"ok": true, "note": "已发送到主窗口；需要结果数量请再调用 get_main_window_state"}),
                display: json!({"kind": "filters", "clear": clear, "patch": patch}),
            })
        }
        "create_automation_rule" => {
            let draft: AutomationRuleDraft = parse(&arguments["rule"])?;
            let record = runtime.agent_db(true, |db| {
                let rule = db.create_automation_rule(&draft).map_err(|e| e.to_string())?;
                db.agent_record_rule(conversation_id, &rule)
            })?;
            let _ = app.emit("automation-rules://changed", ());
            let _ = app.emit("agent://operations-changed", ());
            let value = serde_json::to_value(&record).map_err(|e| e.to_string())?;
            Ok(ToolOutput {
                output: json!({"operationId": record.id, "created": record.title}),
                display: json!({"kind": "operation", "operation": value}),
            })
        }
        other => Err(format!("没有名为 {other} 的工具")),
    }
}
