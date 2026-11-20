//! Minimal streaming client for the OpenAI-compatible `/v1/responses` endpoint.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{Value, json};

use crate::app::llm::api::{client, normalize_url};

pub struct Request<'a> {
    pub base_url: &'a str,
    pub key: &'a str,
    pub model: &'a str,
    pub effort: &'a str,
    pub instructions: &'a str,
    pub input: Vec<Value>,
    pub has_user_attachments: bool,
    pub tools: Value,
}

pub struct FunctionCall {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

/// Token counts the site reported for one request.
#[derive(Clone, Copy)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

#[derive(Default)]
pub struct Outcome {
    pub text: String,
    pub calls: Vec<FunctionCall>,
    /// Reasoning items some providers (e.g. DeepSeek thinking mode) require to be sent back.
    pub reasoning: Vec<Value>,
    pub usage: Option<Usage>,
    /// The site stopped the reply early (e.g. output token cap).
    pub truncated: bool,
}

/// Why a request failed, and whether sending it again could help.
pub struct Failure {
    pub message: String,
    /// Network trouble or a busy site: the same request may succeed in a moment.
    pub retry: bool,
    /// The site said the request is longer than the model can take.
    pub too_long: bool,
}

impl Failure {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), retry: false, too_long: false }
    }

    fn retry(message: impl Into<String>) -> Self {
        Self { message: message.into(), retry: true, too_long: false }
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

fn parse_usage(value: &Value) -> Option<Usage> {
    let number = |keys: [&str; 2]| keys.iter().find_map(|key| value[*key].as_u64());
    let input = number(["input_tokens", "prompt_tokens"])?;
    let output = number(["output_tokens", "completion_tokens"]).unwrap_or(0);
    (input > 0).then_some(Usage { input, output })
}

fn context_overflow(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "context_length", "context length", "context window", "maximum context", "too many tokens",
        "prompt is too long", "input is too long", "too long for", "token limit", "tokens exceed",
        "exceeds the maximum", "input token count",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// The innermost causes of a connection error, e.g. "connection closed before message completed".
fn network_detail(error: &reqwest::Error) -> String {
    let mut causes = Vec::new();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let text = cause.to_string();
        if !text.is_empty() && !causes.contains(&text) {
            causes.push(text);
        }
        source = cause.source();
    }
    let detail = if causes.is_empty() { error.to_string() } else { causes.join("；") };
    detail.chars().take(300).collect()
}

pub enum Delta<'a> {
    Text(&'a str),
    Reasoning(&'a str),
}

fn friendly_status(status: u16, body: &str, model: &str, has_user_attachments: bool) -> String {
    let lower = body.to_lowercase();
    let unsupported = lower.contains("not support")
        || lower.contains("unsupported")
        || lower.contains("does not exist")
        || lower.contains("no available")
        || lower.contains("not found")
        || lower.contains("invalid model")
        || lower.contains("model_not_found");
    // Some gateways echo request fields in their errors. Never save or emit those for uploads.
    let detail = if has_user_attachments {
        String::new()
    } else {
        format!("\n（站点返回：{}）", body.chars().take(300).collect::<String>())
    };
    let input_rejected = matches!(status, 400 | 415 | 422 | 500..=599);
    if status == 413 {
        return "模型站点拒绝了过大的请求。请减少附件大小，或新建对话以避免发送近期附件。".into();
    }
    if input_rejected && has_user_attachments && (lower.contains("input_file") || lower.contains("file_data") || lower.contains("pdf")) {
        return format!("模型「{model}」或这个站点未能处理 PDF。请检查文件，或换一个支持 PDF 输入的模型。");
    }
    if input_rejected && (lower.contains("image") || lower.contains("vision") || lower.contains("multimodal")) {
        return format!("模型「{model}」未能处理图片。请在右上角换一个支持图片输入的模型，然后重新发送消息。{detail}");
    }
    match status {
        401 => "Key 无效或已失效，请到工具箱「LLM提取」→「渠道设置」里检查这个渠道的 Key。".into(),
        403 => format!("这个 Key 没有权限使用模型「{model}」。请换一个模型或检查 Key 权限。{detail}"),
        404 | 405 => format!(
            "模型「{model}」或这个站点不支持 Responses 接口，Agent 用不了它。请在右上角换一个模型（例如 GPT 系列）。{detail}"
        ),
        429 => "请求太频繁或额度不足，请稍后再试。".into(),
        400 | 422 if unsupported => format!(
            "模型「{model}」不支持 Agent 需要的 Responses 接口或工具调用，请在右上角换一个模型。{detail}"
        ),
        500..=599 if unsupported => format!(
            "模型「{model}」暂时不可用或不支持 Responses 接口，请换一个模型。{detail}"
        ),
        500..=599 => format!("模型站点暂时出错（HTTP {status}），请稍后重试。{detail}"),
        _ => format!("模型请求失败（HTTP {status}）。{detail}"),
    }
}

fn collect_item(item: &Value, outcome: &mut Outcome, text_seen: bool) {
    match item["type"].as_str() {
        Some("function_call") => {
            let call_id = item["call_id"].as_str().unwrap_or_default().to_owned();
            if outcome.calls.iter().any(|call| call.call_id == call_id) {
                return;
            }
            outcome.calls.push(FunctionCall {
                call_id,
                name: item["name"].as_str().unwrap_or_default().to_owned(),
                arguments: item["arguments"].as_str().unwrap_or("{}").to_owned(),
            });
        }
        Some("message") if !text_seen => {
            for part in item["content"].as_array().into_iter().flatten() {
                if let Some(text) = part["text"].as_str() {
                    outcome.text.push_str(text);
                }
            }
        }
        Some("reasoning") => {
            let replayable = item["content"].as_array().is_some_and(|parts| !parts.is_empty())
                || item["encrypted_content"].as_str().is_some_and(|text| !text.is_empty());
            let duplicate = outcome.reasoning.iter().any(|seen| {
                seen == item || (item["id"].is_string() && seen["id"] == item["id"])
            });
            if replayable && !duplicate {
                outcome.reasoning.push(item.clone());
            }
        }
        _ => {}
    }
}

/// Stream one model turn. `on_delta` receives text as it arrives; `cancel` aborts promptly.
pub async fn stream(
    request: Request<'_>,
    cancel: Arc<AtomicBool>,
    mut on_delta: impl FnMut(Delta<'_>),
) -> Result<Outcome, Failure> {
    let mut body = json!({
        "model": request.model,
        "instructions": request.instructions,
        "input": request.input,
        "store": false,
        "stream": true,
    });
    if request.tools.as_array().is_some_and(|tools| !tools.is_empty()) {
        body["tools"] = request.tools;
        body["tool_choice"] = json!("auto");
        body["parallel_tool_calls"] = json!(false);
    }
    if !request.effort.is_empty() && request.effort != "default" {
        body["reasoning"] = json!({"effort": request.effort, "summary": "auto"});
    }
    let response = client()?
        .post(format!("{}/responses", normalize_url(request.base_url)?))
        .bearer_auth(request.key)
        .timeout(Duration::from_secs(900))
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                Failure::new("连接模型站点超时，请检查网络后重试。")
            } else {
                Failure::retry(format!("无法连接模型站点，请检查网络或渠道地址。\n（错误详情：{}）", network_detail(&error)))
            }
        })?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let text = response.text().await.unwrap_or_default();
        let message = friendly_status(status, &text, request.model, request.has_user_attachments);
        let too_long = status == 413 || (matches!(status, 400 | 422 | 500..=599) && context_overflow(&text));
        let retry = !too_long && matches!(status, 408 | 429 | 500 | 502 | 503 | 504 | 520..=524);
        return Err(Failure { message, retry, too_long });
    }
    let is_json = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("application/json"));
    let mut outcome = Outcome::default();
    if is_json {
        let value: Value = response.json().await.map_err(|_| Failure::retry("模型返回的内容无法解析"))?;
        for item in value["output"].as_array().into_iter().flatten() {
            collect_item(item, &mut outcome, false);
        }
        outcome.usage = parse_usage(&value["usage"]);
        if !outcome.text.is_empty() {
            on_delta(Delta::Text(&outcome.text.clone()));
        }
        return Ok(outcome);
    }
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::<u8>::new();
    let mut text_seen = false;
    let mut completed = false;
    let mut raw_reasoning = String::new();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("已停止".into());
        }
        let chunk = match tokio::time::timeout(Duration::from_millis(300), stream.next()).await {
            Err(_) => continue,
            Ok(None) => break,
            Ok(Some(Err(error))) => {
                return Err(Failure::retry(format!("与模型站点的连接中断了，请重试。\n（错误详情：{}）", network_detail(&error))));
            }
            Ok(Some(Ok(chunk))) => chunk,
        };
        buffer.extend_from_slice(&chunk);
        while let Some(end) = find_event_end(&buffer) {
            let event = buffer.drain(..end).collect::<Vec<_>>();
            let event = String::from_utf8_lossy(&event);
            let data = event
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(str::trim_start)
                .collect::<Vec<_>>()
                .join("\n");
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&data) else { continue };
            match value["type"].as_str().unwrap_or_default() {
                "response.output_text.delta" => {
                    if let Some(delta) = value["delta"].as_str() {
                        text_seen = true;
                        outcome.text.push_str(delta);
                        on_delta(Delta::Text(delta));
                    }
                }
                "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                    if let Some(delta) = value["delta"].as_str() {
                        if value["type"] == "response.reasoning_text.delta" {
                            raw_reasoning.push_str(delta);
                        }
                        on_delta(Delta::Reasoning(delta));
                    }
                }
                "response.output_item.done" => collect_item(&value["item"], &mut outcome, text_seen),
                "response.completed" | "response.incomplete" => {
                    let has_text = text_seen || !outcome.text.is_empty();
                    for item in value["response"]["output"].as_array().into_iter().flatten() {
                        collect_item(item, &mut outcome, has_text);
                    }
                    if value["type"] == "response.incomplete" && outcome.calls.is_empty() {
                        let reason = value["response"]["incomplete_details"]["reason"].as_str().unwrap_or("未知原因");
                        outcome.text.push_str(&format!("\n\n（模型输出被截断：{reason}）"));
                        outcome.truncated = true;
                    }
                    outcome.usage = parse_usage(&value["response"]["usage"]);
                    completed = true;
                }
                "response.failed" | "error" => {
                    let message = value["response"]["error"]["message"]
                        .as_str()
                        .or(value["error"]["message"].as_str())
                        .or(value["message"].as_str())
                        .unwrap_or("未知错误");
                    let lower = message.to_lowercase();
                    let busy = ["overloaded", "rate limit", "temporarily", "try again", "unavailable", "timeout"]
                        .iter()
                        .any(|needle| lower.contains(needle));
                    let too_long = context_overflow(message);
                    return Err(Failure {
                        message: friendly_status(500, message, request.model, request.has_user_attachments)
                            .replace("模型站点暂时出错（HTTP 500）", "模型返回了错误"),
                        retry: busy && !too_long,
                        too_long,
                    });
                }
                _ => {}
            }
        }
    }
    if !completed && outcome.text.is_empty() && outcome.calls.is_empty() {
        return Err(Failure::retry(format!(
            "模型「{}」没有返回任何内容。它可能不支持 Responses 接口，请换一个模型再试。",
            request.model
        )));
    }
    if outcome.reasoning.is_empty() && !raw_reasoning.trim().is_empty() {
        outcome.reasoning.push(json!({
            "type": "reasoning",
            "summary": [],
            "content": [{"type": "reasoning_text", "text": raw_reasoning}],
        }));
    }
    Ok(outcome)
}

fn find_event_end(buffer: &[u8]) -> Option<usize> {
    let lf = buffer.windows(2).position(|window| window == b"\n\n").map(|index| index + 2);
    let crlf = buffer.windows(4).position(|window| window == b"\r\n\r\n").map(|index| index + 4);
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}
