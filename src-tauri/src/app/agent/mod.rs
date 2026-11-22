//! Conversational library agent: model loop, cancellation and main-window state.
mod compaction;
mod library_data;
mod prompt;
mod responses;
mod sql;
mod toolbox;
pub(crate) mod tools;
pub(crate) mod vision;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::app::AppRuntime;
use crate::db::{Database, RowSelection};
use crate::db::agent::{MainView, Message};

pub const PREFERENCES_KEY: &str = "agent_preferences_v1";
/// Context window assumed when the user has not set one (tokens).
const DEFAULT_CONTEXT_WINDOW: u64 = 200_000;
/// Room kept free for the model's reply and reasoning; past this the older conversation is summarized.
/// Also sent as the request's output cap: some sites otherwise reserve their own default (e.g. 65535)
/// on top of the prompt and reject a request that still fits our budget.
const REPLY_RESERVE: u64 = 32_000;
/// Rough cost of one attached image.
const IMAGE_TOKENS: u64 = 1_500;
/// Automatic re-sends after a network error or a busy site, and compactions after "request too long".
const NETWORK_RETRIES: u32 = 2;
const SHRINK_RETRIES: u32 = 2;
/// Images from at most this many recent `view_images` calls (in the current user turn) are re-sent.
const RECENT_VIEWS: usize = 4;
const MAX_ATTACHMENTS: usize = 4;
const ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;
const TEXT_ATTACHMENT_BYTES: usize = 256 * 1024;
/// Consider at most this many user turns (including the newest) for attachment replay.
const ATTACHMENT_TURNS: usize = 3;

#[derive(Deserialize)]
pub(crate) struct AttachmentInput {
    name: String,
    mime: String,
    data: String,
}

#[derive(Serialize)]
struct AttachmentMetadata {
    name: String,
    mime: &'static str,
    size: usize,
}

fn text_mime_matches(extension: &str, mime: &str) -> bool {
    mime == "text/plain" || match extension {
        "md" => matches!(mime, "text/markdown" | "text/x-markdown"),
        "csv" => mime == "text/csv",
        "tsv" => mime == "text/tab-separated-values",
        "json" => matches!(mime, "application/json" | "text/json"),
        "log" => mime == "text/x-log",
        "yaml" | "yml" => matches!(mime, "application/yaml" | "application/x-yaml" | "text/yaml" | "text/x-yaml"),
        "xml" => matches!(mime, "application/xml" | "text/xml"),
        _ => false,
    }
}

fn base64_size(data: &str) -> Option<usize> {
    if !data.len().is_multiple_of(4) {
        return None;
    }
    let padding = if data.ends_with("==") { 2 } else { usize::from(data.ends_with('=')) };
    Some(data.len() / 4 * 3 - padding)
}

#[derive(Default)]
struct ValidatedAttachments {
    metadata: Vec<AttachmentMetadata>,
    documents: Vec<Option<String>>,
    images: Vec<(u8, Vec<u8>)>,
}

fn validate_attachments(inputs: Vec<AttachmentInput>) -> Result<ValidatedAttachments, String> {
    if inputs.len() > MAX_ATTACHMENTS {
        return Err("每条消息最多可以添加 4 个附件".into());
    }
    let mut remaining = ATTACHMENT_BYTES;
    let mut image_remaining = 64 * 1024 * 1024usize;
    let mut validated = ValidatedAttachments::default();
    for (index, AttachmentInput { name, mime, data }) in inputs.into_iter().enumerate() {
        if name.is_empty() || name.len() > 255 || name.trim() != name || name.ends_with('.')
            || name.chars().any(|c| c.is_control() || "/\\:<>\"|?*".contains(c))
        {
            return Err(format!("第 {} 个附件的文件名无效：只允许不含路径的文件名（最多 255 字节）", index + 1));
        }
        let extension = name.rsplit_once('.').filter(|(stem, _)| !stem.is_empty())
            .map(|(_, ext)| ext.to_ascii_lowercase()).unwrap_or_default();
        let mime = mime.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
        let (canonical, image_format) = match extension.as_str() {
            "png" => ("image/png", Some(image::ImageFormat::Png)),
            "jpg" | "jpeg" => ("image/jpeg", Some(image::ImageFormat::Jpeg)),
            "webp" => ("image/webp", Some(image::ImageFormat::WebP)),
            "pdf" => ("application/pdf", None),
            "txt" | "md" | "csv" | "tsv" | "json" | "log" | "yaml" | "yml" | "xml" => ("text/plain", None),
            _ => return Err(format!("附件「{name}」的扩展名不受支持；请选择 PNG、JPEG、WebP、PDF 或支持的文本文件")),
        };
        let is_text = canonical == "text/plain";
        if if is_text { !text_mime_matches(&extension, &mime) } else { mime != canonical } {
            return Err(format!("附件「{name}」的 MIME 类型和文件扩展名不匹配"));
        }
        let invalid_base64 = || format!("附件「{name}」的数据不是有效的纯 Base64（不应带 data: 前缀）");
        let size = base64_size(&data).ok_or_else(&invalid_base64)?;
        if is_text && size > TEXT_ATTACHMENT_BYTES {
            return Err(format!("文本附件「{name}」超过 256 KiB"));
        }
        if image_format.is_some() {
            if size > 32 * 1024 * 1024 { return Err(format!("图片附件「{name}」超过 32 MiB")); }
            image_remaining = image_remaining.checked_sub(size).ok_or("每条消息的图片合计不能超过 64 MiB")?;
        } else {
            remaining = remaining.checked_sub(size).ok_or("每条消息的文本和 PDF 合计不能超过 8 MiB")?;
        }
        if size == 0 { return Err(format!("附件「{name}」是空文件")); }
        let mut bytes = STANDARD.decode(&data).map_err(|_| invalid_base64())?;
        let payload = if is_text {
            if bytes.starts_with(b"\xef\xbb\xbf") { bytes.drain(..3); }
            Some(String::from_utf8(bytes).ok().filter(|text| !text.contains('\0'))
                .ok_or_else(|| format!("附件「{name}」不是有效的 UTF-8 文本"))?)
        } else if let Some(format) = image_format {
            let invalid = || format!("图片附件「{name}」损坏或与声明的格式不符");
            if image::guess_format(&bytes).ok() != Some(format) { return Err(invalid()); }
            let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
                .into_dimensions().map_err(|_| invalid())?;
            if width == 0 || height == 0 { return Err(invalid()); }
            if u64::from(width) * u64::from(height) > 32_000_000 {
                return Err(format!("图片附件「{name}」超过 3200 万像素"));
            }
            validated.images.push(((index + 1) as u8, bytes));
            None
        } else {
            let valid_header = (bytes.starts_with(b"%PDF-1.") || bytes.starts_with(b"%PDF-2."))
                && bytes.get(7).is_some_and(u8::is_ascii_digit)
                && matches!(bytes.get(8), Some(b'\r' | b'\n'));
            if !valid_header || !bytes.trim_ascii_end().ends_with(b"%%EOF") {
                return Err(format!("附件「{name}」不是有效的 PDF（文件头或结束标记无效）"));
            }
            Some(data)
        };
        validated.metadata.push(AttachmentMetadata { name, mime: canonical, size });
        validated.documents.push(payload);
    }
    Ok(validated)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_effort")]
    pub effort: String,
    #[serde(default)]
    pub channel_models: std::collections::HashMap<String, ChannelModels>,
    /// Context window in tokens; 0 means the default.
    #[serde(default)]
    pub context_window: u64,
}

impl Preferences {
    pub fn context_window(&self) -> u64 {
        match self.context_window {
            0 => DEFAULT_CONTEXT_WINDOW,
            window => window.clamp(16_000, 10_000_000),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelModels {
    pub model: String,
    pub recent: Vec<String>,
}

fn default_effort() -> String {
    "medium".into()
}

#[derive(Default)]
struct MainState {
    selection: Option<RowSelection>,
    filtered: Option<RowSelection>,
    info: Value,
}

#[derive(Default)]
pub struct Engine {
    main: Mutex<MainState>,
    /// What the image comparison window last reported (sample A, B, open tab).
    compare: Mutex<Value>,
    runs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub(crate) vision: vision::Cache,
}

impl Engine {
    pub fn report_main_view(&self, selection: Option<RowSelection>, filtered: Option<RowSelection>, info: Value) {
        if let Ok(mut main) = self.main.lock() {
            *main = MainState { selection, filtered, info };
        }
    }

    pub fn main_view(&self) -> MainView {
        self.main
            .lock()
            .map(|main| MainView { selection: main.selection.clone(), filtered: main.filtered.clone() })
            .unwrap_or_default()
    }

    pub fn main_info(&self) -> Value {
        self.main.lock().map(|main| main.info.clone()).unwrap_or(Value::Null)
    }

    pub fn report_compare_view(&self, info: Value) {
        if let Ok(mut compare) = self.compare.lock() {
            *compare = info;
        }
    }

    pub fn compare_info(&self) -> Value {
        self.compare.lock().map(|compare| compare.clone()).unwrap_or(Value::Null)
    }

    pub fn is_running(&self, conversation_id: &str) -> bool {
        self.runs.lock().is_ok_and(|runs| runs.contains_key(conversation_id))
    }

    pub fn running(&self) -> Vec<String> {
        self.runs.lock().map(|runs| runs.keys().cloned().collect()).unwrap_or_default()
    }

    pub fn cancel(&self, conversation_id: &str) {
        if let Some(flag) = self.runs.lock().ok().and_then(|runs| runs.get(conversation_id).cloned()) {
            flag.store(true, Ordering::SeqCst);
        }
    }

    fn begin(&self, conversation_id: &str) -> Result<Arc<AtomicBool>, String> {
        let mut runs = self.runs.lock().map_err(|_| "状态异常")?;
        if runs.contains_key(conversation_id) {
            return Err("这个对话还在运行中，请等它结束或先停止".into());
        }
        let flag = Arc::new(AtomicBool::new(false));
        runs.insert(conversation_id.to_owned(), flag.clone());
        Ok(flag)
    }

    fn finish(&self, conversation_id: &str) {
        if let Ok(mut runs) = self.runs.lock() {
            runs.remove(conversation_id);
        }
    }
}

pub fn read_preferences(db: &Database) -> Result<Preferences, crate::db::DatabaseError> {
    Ok(db.setting(PREFERENCES_KEY)?
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| Preferences { effort: default_effort(), ..Default::default() }))
}

fn emit(app: &AppHandle, conversation_id: &str, kind: &str, extra: Value) {
    let mut payload = json!({"conversationId": conversation_id, "kind": kind});
    if let (Some(target), Value::Object(extra)) = (payload.as_object_mut(), extra) {
        target.extend(extra);
    }
    let _ = app.emit("agent://event", payload);
}

pub(crate) fn push(app: &AppHandle, conversation_id: &str, kind: &str, content: Value) -> Result<i64, String> {
    let id = app
        .state::<AppRuntime>()
        .agent_db(false, |db| db.agent_push_message(conversation_id, kind, &content))?;
    emit(app, conversation_id, "messages-changed", json!({}));
    Ok(id)
}

fn new_conversation_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    format!("c{nanos:x}")
}

/// Start (or continue) a conversation with a user message; the model loop runs in the background.
pub(crate) fn send(
    app: AppHandle,
    conversation_id: Option<String>,
    text: String,
    attachments: Vec<AttachmentInput>,
) -> Result<String, String> {
    let text = text.trim();
    let ValidatedAttachments { metadata, documents, images } = validate_attachments(attachments)?;
    if text.is_empty() && metadata.is_empty() {
        return Err("请输入消息或添加附件".into());
    }
    let runtime = app.state::<AppRuntime>();
    let conversation_id = match conversation_id {
        Some(id) => {
            runtime
                .agent_db(false, |db| db.agent_conversation(&id))?
                .ok_or("对话不存在（可能已被删除）")?;
            id
        }
        None => {
            let id = new_conversation_id();
            let title_source = if text.is_empty() { metadata[0].name.as_str() } else { text };
            let title = title_source.chars().take(24).collect::<String>();
            runtime.agent_db(false, |db| db.agent_create_conversation(&id, &title))?;
            id
        }
    };
    let engine = app.state::<Engine>();
    let cancel = engine.begin(&conversation_id)?;
    let mut content = json!({"text": text});
    if !metadata.is_empty() {
        content["attachments"] = json!(metadata);
        if documents.iter().any(Option::is_some) {
            content["attachmentData"] = json!(documents);
        }
    }
    if let Err(error) = runtime.agent_db(false, |db| db.agent_push_with_images(&conversation_id, "user", &content, &images)) {
        engine.finish(&conversation_id);
        return Err(error);
    }
    emit(&app, &conversation_id, "messages-changed", json!({}));
    let id = conversation_id.clone();
    tauri::async_runtime::spawn(async move {
        emit(&app, &id, "run-start", json!({}));
        let result = run_loop(&app, &id, cancel.clone()).await;
        if let Err(message) = result {
            let stopped = cancel.load(Ordering::SeqCst);
            let _ = push(&app, &id, "error", json!({"message": if stopped { "已停止".to_owned() } else { message }, "stopped": stopped}));
        }
        app.state::<Engine>().finish(&id);
        emit(&app, &id, "run-end", json!({}));
    });
    Ok(conversation_id)
}

/// `view_images` results whose images go back to the model: the latest few since the user last spoke.
fn attachment_view(content: &Value) -> bool {
    content["name"] == "view_attachment_image" && content["ok"] == true
}

type RecentView = (String, Option<Vec<i64>>, Option<vision::OriginalView>);

fn recent_views(messages: &[crate::db::agent::Message]) -> Vec<RecentView> {
    let start = messages.iter().rposition(|message| message.kind == "user").unwrap_or(0);
    let mut views = messages[start..]
        .iter()
        .filter(|message| message.kind == "tool_result")
        .filter_map(|message| {
            let ids = vision::viewed_ids(&message.content);
            if ids.is_none() && !attachment_view(&message.content) { return None; }
            Some((message.content["callId"].as_str()?.to_owned(), ids, vision::original_view(&message.content)))
        })
        .collect::<Vec<_>>();
    views.drain(..views.len().saturating_sub(RECENT_VIEWS));
    views
}

/// Images for the recent views, rendered again when they are no longer cached.
async fn attach_views(app: &AppHandle, conversation_id: &str, messages: &[crate::db::agent::Message]) -> vision::Attached {
    let mut attached = vision::Attached::new();
    for (call_id, ids, original) in recent_views(messages) {
        if ids.is_none() {
            if let Some(view) = messages.iter().rev().filter(|m| m.content["callId"].as_str() == Some(&call_id)).find_map(|m| vision::AttachmentView::from_result(&m.content))
                && let Some(images) = app.state::<Engine>().vision.attachment(conversation_id, &call_id, &view) {
                attached.insert(call_id, images);
            }
            continue;
        }
        if let Some(images) = app.state::<Engine>().vision.get(&call_id) {
            attached.insert(call_id, images);
            continue;
        }
        let Some(ids) = ids else { continue; }; // Attachment cache miss: never interpret the reference as a gallery row.
        let handle = app.clone();
        let rendered = tauri::async_runtime::spawn_blocking(move || {
            let runtime = handle.state::<AppRuntime>();
            match (original, ids.as_slice()) {
                (Some(view), [id]) => vision::render_original(&runtime, *id, &view).map(|(url, _)| vec![url]),
                _ => vision::render(&runtime, &ids).map(|rendered| rendered.images),
            }
        }).await;
        if let Ok(Ok(images)) = rendered {
            app.state::<Engine>().vision.put(&call_id, images.clone());
            attached.insert(call_id, images);
        }
    }
    attached
}

fn image_message(content: &Value, images: &[String]) -> Value {
    let ids = vision::viewed_ids(content).unwrap_or_default();
    let list = ids.iter().map(|id| format!("#{id}")).collect::<Vec<_>>().join("、");
    let text = if attachment_view(content) {
        "（view_attachment_image 的结果）这是工具返回的附件预览或局部裁切，不是原图文件。".to_owned()
    } else if let Some(view) = vision::original_view(content) {
        format!("（view_images 的结果，不是用户发的消息）#{} 的原图{}", ids.first().copied().unwrap_or_default(),
            if view.crop.is_some() { "局部裁切" } else { "高清视图" })
    } else if ids.len() > vision::SEPARATE_LIMIT {
        format!("（view_images 的结果，不是用户发的消息）这 {} 张图拼成了一张总览图，从左到右、从上到下排列，每格下方标着图片编号：{list}", ids.len())
    } else {
        format!("（view_images 的结果，不是用户发的消息）按顺序依次是：{list}")
    };
    let mut parts = vec![json!({"type": "input_text", "text": text})];
    parts.extend(images.iter().map(|url| json!({"type": "input_image", "image_url": url, "detail": "high"})));
    json!({"role": "user", "content": parts})
}

type AttachmentData = HashMap<(i64, usize), String>;

/// Replay recent documents within one upload's budget. Images are always references.
fn load_attachment_data(db: &Database, conversation_id: &str, messages: &[Message]) -> AttachmentData {
    let (mut bytes_left, mut files_left) = (ATTACHMENT_BYTES, MAX_ATTACHMENTS);
    let mut data = AttachmentData::new();
    for message in messages.iter().rev().filter(|message| message.kind == "user").take(ATTACHMENT_TURNS) {
        let Some(files) = message.content["attachments"].as_array() else { continue };
        let documents: Vec<_> = files.iter().enumerate().filter(|(_, file)|
            matches!(file["mime"].as_str(), Some("text/plain" | "application/pdf"))
        ).collect();
        if documents.len() > files_left { continue; }
        let remaining = documents.iter().try_fold(bytes_left, |budget, (_, file)| {
            budget.checked_sub(usize::try_from(file["size"].as_u64()?).ok()?)
        });
        if let Some(remaining) = remaining {
            bytes_left = remaining;
            files_left -= documents.len();
            for (slot, _) in documents {
                if let Ok(payload) = db.agent_document_attachment(conversation_id, message.id, slot) {
                    data.insert((message.id, slot), payload);
                }
            }
        }
    }
    data
}

fn user_message(message: &Message, attachment_data: &AttachmentData) -> Value {
    let content = &message.content;
    let text = content["text"].as_str().unwrap_or_default();
    let Some(files) = content["attachments"].as_array().filter(|files| !files.is_empty()) else {
        return json!({"role": "user", "content": text});
    };
    let mut parts = Vec::new();
    if !text.is_empty() {
        parts.push(json!({"type": "input_text", "text": text}));
    }
    for (index, file) in files.iter().enumerate() {
        let name = file["name"].as_str().unwrap_or("未命名附件");
        let payload = attachment_data.get(&(message.id, index));
        let image_ref = format!("messageId={}, attachmentNumber={}", message.id, index + 1);
        match (file["mime"].as_str(), payload) {
            (Some("text/plain"), Some(text)) => {
                parts.push(json!({"type": "input_text", "text": format!("文本附件「{name}」：\n{text}")}));
            }
            (Some("image/png" | "image/jpeg" | "image/webp"), _) => {
                parts.push(json!({"type": "input_text", "text": format!("图片附件「{name}」（{image_ref}）：本次仅附引用，可按需调用工具查看画面或读取生成信息。 ")}));
            }
            (Some("application/pdf"), Some(data)) => {
                parts.push(json!({"type": "input_file", "filename": name, "file_data": format!("data:application/pdf;base64,{data}")}));
            }
            _ => parts.push(json!({"type": "input_text", "text": format!("附件「{name}」（本次仅附带文件名，未发送文件内容）")})),
        }
    }
    json!({"role": "user", "content": parts})
}

/// Rough token count: about 3 ASCII characters per token, one per CJK or other character.
fn estimate_text(text: &str) -> u64 {
    let (ascii, other) = text.chars().fold((0u64, 0u64), |(a, o), c| if c.is_ascii() { (a + 1, o) } else { (a, o + 1) });
    ascii.div_ceil(3) + other
}

fn estimate_value(value: &Value) -> u64 {
    match value {
        Value::String(text) => estimate_text(text),
        Value::Array(items) => items.iter().map(estimate_value).sum(),
        Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| match key.as_str() {
                "image_url" => IMAGE_TOKENS,
                "file_data" => value.as_str().map_or(0, |data| data.len() as u64 / 6),
                "encrypted_content" => value.as_str().map_or(0, |data| data.len() as u64 / 4),
                _ => estimate_value(value) + 1,
            })
            .sum(),
        _ => 1,
    }
}

/// How the site's real token counts compared with our estimate last time (1.0 when unknown).
fn calibration(messages: &[Message]) -> f64 {
    messages
        .iter()
        .rev()
        .find_map(|message| {
            let usage = &message.content["usage"];
            let input = usage["input"].as_u64()? as f64;
            let estimate = usage["estimate"].as_u64().filter(|&value| value > 0)? as f64;
            Some((input / estimate).clamp(0.25, 2.5))
        })
        .unwrap_or(1.0)
}

fn build_input(messages: &[Message], attached: &vision::Attached, attachment_data: &AttachmentData) -> Vec<Value> {
    let answered = messages
        .iter()
        .filter(|message| message.kind == "tool_result")
        .filter_map(|message| message.content["callId"].as_str().map(str::to_owned))
        .collect::<HashSet<_>>();
    let mut input = Vec::new();
    let tool_output = |message: &Message, images: &mut Vec<Value>| {
        let content = &message.content;
        let mut output = content["output"].as_str().unwrap_or_default().to_owned();
        if attachment_view(content) || vision::viewed_ids(content).is_some() {
            match content["callId"].as_str().and_then(|id| attached.get(id)) {
                Some(urls) => images.push(image_message(content, urls)),
                None if attachment_view(content) => output.push_str("\n（本次不附该预览；需要再看时请重新调用 view_attachment_image。）"),
                None => output.push_str("\n（这是较早查看的图片，图片本身已不再附带；需要再看时请重新调用 view_images）"),
            }
        }
        json!({"type": "function_call_output", "call_id": content["callId"], "output": output})
    };
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        let content = &message.content;
        index += 1;
        match message.kind.as_str() {
            "user" => input.push(user_message(message, attachment_data)),
            "user_action" => input.push(json!({"role": "user", "content": content["text"]})),
            "assistant" => {
                input.extend(content["reasoning"].as_array().into_iter().flatten().cloned());
                input.push(json!({"role": "assistant", "content": content["text"]}));
            }
            "tool_call" => {
                // Calls returned in one model response are replayed together, then their results;
                // providers that convert to chat messages need them in a single assistant turn.
                let mut batch = vec![message];
                let mut results = Vec::new();
                if let Some(turn) = content["turn"].as_str() {
                    while let Some(next) = messages.get(index) {
                        let same_turn = match next.kind.as_str() {
                            "tool_call" => next.content["turn"].as_str() == Some(turn),
                            "tool_result" => batch.iter().any(|call| call.content["callId"] == next.content["callId"]),
                            _ => false,
                        };
                        if !same_turn {
                            break;
                        }
                        if next.kind == "tool_call" { batch.push(next) } else { results.push(next) }
                        index += 1;
                    }
                }
                for call in &batch {
                    input.extend(call.content["reasoning"].as_array().into_iter().flatten().cloned());
                }
                for call in &batch {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": call.content["callId"],
                        "name": call.content["name"],
                        "arguments": call.content["arguments"],
                    }));
                }
                // Images follow all of the batch's outputs: tool results must stay next to their calls.
                let mut images = Vec::new();
                for call in &batch {
                    let call_id = call.content["callId"].as_str().unwrap_or_default();
                    if let Some(result) = results.iter().find(|result| result.content["callId"] == call.content["callId"]) {
                        input.push(tool_output(result, &mut images));
                    } else if !answered.contains(call_id) {
                        input.push(json!({"type": "function_call_output", "call_id": call_id,
                            "output": "已被用户中断，未执行"}));
                    }
                }
                input.extend(images);
            }
            "tool_result" => {
                let mut images = Vec::new();
                input.push(tool_output(message, &mut images));
                input.extend(images);
            }
            "error" => {
                if content["stopped"].as_bool() == Some(true) {
                    input.push(json!({"role": "user", "content": "（我中途停止了上一轮回复）"}));
                }
            }
            _ => {}
        }
    }
    input
}

/// The model site and model a conversation talks to.
struct Site<'a> {
    base_url: &'a str,
    key: &'a str,
    model: &'a str,
    effort: &'a str,
    /// Output cap sent with every request (reply plus reasoning).
    max_output: u64,
}

/// Wait 2 s, then 4 s, … before retrying; stops early when the user cancels.
async fn wait_before_retry(attempt: u32, cancel: &AtomicBool) -> Result<(), String> {
    let wait = std::time::Duration::from_secs(2 * u64::from(attempt));
    let started = std::time::Instant::now();
    while started.elapsed() < wait {
        if cancel.load(Ordering::SeqCst) {
            return Err("已停止".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Ok(())
}

/// One summarization request (no tools), retried on network trouble like a normal turn.
async fn summarize(site: &Site<'_>, prompt: &str, cancel: Arc<AtomicBool>) -> Result<String, String> {
    let mut attempt = 0;
    loop {
        let request = responses::Request {
            base_url: site.base_url,
            key: site.key,
            model: site.model,
            effort: site.effort,
            instructions: compaction::SYSTEM_PROMPT,
            input: vec![json!({"role": "user", "content": prompt})],
            has_user_attachments: false,
            tools: json!([]),
            max_output_tokens: site.max_output,
        };
        match responses::stream(request, cancel.clone(), |_: responses::Delta<'_>| {}).await {
            // A cut-off summary must not become the conversation's memory.
            Ok(outcome) if outcome.truncated => {
                return Err("整理较早的对话时，模型的输出被截断了，摘要不完整，所以没有压缩。可以换一个输出上限更大的模型，或新开一个对话。".into());
            }
            Ok(outcome) if !outcome.text.trim().is_empty() => return Ok(outcome.text.trim().to_owned()),
            Ok(_) => return Err("整理较早的对话时，模型没有返回摘要内容。".into()),
            Err(failure) => {
                if cancel.load(Ordering::SeqCst) {
                    return Err("已停止".into());
                }
                if failure.retry && attempt < NETWORK_RETRIES {
                    attempt += 1;
                    wait_before_retry(attempt, &cancel).await?;
                    continue;
                }
                return Err(format!("整理较早的对话（压缩上下文）失败：{}", failure.message));
            }
        }
    }
}

/// Summarize what `plan` drops: whole turns (merged into the previous summary) and, when the cut
/// splits a turn, that turn's beginning (pi's `compact`).
async fn compact(site: &Site<'_>, plan: &compaction::Plan, limit: u64, cancel: Arc<AtomicBool>) -> Result<String, String> {
    let history = match compaction::history_prompt(plan, limit) {
        Some(prompt) => Some(summarize(site, &prompt, cancel.clone()).await?),
        None => None,
    };
    let prefix = match compaction::prefix_prompt(plan, limit) {
        Some(prompt) => Some(summarize(site, &prompt, cancel.clone()).await?),
        None => None,
    };
    Ok(compaction::merge(plan, history, prefix))
}

async fn run_loop(app: &AppHandle, conversation_id: &str, cancel: Arc<AtomicBool>) -> Result<(), String> {
    let runtime = app.state::<AppRuntime>();
    let (_, library, settings) = runtime.llm_context()?;
    let preferences = runtime.llm_db(&library, |db| read_preferences(db))?;
    let channel = settings
        .channels
        .iter()
        .find(|channel| channel.id == preferences.channel_id)
        .or_else(|| settings.channels.first())
        .ok_or("还没有可用的模型渠道。请先在工具箱「LLM提取」→「渠道设置」里添加渠道和 Key。")?
        .clone();
    let selected_model = if channel.id == preferences.channel_id || preferences.channel_id.is_empty() {
        preferences.model.as_str()
    } else {
        preferences.channel_models.get(&channel.id).map_or("", |saved| saved.model.as_str())
    };
    let model = if selected_model.trim().is_empty() { channel.model.trim() } else { selected_model.trim() };
    if model.is_empty() {
        return Err("还没有选择模型，请在右上角选择一个模型。".into());
    }
    let key = crate::app::llm::key(&channel)?;
    if key.is_empty() {
        return Err(format!("渠道「{}」还没有填写 Key。", channel.name));
    }
    let base_instructions = format!(
        "{}

## 可按需加载的工具组
需要时用 load_tools 加载，加载后本对话一直可用：
{}",
        prompt::SYSTEM_PROMPT,
        tools::groups_prompt(),
    );
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M（UTC%:z）").to_string();
    let window = preferences.context_window();
    // Past this the older conversation is summarized; lowered for the rest of the run if the site says it is too long.
    let reserve = REPLY_RESERVE.min(window / 4);
    let mut budget = window.saturating_sub(reserve);
    let site = Site { base_url: &channel.base_url, key: &key, model, effort: &preferences.effort, max_output: reserve };
    let (mut network_retries, mut shrink_retries) = (0, 0);
    // Set when the site said the request is too long: compact now, keeping this much recent conversation.
    let mut forced_keep: Option<u64> = None;
    // At most one automatic compaction between two model replies, so it can never loop.
    let mut compacted_since_reply = false;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("已停止".into());
        }
        if runtime.agent_library_id()? != library {
            return Err("资料库已切换，这轮对话已停止".into());
        }
        let all = runtime.agent_db(false, |db| db.agent_messages(conversation_id))?;
        let compaction::View { summary, messages } = compaction::view(&all);
        let attachment_data = runtime.agent_db(false, |db| Ok(load_attachment_data(db, conversation_id, &messages)))?;
        let attached = attach_views(app, conversation_id, &messages).await;
        // Tool groups loaded before a compaction stay loaded, and so do their usage notes.
        let active = tools::active_tools(&all);
        let tools = tools::definitions(&active);
        let guides = tools::loaded_guides(&runtime, &active);
        let instructions = if guides.is_empty() {
            format!("{base_instructions}\n\n当前本地时间：{now}")
        } else {
            format!("{base_instructions}\n\n## 已加载的工具组用法\n{guides}\n\n当前本地时间：{now}")
        };
        let fixed = estimate_text(&instructions) + estimate_value(&tools);
        let ratio = calibration(&all);
        let mut input = summary.as_deref().map(compaction::summary_message).into_iter().collect::<Vec<_>>();
        input.extend(build_input(&messages, &attached, &attachment_data));
        let estimate = fixed + input.iter().map(estimate_value).sum::<u64>();
        let expected = (estimate as f64 * ratio) as u64;
        let compactions = all.iter().filter(|message| message.kind == "compaction").count();
        let forced = forced_keep.take();
        if forced.is_some() || (expected > budget && !compacted_since_reply) {
            match compaction::prepare(&all, forced.unwrap_or(compaction::KEEP_RECENT_TOKENS)) {
                Some(plan) => {
                    emit(app, conversation_id, "retry", json!({"reason": if forced.is_some() { "too-long" } else { "compact" }}));
                    // The summarizer's own request has to fit as well.
                    let limit = (budget as f64 / ratio * 0.7) as u64;
                    let summary = compact(&site, &plan, limit, cancel.clone()).await?;
                    // What the next request will roughly be, for the context meter until the site reports.
                    let kept = messages.iter().filter(|message| message.id >= plan.first_kept).cloned().collect::<Vec<_>>();
                    let after = fixed
                        + estimate_value(&compaction::summary_message(&summary))
                        + build_input(&kept, &attached, &attachment_data).iter().map(estimate_value).sum::<u64>();
                    push(app, conversation_id, "compaction", json!({
                        "summary": summary,
                        "firstKeptId": plan.first_kept,
                        "tokensBefore": expected,
                        "usage": {
                            "window": window, "estimate": after, "used": (after as f64 * ratio) as u64,
                            "source": "estimate", "compactions": compactions + 1,
                        },
                    }))?;
                    compacted_since_reply = true;
                    continue;
                }
                None if forced.is_some() => {
                    return Err("对话已经超出模型能接收的长度，又没有可以再压缩的较早内容。请新开一个对话继续。".into());
                }
                None => {}
            }
        }
        let request = responses::Request {
            base_url: site.base_url,
            key: site.key,
            model: site.model,
            effort: site.effort,
            instructions: &instructions,
            input,
            has_user_attachments: !attachment_data.is_empty(),
            tools,
            max_output_tokens: site.max_output,
        };
        let result = responses::stream(request, cancel.clone(), |delta| match delta {
            responses::Delta::Text(text) => emit(app, conversation_id, "text-delta", json!({"delta": text})),
            responses::Delta::Reasoning(text) => emit(app, conversation_id, "reasoning-delta", json!({"delta": text})),
        })
        .await;
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(failure) => {
                if cancel.load(Ordering::SeqCst) {
                    return Err("已停止".into());
                }
                // Nothing from a failed attempt is saved and tools only run after a complete reply,
                // so sending again is safe; the "retry" event tells the window to drop the partial text.
                if failure.too_long && shrink_retries < SHRINK_RETRIES {
                    shrink_retries += 1;
                    budget = budget.min(expected * 8 / 10);
                    forced_keep = Some(compaction::KEEP_RECENT_TOKENS >> shrink_retries);
                    continue;
                }
                if failure.retry && network_retries < NETWORK_RETRIES {
                    network_retries += 1;
                    emit(app, conversation_id, "retry", json!({"reason": "network", "attempt": network_retries, "message": failure.message}));
                    wait_before_retry(network_retries, &cancel).await?;
                    continue;
                }
                return Err(failure.message);
            }
        };
        network_retries = 0;
        compacted_since_reply = false;
        // What the next request will start from: this request plus the reply that joins it.
        let reply = estimate_text(&outcome.text)
            + outcome.calls.iter().map(|call| estimate_text(&call.arguments) + 20).sum::<u64>();
        let mut usage = json!({
            "window": window,
            "estimate": estimate,
            "compactions": compactions,
        });
        match outcome.usage {
            Some(real) => {
                usage["input"] = json!(real.input);
                usage["used"] = json!(real.input + real.output);
                usage["source"] = json!("api");
            }
            None => {
                usage["used"] = json!(((estimate + reply) as f64 * ratio) as u64);
                usage["source"] = json!("estimate");
            }
        }
        let mut usage = Some(usage);
        // Reasoning precedes this turn's output, so it rides on the first stored message.
        let mut reasoning = (!outcome.reasoning.is_empty()).then(|| Value::Array(outcome.reasoning));
        if !outcome.text.trim().is_empty() {
            let mut content = json!({"text": outcome.text});
            if let Some(items) = reasoning.take() {
                content["reasoning"] = items;
            }
            if let Some(usage) = usage.take() {
                content["usage"] = usage;
            }
            push(app, conversation_id, "assistant", content)?;
        }
        if outcome.calls.is_empty() {
            return Ok(());
        }
        let turn = outcome.calls.first().map(|call| call.call_id.clone()).unwrap_or_default();
        for call in outcome.calls {
            if cancel.load(Ordering::SeqCst) {
                return Err("已停止".into());
            }
            if runtime.agent_library_id()? != library {
                return Err("资料库已切换，这轮对话已停止".into());
            }
            let mut content = json!({
                "callId": call.call_id, "name": call.name, "arguments": call.arguments, "turn": turn,
            });
            if let Some(items) = reasoning.take() {
                content["reasoning"] = items;
            }
            if let Some(usage) = usage.take() {
                content["usage"] = usage;
            }
            push(app, conversation_id, "tool_call", content)?;
            emit(app, conversation_id, "tool-start", json!({"name": call.name}));
            let arguments = serde_json::from_str::<Value>(&call.arguments).unwrap_or_else(|_| json!({}));
            let app_handle = app.clone();
            let id = conversation_id.to_owned();
            let name = call.name.clone();
            let call_id = call.call_id.clone();
            let result = tauri::async_runtime::spawn_blocking(move || tools::run(&app_handle, &id, &call_id, &name, &arguments))
                .await
                .map_err(|_| "工具执行异常中断".to_owned())?;
            let (output, display, ok) = match result {
                Ok(value) => (value.output.to_string(), value.display, true),
                Err(error) => (json!({"error": error}).to_string(), json!({"kind": "error", "message": error}), false),
            };
            push(app, conversation_id, "tool_result", json!({
                "callId": call.call_id, "name": call.name, "output": output, "display": display, "ok": ok,
            }))?;
            emit(app, conversation_id, "tool-end", json!({"name": call.name}));
        }
    }
}
