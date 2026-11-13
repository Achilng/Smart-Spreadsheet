//! Conversational library agent: model loop, cancellation and main-window state.
mod prompt;
mod responses;
pub(crate) mod tools;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::app::AppRuntime;
use crate::db::RowSelection;
use crate::db::agent::MainView;

pub const PREFERENCES_KEY: &str = "agent_preferences_v1";
const MAX_STEPS: usize = 30;
const OLD_OUTPUT_LIMIT: usize = 1500;
const RECENT_FULL_OUTPUTS: usize = 6;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_effort")]
    pub effort: String,
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
    runs: Mutex<HashMap<String, Arc<AtomicBool>>>,
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

pub fn load_preferences(runtime: &AppRuntime) -> Result<Preferences, String> {
    runtime.agent_db(false, |db| {
        let saved = db.setting(PREFERENCES_KEY).map_err(|e| e.to_string())?;
        Ok(saved
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| Preferences { effort: default_effort(), ..Default::default() }))
    })
}

fn emit(app: &AppHandle, conversation_id: &str, kind: &str, extra: Value) {
    let mut payload = json!({"conversationId": conversation_id, "kind": kind});
    if let (Some(target), Value::Object(extra)) = (payload.as_object_mut(), extra) {
        target.extend(extra);
    }
    let _ = app.emit("agent://event", payload);
}

fn push(app: &AppHandle, conversation_id: &str, kind: &str, content: Value) -> Result<i64, String> {
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
pub fn send(app: AppHandle, conversation_id: Option<String>, text: String) -> Result<String, String> {
    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err("消息不能为空".into());
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
            let title = text.chars().take(24).collect::<String>();
            runtime.agent_db(false, |db| db.agent_create_conversation(&id, &title))?;
            id
        }
    };
    let engine = app.state::<Engine>();
    let cancel = engine.begin(&conversation_id)?;
    if let Err(error) = push(&app, &conversation_id, "user", json!({"text": text})) {
        engine.finish(&conversation_id);
        return Err(error);
    }
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

fn build_input(messages: &[crate::db::agent::Message]) -> Vec<Value> {
    let answered = messages
        .iter()
        .filter(|message| message.kind == "tool_result")
        .filter_map(|message| message.content["callId"].as_str().map(str::to_owned))
        .collect::<std::collections::HashSet<_>>();
    let result_count = messages.iter().filter(|message| message.kind == "tool_result").count();
    let mut seen_results = 0;
    let mut input = Vec::new();
    let tool_output = |content: &Value, seen_results: &mut usize| {
        *seen_results += 1;
        let mut output = content["output"].as_str().unwrap_or_default().to_owned();
        if *seen_results + RECENT_FULL_OUTPUTS <= result_count && output.chars().count() > OLD_OUTPUT_LIMIT {
            output = output.chars().take(OLD_OUTPUT_LIMIT).collect::<String>() + "…（较早的结果已截断，需要时请重新查询）";
        }
        json!({"type": "function_call_output", "call_id": content["callId"], "output": output})
    };
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        let content = &message.content;
        index += 1;
        match message.kind.as_str() {
            "user" => input.push(json!({"role": "user", "content": content["text"]})),
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
                for call in &batch {
                    let call_id = call.content["callId"].as_str().unwrap_or_default();
                    if let Some(result) = results.iter().find(|result| result.content["callId"] == call.content["callId"]) {
                        input.push(tool_output(&result.content, &mut seen_results));
                    } else if !answered.contains(call_id) {
                        input.push(json!({"type": "function_call_output", "call_id": call_id,
                            "output": "已被用户中断，未执行"}));
                    }
                }
            }
            "tool_result" => input.push(tool_output(content, &mut seen_results)),
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

async fn run_loop(app: &AppHandle, conversation_id: &str, cancel: Arc<AtomicBool>) -> Result<(), String> {
    let runtime = app.state::<AppRuntime>();
    let preferences = load_preferences(&runtime)?;
    let (_, _, settings) = runtime.llm_context()?;
    let channel = settings
        .channels
        .iter()
        .find(|channel| channel.id == preferences.channel_id)
        .or_else(|| settings.channels.first())
        .ok_or("还没有可用的模型渠道。请先在工具箱的「模型设置」里添加渠道和 Key。")?
        .clone();
    let model = if preferences.model.trim().is_empty() { channel.model.clone() } else { preferences.model.clone() };
    if model.trim().is_empty() {
        return Err("还没有选择模型，请在右上角选择一个模型。".into());
    }
    let key = crate::app::llm::key(&channel)?;
    if key.is_empty() {
        return Err(format!("渠道「{}」还没有填写 Key。", channel.name));
    }
    let tools = tools::definitions();
    let library = runtime.agent_library_id()?;
    for _ in 0..MAX_STEPS {
        if cancel.load(Ordering::SeqCst) {
            return Err("已停止".into());
        }
        if runtime.agent_library_id()? != library {
            return Err("资料库已切换，这轮对话已停止".into());
        }
        let messages = runtime.agent_db(false, |db| db.agent_messages(conversation_id))?;
        let request = responses::Request {
            base_url: &channel.base_url,
            key: &key,
            model: &model,
            effort: &preferences.effort,
            instructions: prompt::SYSTEM_PROMPT,
            input: build_input(&messages),
            tools: tools.clone(),
        };
        let outcome = responses::stream(request, cancel.clone(), |delta| match delta {
            responses::Delta::Text(text) => emit(app, conversation_id, "text-delta", json!({"delta": text})),
            responses::Delta::Reasoning(text) => emit(app, conversation_id, "reasoning-delta", json!({"delta": text})),
        })
        .await?;
        // Reasoning precedes this turn's output, so it rides on the first stored message.
        let mut reasoning = (!outcome.reasoning.is_empty()).then(|| Value::Array(outcome.reasoning));
        if !outcome.text.trim().is_empty() {
            let mut content = json!({"text": outcome.text});
            if let Some(items) = reasoning.take() {
                content["reasoning"] = items;
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
            push(app, conversation_id, "tool_call", content)?;
            emit(app, conversation_id, "tool-start", json!({"name": call.name}));
            let arguments = serde_json::from_str::<Value>(&call.arguments).unwrap_or_else(|_| json!({}));
            let app_handle = app.clone();
            let id = conversation_id.to_owned();
            let name = call.name.clone();
            let result = tauri::async_runtime::spawn_blocking(move || tools::run(&app_handle, &id, &name, &arguments))
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
    push(app, conversation_id, "assistant", json!({"text": "（这一轮步骤太多，我先停下来了。需要的话请告诉我继续。）"}))?;
    Ok(())
}
