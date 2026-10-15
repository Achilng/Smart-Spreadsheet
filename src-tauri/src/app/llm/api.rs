use crate::db::llm::{Channel, Item, Job};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::time::Duration;

pub struct Failure {
    pub message: String,
    pub status: u16,
    pub retry_after: u64,
}
fn failure(message: &str) -> Failure {
    Failure {
        message: message.into(),
        status: 0,
        retry_after: 0,
    }
}
pub fn normalize_url(value: &str) -> Result<String, String> {
    let value = value.trim().trim_end_matches('/');
    let url = reqwest::Url::parse(value).map_err(|_| "Base URL 格式无效")?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("请填写不含账号、参数或片段的 HTTP / HTTPS 地址".into());
    }
    if value.ends_with("/chat/completions") {
        return Err("Base URL 请填写到 /v1，不要附加 /chat/completions".into());
    }
    Ok(value.into())
}
pub fn client() -> Result<reqwest::Client, String> {
    static CLIENT: std::sync::OnceLock<Result<reqwest::Client, String>> =
        std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            // Use the same installed crypto implementation as the updater; no system/toolchain change.
            let _ = rustls::crypto::ring::default_provider().install_default();
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(20))
                .timeout(Duration::from_secs(300))
                .build()
                .map_err(|_| "无法创建网络客户端".into())
        })
        .clone()
}
pub async fn models(channel: &Channel, key: &str) -> Result<Vec<String>, String> {
    let reply = client()?
        .get(format!("{}/models", normalize_url(&channel.base_url)?))
        .bearer_auth(key)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| "获取模型列表失败，请检查地址和网络")?;
    if !reply.status().is_success() {
        return Err(format!(
            "获取模型列表失败：HTTP {}",
            reply.status().as_u16()
        ));
    }
    let body: Value = reply.json().await.map_err(|_| "模型列表不是有效 JSON")?;
    let mut values = body["data"]
        .as_array()
        .ok_or("接口未返回模型列表")?
        .iter()
        .filter_map(|v| v["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    Ok(values)
}
pub async fn extract(
    channel: Channel,
    key: String,
    job: Job,
    items: Vec<Item>,
    preview: impl Fn(String) + Send,
) -> Result<Vec<Result<String, String>>, Failure> {
    let inputs = items
        .iter()
        .enumerate()
        .map(|(i, item)| json!({"id":(i+1).to_string(),"positive_prompt":item.prompt}))
        .collect::<Vec<_>>();
    let ids = (1..=items.len()).map(|i| i.to_string()).collect::<Vec<_>>();
    let mut body = json!({"model":channel.model,"stream":true,"reasoning_effort":job.reasoning,"messages":[
        {"role":"system","content":format!("{}\n只返回 JSON：{{\"items\":[{{\"id\":\"1\",\"status\":\"ok 或 none\",\"artist_string\":\"原文片段\"}}]}}。每条输入恰好对应一条结果，无画风时 status=none 且 artist_string 为空字符串。",include_str!("../../../../tools/style-extractor/prompt.txt"))},
        {"role":"user","content":json!({"items":inputs}).to_string()}],
        "response_format":{"type":"json_schema","json_schema":{"name":"style_extraction","strict":true,"schema":{"type":"object","properties":{"items":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string","enum":ids},"status":{"type":"string","enum":["ok","none"]},"artist_string":{"type":"string"}},"required":["id","status","artist_string"],"additionalProperties":false}}},"required":["items"],"additionalProperties":false}}}});
    let client = client().map_err(|e| failure(&e))?;
    let mut content = String::new();
    for _ in 0..4 {
        let response = client
            .post(format!("{}/chat/completions", channel.base_url))
            .bearer_auth(&key)
            .json(&body)
            .send()
            .await
            .map_err(|_| failure("接口连接失败或超时"))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(5);
            let text = response.text().await.unwrap_or_default().to_lowercase();
            if [400, 422].contains(&status) {
                let mut changed = false;
                for (field, aliases) in [
                    (
                        "response_format",
                        vec!["response_format", "json_schema", "structured"],
                    ),
                    (
                        "reasoning_effort",
                        vec!["reasoning_effort", "reasoning effort"],
                    ),
                    ("stream", vec!["stream"]),
                ] {
                    if aliases.iter().any(|s| text.contains(s)) && body.get(field).is_some() {
                        body.as_object_mut().unwrap().remove(field);
                        changed = true;
                    }
                }
                if changed {
                    continue;
                }
            }
            let quota = text.contains("insufficient_quota") || text.contains("quota exceeded");
            return Err(Failure {
                message: if quota {
                    "接口额度不足，请检查渠道".into()
                } else {
                    format!("接口返回 HTTP {status}，请检查渠道配置或稍后重试")
                },
                status: if quota { 403 } else { status },
                retry_after,
            });
        }
        let is_stream = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|s| s.contains("text/event-stream"));
        if is_stream {
            let mut stream = response.bytes_stream();
            let mut buffer = Vec::<u8>::new();
            while let Some(chunk) = stream.next().await {
                buffer.extend_from_slice(
                    &chunk.map_err(|_| failure("接收结果中断，未完成项将重试"))?,
                );
                while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                    let line = String::from_utf8(buffer.drain(..=end).collect())
                        .map_err(|_| failure("接口返回无效文本编码"))?;
                    if let Some(data) = line.trim().strip_prefix("data:") {
                        if data.trim() == "[DONE]" {
                            continue;
                        }
                        let event: Value = serde_json::from_str(data.trim())
                            .map_err(|_| failure("接口流式数据格式无效"))?;
                        if event.get("error").is_some() {
                            return Err(failure("接口流式响应包含错误"));
                        }
                        if let Some(delta) = event["choices"][0]["delta"]["content"].as_str() {
                            content.push_str(delta);
                            preview(
                                content
                                    .chars()
                                    .rev()
                                    .take(500)
                                    .collect::<String>()
                                    .chars()
                                    .rev()
                                    .collect(),
                            );
                        }
                    }
                }
            }
        } else {
            let response: Value = response
                .json()
                .await
                .map_err(|_| failure("接口未返回有效 JSON"))?;
            content = response["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| failure("接口响应缺少文本结果"))?
                .into();
        }
        break;
    }
    let trimmed = content.trim();
    let trimmed = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim()
        .trim_end_matches("```")
        .trim();
    let reply: Value =
        serde_json::from_str(trimmed).map_err(|_| failure("模型返回的 JSON 不完整或格式错误"))?;
    let replies = reply["items"]
        .as_array()
        .ok_or_else(|| failure("模型结果缺少 items 数组"))?;
    Ok(items
        .iter()
        .enumerate()
        .map(|(index, input)| {
            let id = (index + 1).to_string();
            let found = replies
                .iter()
                .filter(|v| {
                    v["id"].as_str() == Some(&id) || v["id"].as_u64() == Some((index + 1) as u64)
                })
                .collect::<Vec<_>>();
            if found.len() != 1 {
                return Err("编号缺失或重复，仅重试本条".into());
            }
            let entry = found[0];
            let text = entry["artist_string"].as_str();
            match (entry["status"].as_str(), text) {
                (Some("none"), Some("")) => Ok(String::new()),
                (Some("ok"), Some(text)) if !text.is_empty() && input.prompt.contains(text) => {
                    Ok(text.into())
                }
                _ => Err("结果状态无效或不是连续原文".into()),
            }
        })
        .collect())
}
