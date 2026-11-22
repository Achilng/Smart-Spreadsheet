//! Toolbox features the agent can use: image search by perceptual hash, background LLM
//! extraction of artist strings, and exports (prepared as a card the user completes).
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use super::tools::ToolOutput;
use crate::app::AppRuntime;
use crate::db::RowSelection;
use crate::db::agent::{MainView, Target};

const DEFAULT_THRESHOLD: u32 = 10;
const DEFAULT_MATCHES: usize = 30;
const MAX_MATCHES: usize = 200;

fn parse<T: for<'de> Deserialize<'de>>(arguments: &Value) -> Result<T, String> {
    serde_json::from_value(arguments.clone()).map_err(|error| format!("参数格式不对：{error}"))
}

fn plain(output: Value) -> Result<ToolOutput, String> {
    Ok(ToolOutput { display: json!({"kind": "info"}), output })
}

/// Image IDs a target covers, in library order.
fn resolve_ids(runtime: &AppRuntime, target: &Target, view: &MainView) -> Result<Vec<i64>, String> {
    let ids = runtime.agent_db(false, |db| Ok(db.agent_resolve(target, view)?.rows.iter().map(|row| row.id).collect::<Vec<_>>()))?;
    if ids.is_empty() {
        return Err("范围内没有图片".into());
    }
    Ok(ids)
}

pub fn run(
    app: &AppHandle,
    conversation_id: &str,
    call_id: &str,
    name: &str,
    arguments: &Value,
    view: &MainView,
) -> Result<ToolOutput, String> {
    let runtime = app.state::<AppRuntime>();
    match name {
        "find_similar_images" => find_similar(&runtime, conversation_id, arguments),
        "refresh_perceptual_hashes" => refresh_hashes(app, &runtime),
        "estimate_style_extraction" | "start_style_extraction" => extraction(app, &runtime, name == "start_style_extraction", arguments, view),
        "get_style_extraction_status" => extraction_status(app, &runtime, arguments),
        "control_style_extraction" => control_extraction(app, &runtime, arguments),
        "prepare_export" => prepare_export(&runtime, call_id, arguments, view),
        other => Err(format!("没有名为 {other} 的工具")),
    }
}

// ---- 以图搜图 ----

fn find_similar(runtime: &AppRuntime, conversation_id: &str, arguments: &Value) -> Result<ToolOutput, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        row_id: Option<i64>,
        message_id: Option<i64>,
        attachment_number: Option<u8>,
        threshold: Option<u32>,
        limit: Option<usize>,
    }
    let args: Args = parse(arguments)?;
    let threshold = args.threshold.unwrap_or(DEFAULT_THRESHOLD).min(64);
    let limit = args.limit.unwrap_or(DEFAULT_MATCHES).clamp(1, MAX_MATCHES);
    let (query, hash) = match (args.row_id, args.message_id, args.attachment_number) {
        (Some(row_id), None, None) => {
            let hash = runtime.agent_db(false, |db| db.perceptual_hash_of(row_id).map_err(|error| match error {
                crate::db::DatabaseError::RowNotFound(_) => format!("图片 #{row_id} 不存在"),
                other => other.to_string(),
            }))?;
            let hash = hash.ok_or_else(|| format!(
                "图片 #{row_id} 还没有相似度指纹，搜不了。可以先用 refresh_perceptual_hashes 在后台补算（用户同意后），算完再搜。"
            ))?;
            (json!({"rowId": row_id}), hash)
        }
        (None, Some(message_id), Some(number)) => {
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            let (file_name, encoded) = runtime.agent_db(false, |db| db.agent_image_attachment(conversation_id, message_id, number))?;
            let bytes = STANDARD.decode(encoded).map_err(|_| "无法读取已保存的图片附件")?;
            let hash = crate::storage::compute_phash_bytes(&bytes).map_err(|error| format!("这张附件图片读不出来：{error}"))?;
            (json!({"messageId": message_id, "attachmentNumber": number, "fileName": file_name}), hash)
        }
        _ => return Err("请只给 rowId（图库图片），或者同时给 messageId 和 attachmentNumber（聊天附件）".into()),
    };
    let (hashes, missing) = runtime.agent_db(false, |db| {
        Ok((db.all_perceptual_hashes().map_err(|e| e.to_string())?, db.missing_perceptual_hash_count().map_err(|e| e.to_string())?))
    })?;
    let mut matches = hashes
        .iter()
        .filter(|(id, _)| Some(*id) != args.row_id)
        .filter_map(|(id, stored)| crate::storage::phash_distance(&hash, stored).filter(|distance| *distance <= threshold).map(|distance| (*id, distance)))
        .collect::<Vec<_>>();
    matches.sort_by_key(|&(id, distance)| (distance, id));
    let total = matches.len();
    matches.truncate(limit);
    let ids = matches.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    let output = json!({
        "query": query,
        "threshold": threshold,
        "totalMatches": total,
        "matches": matches.iter().map(|(id, distance)| json!({"rowId": id, "distance": distance})).collect::<Vec<_>>(),
        "missingHashes": missing,
        "refreshing": runtime.perceptual_hash_backfill_running(),
        "note": "distance 是指纹差异（0 = 指纹完全相同），越小越像。只能说明画面几乎相同，不代表画风相似。",
    });
    Ok(ToolOutput { display: json!({"kind": "rows", "rowIds": ids, "total": total}), output })
}

fn refresh_hashes(app: &AppHandle, runtime: &AppRuntime) -> Result<ToolOutput, String> {
    if runtime.perceptual_hash_backfill_running() {
        return plain(json!({"started": false, "running": true, "note": "已经在后台计算了，等它算完再搜"}));
    }
    let missing = runtime.agent_db(false, |db| db.missing_perceptual_hash_count().map_err(|e| e.to_string()))?;
    if missing == 0 {
        return plain(json!({"started": false, "missing": 0, "note": "所有图片都有相似度指纹，不用补算"}));
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let runtime = handle.state::<AppRuntime>();
        let _ = runtime.backfill_perceptual_hashes(|progress| {
            let _ = handle.emit("perceptual-hash://progress", progress);
        });
    });
    plain(json!({
        "started": true,
        "missing": missing,
        "note": "已在后台开始补算（只读原图、不改其它数据），张数多时要等一会儿。之后再调用 find_similar_images，看 missingHashes 是否变少、refreshing 是否为 false；读不到原图的图片会一直缺指纹。",
    }))
}

// ---- LLM 提取 ----

/// The library identity the extraction tables belong to, and the channels a task may use.
fn extraction_context(runtime: &AppRuntime) -> Result<(String, Vec<String>, Vec<String>), String> {
    let (_, library, settings) = runtime.llm_context()?;
    let channels = settings
        .channels
        .iter()
        .filter(|channel| channel.enabled && crate::app::llm::key(channel).is_ok_and(|key| !key.is_empty()))
        .collect::<Vec<_>>();
    Ok((library, channels.iter().map(|c| c.id.clone()).collect(), channels.iter().map(|c| format!("{}（{}）", c.name, c.model)).collect()))
}

fn extraction(app: &AppHandle, runtime: &AppRuntime, start: bool, arguments: &Value, view: &MainView) -> Result<ToolOutput, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        target: Target,
        #[serde(default)]
        include_processed: bool,
        #[serde(default)]
        all_matching: bool,
    }
    let args: Args = parse(arguments)?;
    let ids = resolve_ids(runtime, &args.target, view)?;
    let scope = ids.len();
    let selection = RowSelection::Explicit { row_ids: ids };
    let (library, channels, channel_names) = extraction_context(runtime)?;
    let estimate = runtime.llm_db(&library, |db| db.llm_estimate(Some(&selection), args.include_processed, args.all_matching))?;
    if !start {
        return plain(json!({
            "scopeRows": scope,
            "rows": estimate.rows,
            "skippedRows": estimate.skipped,
            "affectedRows": estimate.affected_rows,
            "uniquePrompts": estimate.unique_prompts,
            "reusablePrompts": estimate.reusable_prompts,
            "waitingPrompts": estimate.waiting_prompts,
            "requestPrompts": estimate.request_prompts,
            "enabledChannels": channel_names,
            "note": "skippedRows=已经提取过或正向为空而跳过的图；affectedRows=会写入画师串的图；requestPrompts=需要真正调用模型的不同正文条数（消耗额度），reusablePrompts 复用历史结果、waitingPrompts 正在别的任务里处理，这两类不花额度。",
        }));
    }
    if estimate.request_prompts > 0 && channels.is_empty() {
        return Err("还没有启用并填好 Key 的 LLM 提取渠道。请让用户到 工具箱 →「LLM提取」→「渠道设置」里配置。".into());
    }
    let job = runtime.llm_db(&library, |db| db.llm_create(Some(&selection), args.include_processed, args.all_matching, channels, false))?;
    let Some(job) = job else {
        return plain(json!({"started": false, "note": "范围内没有需要处理的图（可能都提取过了，或已在别的任务里排队）"}));
    };
    app.state::<crate::app::llm::Engine>().revision.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    plain(json!({
        "started": true,
        "jobId": job,
        "affectedRows": estimate.affected_rows,
        "requestPrompts": estimate.request_prompts,
        "channels": channel_names,
        "note": "任务已在后台开始，结果会自动写入画师串。用户问进度时用 get_style_extraction_status；也可以在 工具箱 →「LLM提取」→「提取任务」里看。",
    }))
}

fn job_state(state: &str) -> &str {
    match state {
        "queued" => "等待提取",
        "running" => "正在提取",
        "paused" => "已暂停",
        "blocked" => "需要处理",
        "completed" => "已完成",
        "cancelled" => "已结束",
        "undone" => "已撤销",
        other => other,
    }
}

fn extraction_status(app: &AppHandle, runtime: &AppRuntime, arguments: &Value) -> Result<ToolOutput, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Args { job_id: Option<String> }
    let args: Args = parse(arguments)?;
    let (library, _, _) = extraction_context(runtime)?;
    let jobs = runtime.llm_db(&library, |db| db.llm_jobs())?;
    let progress = app.state::<crate::app::llm::Engine>().progress.lock().map(|value| value.clone()).ok();
    let selected = match &args.job_id {
        Some(id) => vec![jobs.iter().find(|job| &job.id == id).ok_or_else(|| format!("没有 ID 为 {id} 的提取任务"))?],
        None => jobs.iter().take(5).collect(),
    };
    let list = selected
        .into_iter()
        .map(|job| {
            let live = progress.as_ref().filter(|p| p.job_id == job.id && p.library_id == library);
            json!({
                "jobId": job.id,
                "createdAt": crate::db::agent::local_time(Some(&job.created_at)),
                "state": job_state(&job.state),
                "prompts": job.total, "done": job.done, "failed": job.failed, "noArtists": job.empty, "appliedRows": job.applied,
                "inFlight": live.map(|p| p.in_flight),
                "message": job.message,
            })
        })
        .collect::<Vec<_>>();
    plain(json!({
        "jobs": list,
        "note": "prompts/done/failed/noArtists 按不同正文计（相同正文只提取一次），appliedRows 才是写入的图片张数。没给 jobId 时列出最近 5 个任务。",
    }))
}

fn control_extraction(app: &AppHandle, runtime: &AppRuntime, arguments: &Value) -> Result<ToolOutput, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Args { job_id: String, action: String }
    let args: Args = parse(arguments)?;
    if !["pause", "resume", "cancel", "retry", "undo"].contains(&args.action.as_str()) {
        return Err("action 只能是 pause、resume、cancel、retry、undo".into());
    }
    let (library, channels, _) = extraction_context(runtime)?;
    if matches!(args.action.as_str(), "resume" | "retry") && channels.is_empty() {
        return Err("还没有启用并填好 Key 的 LLM 提取渠道，继续不了".into());
    }
    runtime.llm_db(&library, |db| db.llm_action(&args.job_id, &args.action, channels))?;
    app.state::<crate::app::llm::Engine>().revision.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if args.action == "undo" {
        let _ = app.emit("llm://library-changed", &library);
        let _ = app.emit("agent://library-changed", ());
    }
    let done = match args.action.as_str() {
        "pause" => "已暂停",
        "resume" => "已继续",
        "cancel" => "已结束（保留已完成的结果）",
        "retry" => "已把失败项重新排队",
        _ => "已撤销这次任务写入的画师串（之后又被改过的图保留现状）",
    };
    plain(json!({"ok": true, "jobId": args.job_id, "done": done}))
}

// ---- 导出 ----

pub const EXPORT_FORMATS: &[&str] = &["xlsx", "zhihuijiJson", "rotationJson", "copyImages", "hardlinkImages", "cleanImages"];

fn prepare_export(runtime: &AppRuntime, call_id: &str, arguments: &Value, view: &MainView) -> Result<ToolOutput, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Options {
        note_number_names: Option<bool>,
        artists_as_positive: Option<bool>,
        deduplicate: Option<bool>,
        rename: Option<String>,
        custom_name: Option<String>,
        strip_metadata: Option<bool>,
    }
    #[derive(Deserialize)]
    struct Args {
        title: String,
        target: Target,
        format: String,
        options: Option<Options>,
    }
    let args: Args = parse(arguments)?;
    if !EXPORT_FORMATS.contains(&args.format.as_str()) {
        return Err(format!("format 只能是 {}", EXPORT_FORMATS.join("、")));
    }
    let ids = resolve_ids(runtime, &args.target, view)?;
    let options = args.options.unwrap_or(Options {
        note_number_names: None, artists_as_positive: None, deduplicate: None, rename: None, custom_name: None, strip_metadata: None,
    });
    let options = match args.format.as_str() {
        "zhihuijiJson" => json!({
            "noteNumberNames": options.note_number_names.unwrap_or(false),
            "artistsAsPositive": options.artists_as_positive.unwrap_or(true),
            "deduplicate": options.deduplicate.unwrap_or(false),
        }),
        "cleanImages" => {
            let rename = options.rename.unwrap_or_else(|| "original".into());
            if !["original", "random", "custom"].contains(&rename.as_str()) {
                return Err("options.rename 只能是 original、random、custom".into());
            }
            let custom = options.custom_name.unwrap_or_default().trim().to_owned();
            if rename == "custom" && custom.is_empty() {
                return Err("rename=custom 时要给 customName".into());
            }
            json!({"rename": rename, "customName": custom, "stripMetadata": options.strip_metadata.unwrap_or(true)})
        }
        _ => json!({}),
    };
    let title = if args.title.trim().is_empty() { "导出图片".to_owned() } else { args.title.trim().to_owned() };
    Ok(ToolOutput {
        output: json!({
            "exportId": call_id,
            "rows": ids.len(),
            "format": args.format,
            "options": options,
            "note": "导出卡片已经显示给用户。用户在卡片上点按钮、自己选好保存位置后才会真正导出，导出结果会以一条消息告诉你；在那之前不要说已经导出。",
        }),
        display: json!({"kind": "export", "exportId": call_id, "title": title, "format": args.format, "options": options, "rowIds": ids}),
    })
}
