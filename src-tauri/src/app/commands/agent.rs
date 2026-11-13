//! IPC for the agent window: conversations, sending, undo and preferences.
use serde::Serialize;
use serde_json::Value;
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
        .build()
        .map_err(error_text)?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentState {
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
}

#[tauri::command]
pub(crate) async fn agent_state(app: tauri::AppHandle) -> Result<AgentState, String> {
    let running = app.state::<Engine>().running();
    blocking(app, move |runtime| {
        let (_, _, settings) = runtime.llm_context()?;
        let channels = settings
            .channels
            .iter()
            .map(|channel| ChannelInfo {
                id: channel.id.clone(),
                name: channel.name.clone(),
                model: channel.model.clone(),
                has_key: crate::app::llm::key(channel).is_ok_and(|key| !key.is_empty()),
            })
            .collect();
        Ok(AgentState {
            conversations: runtime.agent_db(false, |db| db.agent_conversations())?,
            running,
            preferences: agent::load_preferences(runtime)?,
            channels,
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn agent_save_preferences(preferences: Preferences, app: tauri::AppHandle) -> Result<(), String> {
    blocking(app, move |runtime| {
        let text = serde_json::to_string(&preferences).map_err(error_text)?;
        runtime.agent_db(false, |db| db.set_setting(PREFERENCES_KEY, &text).map_err(error_text))
    })
    .await
}

#[tauri::command]
pub(crate) async fn agent_models(channel_id: String, app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let channel = blocking(app, move |runtime| {
        let (_, _, settings) = runtime.llm_context()?;
        settings
            .channels
            .into_iter()
            .find(|channel| channel.id == channel_id)
            .ok_or_else(|| "找不到这个渠道".to_owned())
    })
    .await?;
    let key = crate::app::llm::key(&channel)?;
    crate::app::llm::api::models(&channel, &key).await
}

#[tauri::command]
pub(crate) async fn agent_messages(conversation_id: String, app: tauri::AppHandle) -> Result<Vec<Message>, String> {
    blocking(app, move |runtime| runtime.agent_db(false, |db| db.agent_messages(&conversation_id))).await
}

#[tauri::command]
pub(crate) async fn agent_send(
    conversation_id: Option<String>,
    text: String,
    app: tauri::AppHandle,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || agent::send(app, conversation_id, text))
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
pub(crate) async fn agent_operations(limit: Option<usize>, app: tauri::AppHandle) -> Result<Vec<OperationRecord>, String> {
    blocking(app, move |runtime| runtime.agent_db(false, |db| db.agent_operations(limit.unwrap_or(100)))).await
}

#[tauri::command]
pub(crate) async fn agent_undo(operation_id: i64, app: tauri::AppHandle) -> Result<UndoOutcome, String> {
    let handle = app.clone();
    let outcome = blocking(app, move |runtime| runtime.agent_db(true, |db| db.agent_undo(operation_id))).await?;
    let _ = handle.emit("agent://library-changed", ());
    let _ = handle.emit("agent://operations-changed", ());
    Ok(outcome)
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
