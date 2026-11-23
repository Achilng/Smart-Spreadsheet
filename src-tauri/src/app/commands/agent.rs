//! IPC for the agent window: conversations, sending, undo and preferences.
use std::hash::{BuildHasher, RandomState};
use std::sync::OnceLock;

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use super::error_text;
use crate::app::AppRuntime;
use crate::app::agent::{self, Engine, PREFERENCES_KEY, Preferences};
use crate::db::RowSelection;
use crate::db::agent::{Conversation, Message, OperationRecord, RowChange, UndoOutcome};

async fn blocking<T: Send + 'static>(
    app: tauri::AppHandle,
    f: impl FnOnce(&AppRuntime) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppRuntime>()))
        .await
        .map_err(|_| "后台任务异常中断".to_owned())?
}

#[tauri::command]
pub(crate) async fn open_agent_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("agent") {
        window.show().map_err(error_text)?;
        window.unminimize().map_err(error_text)?;
        window.set_focus().map_err(error_text)?;
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "agent", WebviewUrl::App("index.html?window=agent".into()))
        .title("整理助手")
        .inner_size(1120.0, 760.0)
        .min_inner_size(760.0, 520.0)
        .center()
        .resizable(true)
        .decorations(false)
        .skip_taskbar(false)
        .disable_drag_drop_handler()
        .build()
        .map_err(error_text)?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentState {
    library_id: String,
    conversations: Vec<Conversation>,
    running: Vec<String>,
    preferences: Preferences,
    channels: Vec<ChannelInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    id: String,
    name: String,
    model: String,
    has_key: bool,
    catalog_revision: String,
}

fn catalog_revision(channel: &crate::db::llm::Channel) -> String {
    static HASHER: OnceLock<RandomState> = OnceLock::new();
    let revision = HASHER.get_or_init(RandomState::new).hash_one((
        &channel.base_url, &channel.key, &channel.encrypted_key,
    ));
    format!("{revision:x}")
}

#[tauri::command]
pub(crate) async fn agent_state(app: tauri::AppHandle) -> Result<AgentState, String> {
    let running = app.state::<Engine>().running();
    blocking(app, move |runtime| {
        let (_, library_id, settings) = runtime.llm_context()?;
        let channels = settings
            .channels
            .iter()
            .map(|channel| ChannelInfo {
                id: channel.id.clone(),
                name: channel.name.clone(),
                model: channel.model.clone(),
                has_key: crate::app::llm::key(channel).is_ok_and(|key| !key.is_empty()),
                catalog_revision: catalog_revision(channel),
            })
            .collect();
        runtime.llm_db(&library_id, |db| {
            Ok(AgentState {
                library_id: library_id.clone(),
                conversations: db.agent_conversations().map_err(crate::db::llm::invalid)?,
                running,
                preferences: agent::read_preferences(db)?,
                channels,
            })
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn agent_save_preferences(library_id: String, preferences: Preferences, app: tauri::AppHandle) -> Result<(), String> {
    blocking(app, move |runtime| {
        let text = serde_json::to_string(&preferences).map_err(error_text)?;
        runtime.llm_db(&library_id, |db| {
            if !db.llm_settings()?.channels.iter().any(|channel| channel.id == preferences.channel_id) {
                return Err(crate::db::llm::invalid("找不到这个模型渠道，请重新打开设置。"));
            }
            db.set_setting(PREFERENCES_KEY, &text)
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn agent_models(library_id: String, channel_id: String, revision: String, app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let channel = blocking(app, move |runtime| {
        let (_, current_library_id, settings) = runtime.llm_context()?;
        if current_library_id != library_id {
            return Err("资料库已切换".into());
        }
        settings
            .channels
            .into_iter()
            .find(|channel| channel.id == channel_id && catalog_revision(channel) == revision)
            .ok_or_else(|| "模型渠道已更改，请重新打开设置。".to_owned())
    })
    .await?;
    let key = crate::app::llm::key(&channel)?;
    if key.is_empty() {
        return Err("这个渠道还没有填写 Key。".into());
    }
    crate::app::llm::api::models(&channel, &key).await
}

#[tauri::command]
pub(crate) async fn agent_messages(conversation_id: String, app: tauri::AppHandle) -> Result<Vec<Message>, String> {
    blocking(app, move |runtime| runtime.agent_db(false, |db| db.agent_messages(&conversation_id))).await
}

#[tauri::command]
pub(crate) async fn agent_send(
    conversation_id: Option<String>,
    text: Option<String>,
    attachments: Option<Vec<agent::AttachmentInput>>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        agent::send(app, conversation_id, text.unwrap_or_default(), attachments.unwrap_or_default())
    })
    .await
    .map_err(|_| "后台任务异常中断".to_owned())?
}

#[tauri::command]
pub(crate) fn agent_cancel(conversation_id: String, app: tauri::AppHandle) {
    app.state::<Engine>().cancel(&conversation_id);
}

#[tauri::command]
pub(crate) async fn agent_rename_conversation(id: String, title: String, app: tauri::AppHandle) -> Result<(), String> {
    blocking(app, move |runtime| runtime.agent_db(false, |db| db.agent_rename_conversation(&id, &title))).await
}

#[tauri::command]
pub(crate) async fn agent_delete_conversation(id: String, app: tauri::AppHandle) -> Result<(), String> {
    if app.state::<Engine>().running().contains(&id) {
        return Err("这个对话还在运行中，请先停止".into());
    }
    blocking(app, move |runtime| runtime.agent_db(false, |db| db.agent_delete_conversation(&id))).await
}

#[tauri::command]
pub(crate) async fn agent_dismiss_error(conversation_id: String, message_id: i64, app: tauri::AppHandle) -> Result<(), String> {
    blocking(app, move |runtime| runtime.agent_db(false, |db| db.agent_dismiss_error(&conversation_id, message_id))).await
}

/// A preview of an image the user attached: small for the chat bubble, large for the viewer.
#[tauri::command]
pub(crate) async fn agent_attachment_preview(conversation_id: String, message_id: i64, number: u8, large: bool, app: tauri::AppHandle) -> Result<String, String> {
    let side = if large { 2000 } else { 360 };
    blocking(app, move |runtime| agent::vision::attachment_preview(runtime, &conversation_id, message_id, number, side)).await
}

/// The user finished (or failed) an export from a card the assistant prepared.
#[tauri::command]
pub(crate) fn agent_note_export(conversation_id: String, export_id: String, title: String, result: String, app: tauri::AppHandle) {
    note_user_action(
        &app,
        Some(&conversation_id),
        format!("（我在导出卡片「{title}」上完成了导出：{result}）"),
        json!({"action": "export", "exportId": export_id, "title": title, "result": result}),
    );
}

#[tauri::command]
pub(crate) async fn agent_operations(limit: Option<usize>, app: tauri::AppHandle) -> Result<Vec<OperationRecord>, String> {
    blocking(app, move |runtime| runtime.agent_db(false, |db| db.agent_operations(limit.unwrap_or(100)))).await
}

/// Tell the conversation what the user did on a card, unless a model turn is in progress there.
fn note_user_action(app: &tauri::AppHandle, conversation_id: Option<&str>, text: String, extra: Value) {
    let Some(id) = conversation_id else { return };
    if app.state::<Engine>().is_running(id) {
        return;
    }
    let mut content = json!({"text": text});
    if let (Some(target), Value::Object(extra)) = (content.as_object_mut(), extra) {
        target.extend(extra);
    }
    let _ = agent::push(app, id, "user_action", content);
}

#[tauri::command]
pub(crate) async fn agent_undo(operation_id: i64, app: tauri::AppHandle) -> Result<UndoOutcome, String> {
    let handle = app.clone();
    let (outcome, operation) = blocking(app, move |runtime| {
        runtime.agent_db(true, |db| Ok((db.agent_undo(operation_id)?, db.agent_operation(operation_id)?)))
    })
    .await?;
    let _ = handle.emit("agent://library-changed", ());
    let _ = handle.emit("agent://operations-changed", ());
    if operation.as_ref().is_some_and(|operation| operation.kind.starts_with("rule")) {
        let _ = handle.emit("automation-rules://changed", ());
    }
    if let Some(operation) = operation {
        note_user_action(
            &handle,
            operation.conversation_id.as_deref(),
            format!("（我在卡片上撤销了操作 #{}「{}」：{}）", operation.id, operation.title, outcome.message),
            json!({"action": "undo", "operationId": operation.id, "title": operation.title}),
        );
    }
    Ok(outcome)
}

/// Run a preview from its card, without going through the model.
#[tauri::command]
pub(crate) async fn agent_apply_preview(conversation_id: String, preview_id: String, app: tauri::AppHandle) -> Result<OperationRecord, String> {
    if app.state::<Engine>().is_running(&conversation_id) {
        return Err("助手还在处理这个对话，请等它停下来再执行".into());
    }
    let handle = app.clone();
    let id = conversation_id.clone();
    let record = blocking(app, move |runtime| runtime.agent_db(true, |db| db.agent_apply(&id, &preview_id))).await?;
    let _ = handle.emit("agent://library-changed", ());
    let _ = handle.emit("agent://operations-changed", ());
    let rows = if record.changed_rows > 0 { format!("，影响了 {} 张", record.changed_rows) } else { String::new() };
    note_user_action(
        &handle,
        Some(&conversation_id),
        format!("（我在预演卡片上点了「确认执行」：「{}」已执行{rows}，操作记录 #{}）", record.title, record.id),
        json!({"action": "apply", "operationId": record.id, "title": record.title}),
    );
    Ok(record)
}

/// Show the rows a preview or operation changes (optionally one change group) in the main window.
#[tauri::command]
pub(crate) async fn agent_show_changed_rows(source: String, id: String, group: Option<usize>, app: tauri::AppHandle) -> Result<usize, String> {
    let handle = app.clone();
    let ids = blocking(app, move |runtime| runtime.agent_db(false, |db| db.agent_changed_rows(&source, &id, group))).await?;
    handle
        .emit_to("main", "agent://main-selection", json!({"rowIds": ids, "showOnly": true}))
        .map_err(error_text)?;
    if let Some(window) = handle.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    Ok(ids.len())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePage {
    total: usize,
    changes: Vec<RowChange>,
}

#[tauri::command]
pub(crate) async fn agent_change_page(
    source: String,
    id: String,
    offset: usize,
    limit: usize,
    app: tauri::AppHandle,
) -> Result<ChangePage, String> {
    blocking(app, move |runtime| {
        let (total, changes) = runtime.agent_db(false, |db| db.agent_change_page(&source, &id, offset, limit))?;
        Ok(ChangePage { total, changes })
    })
    .await
}

#[tauri::command]
pub(crate) fn agent_report_main_view(
    selection: Option<RowSelection>,
    filtered: Option<RowSelection>,
    info: Value,
    app: tauri::AppHandle,
) {
    app.state::<Engine>().report_main_view(selection, filtered, info);
}

#[tauri::command]
pub(crate) fn agent_report_compare_view(info: Value, app: tauri::AppHandle) {
    app.state::<Engine>().report_compare_view(info);
}
