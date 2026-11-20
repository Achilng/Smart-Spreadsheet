//! Conversation compaction, following pi's coding agent (badlogic/pi-mono, core/compaction).
//!
//! When a request would come close to the model's context window, the older part of the
//! conversation is summarized by the model into a structured checkpoint. The checkpoint is stored
//! as a `compaction` message; from then on requests carry the summary plus the newest messages
//! verbatim (about `KEEP_RECENT_TOKENS`). The window still shows the whole history.
//!
//! A cut never lands on a tool result, so calls stay next to their results. When the cut falls
//! inside a turn (one user request with many tool calls, the usual case here), the earlier part of
//! that turn gets its own short summary, as in pi's "split turn".
use serde_json::{Value, json};

use super::{estimate_text, estimate_value};
use crate::db::agent::Message;

/// Recent conversation kept word for word after compaction.
pub const KEEP_RECENT_TOKENS: u64 = 20_000;
/// Tool results are cut to this length in the text the summarizer reads, shorter if that is still too long.
const TOOL_RESULT_CHARS: [usize; 3] = [2000, 800, 300];

pub const SYSTEM_PROMPT: &str = "你是对话摘要助手。你的任务是阅读用户和图库整理助手之间的一段对话，按指定格式写出结构化摘要。

不要继续这段对话，不要回答对话里的任何问题，只输出结构化摘要。";

const FORMAT: &str = "## 目标
[用户想完成什么？一段对话里有多个任务可以列多条。]

## 约束和偏好
- [用户提出的限制、偏好和要求，例如范围、要不要改角色提示词、哪些图不要动]
- [没有就写「（无）」]

## 进展
### 已完成
- [x] [已完成的查询和修改，写明 previewId / operationId 和影响张数]

### 进行中
- [ ] [正在做的事]

### 受阻
- [卡住的问题，没有就写「（无）」]

## 关键决定
- **[决定]**：[简短理由]

## 下一步
1. [按顺序列出接下来要做的事]

## 关键上下文
- [继续工作需要的数据：图片编号、画师串、TAG 名、分组名、规则 id、还在等用户确认的 previewId、用户的原话要求等]
- [没有就写「（无）」]

每一节都要简洁。图片编号、TAG 名、previewId、operationId、提示词原文和报错信息要原样保留。用简体中文。";

const UPDATE_RULES: &str = "上面 <conversation> 里是新的对话消息，请把它们合并进 <previous-summary> 里已有的摘要。规则：
- 保留旧摘要里的所有信息
- 加入新消息里的进展、决定和上下文
- 更新「进展」：完成了的事从「进行中」移到「已完成」
- 根据已完成的工作更新「下一步」
- 图片编号、TAG 名、previewId、operationId、提示词原文和报错信息要原样保留
- 已经不再相关的内容可以删掉

严格使用以下格式：

";

const TURN_PREFIX_PROMPT: &str = "上面是一段进行中对话里较早的消息（同一轮的前半部分）。后面的消息另外保存，不需要复述。

请为用户这一轮的请求和上面已经取得的进展写一份简洁的检查点，它会放在后面那些消息之前，让工作能接着进行。

## 原始请求
[用户这一轮要求做什么？]

## 目前进展
- [这些消息里的关键决定和已完成的工作]

## 继续所需的上下文
- [理解后面工作需要的信息，例如查到的图片编号、TAG、previewId]

只总结上面明确出现的信息，不要推测或补写后面的消息。用简体中文。";

/// What a request is built from: the latest summary (if any) and the messages after its cut.
pub struct View {
    pub summary: Option<String>,
    pub messages: Vec<Message>,
}

pub fn view(all: &[Message]) -> View {
    let Some(index) = all.iter().rposition(|message| message.kind == "compaction") else {
        return View { summary: None, messages: all.to_vec() };
    };
    let compaction = &all[index];
    let first_kept = compaction.content["firstKeptId"].as_i64().unwrap_or(compaction.id + 1);
    View {
        summary: compaction.content["summary"].as_str().map(str::to_owned),
        messages: all.iter().filter(|message| message.kind != "compaction" && message.id >= first_kept).cloned().collect(),
    }
}

/// The summary as it is sent at the start of every later request.
pub fn summary_message(summary: &str) -> Value {
    json!({"role": "user", "content": format!(
        "（系统消息，不是用户说的话）前面的对话太长，已经整理成下面这份摘要；之后的消息照原样保留。需要摘要里没有的细节时，请重新用工具查询。\n\n<summary>\n{summary}\n</summary>"
    )})
}

fn tokens(message: &Message) -> u64 {
    let content = &message.content;
    let text = |key: &str| content[key].as_str().map_or(0, estimate_text);
    match message.kind.as_str() {
        "user" => text("text") + estimate_value(&content["attachments"]),
        "user_action" => text("text"),
        "assistant" => text("text") + estimate_value(&content["reasoning"]),
        "tool_call" => text("name") + text("arguments") + estimate_value(&content["reasoning"]) + 10,
        "tool_result" => text("output") + 10,
        "error" => 10,
        _ => 0,
    }
}

/// Where a kept part may begin: a user message, a reply, or the first call of a batch.
fn is_cut_point(message: &Message) -> bool {
    match message.kind.as_str() {
        "user" | "user_action" | "assistant" => true,
        "tool_call" => message.content["turn"].as_str().is_none_or(|turn| message.content["callId"].as_str() == Some(turn)),
        _ => false,
    }
}

fn is_turn_start(message: &Message) -> bool {
    matches!(message.kind.as_str(), "user" | "user_action")
}

pub struct Plan {
    /// First message kept word for word.
    pub first_kept: i64,
    /// Whole turns before the cut, merged into the previous summary.
    pub history: Vec<Message>,
    /// The start of the turn the cut falls in, summarized on its own.
    pub prefix: Vec<Message>,
    pub previous: Option<String>,
}

/// Find the cut that keeps about `keep` tokens of recent conversation (pi's `findCutPoint`).
pub fn prepare(all: &[Message], keep: u64) -> Option<Plan> {
    let View { summary, messages } = view(all);
    let cut_points = messages.iter().enumerate().filter(|(_, message)| is_cut_point(message)).map(|(index, _)| index).collect::<Vec<_>>();
    let mut kept = 0;
    let mut cut = None;
    for index in (0..messages.len()).rev() {
        kept += tokens(&messages[index]);
        if kept >= keep {
            // The closest cut point at or after this message; if the newest results alone exceed
            // the budget, keep from their call rather than from the start.
            cut = cut_points.iter().copied().find(|&point| point >= index).or(cut_points.last().copied());
            break;
        }
    }
    let cut = cut.filter(|&cut| cut > 0)?;
    let turn_start = if is_turn_start(&messages[cut]) { None } else { (0..cut).rev().find(|&index| is_turn_start(&messages[index])) };
    let history = messages[..turn_start.unwrap_or(cut)].to_vec();
    let prefix = turn_start.map(|start| messages[start..cut].to_vec()).unwrap_or_default();
    if history.is_empty() && prefix.is_empty() {
        return None;
    }
    Some(Plan { first_kept: messages[cut].id, history, prefix, previous: summary })
}

fn clip(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_owned();
    }
    format!("{}\n\n[…另有 {} 字已省略]", text.chars().take(limit).collect::<String>(), count - limit)
}

/// The conversation as plain text, so the summarizer reads it instead of continuing it.
/// Reasoning is left out, as in pi: the visible text, calls and results record what happened.
fn serialize(messages: &[Message], result_chars: usize) -> String {
    let mut parts = Vec::new();
    for message in messages {
        let content = &message.content;
        let text = content["text"].as_str().unwrap_or_default();
        match message.kind.as_str() {
            "user" => {
                let files = content["attachments"].as_array().into_iter().flatten()
                    .filter_map(|file| file["name"].as_str()).collect::<Vec<_>>();
                let files = if files.is_empty() { String::new() } else { format!("（附件：{}）", files.join("、")) };
                if !text.is_empty() || !files.is_empty() {
                    parts.push(format!("[用户]: {text}{files}"));
                }
            }
            "user_action" if !text.is_empty() => parts.push(format!("[用户在界面上操作]: {text}")),
            "assistant" if !text.trim().is_empty() => parts.push(format!("[助手]: {text}")),
            "tool_call" => parts.push(format!(
                "[助手调用工具]: {}({})",
                content["name"].as_str().unwrap_or_default(),
                content["arguments"].as_str().unwrap_or_default(),
            )),
            "tool_result" => {
                let output = content["output"].as_str().unwrap_or_default();
                if !output.is_empty() {
                    parts.push(format!("[工具结果]: {}", clip(output, result_chars)));
                }
            }
            "error" if content["stopped"] == true => parts.push("[用户]: （中途停止了上一轮回复）".into()),
            "error" => parts.push(format!("[出错]: {}", content["message"].as_str().unwrap_or_default())),
            _ => {}
        }
    }
    parts.join("\n\n")
}

/// The longest serialization that fits in `limit` tokens (tool results cut shorter if needed).
fn fitted(messages: &[Message], limit: u64) -> String {
    let mut text = String::new();
    for chars in TOOL_RESULT_CHARS {
        text = serialize(messages, chars);
        if estimate_text(&text) <= limit {
            break;
        }
    }
    text
}

/// Prompt that summarizes (or updates the summary with) whole turns; None when there are none.
pub fn history_prompt(plan: &Plan, limit: u64) -> Option<String> {
    if plan.history.is_empty() {
        return None;
    }
    let mut prompt = format!("<conversation>\n{}\n</conversation>\n\n", fitted(&plan.history, limit));
    match &plan.previous {
        Some(previous) => {
            prompt.push_str(&format!("<previous-summary>\n{previous}\n</previous-summary>\n\n"));
            prompt.push_str(UPDATE_RULES);
        }
        None => prompt.push_str("上面 <conversation> 里是要整理的对话。请写一份结构化的上下文检查点摘要，另一个模型会靠它接着完成工作。\n\n严格使用以下格式：\n\n"),
    }
    prompt.push_str(FORMAT);
    Some(prompt)
}

/// Prompt that summarizes the start of a turn cut in two; None when the cut is between turns.
pub fn prefix_prompt(plan: &Plan, limit: u64) -> Option<String> {
    (!plan.prefix.is_empty()).then(|| format!("# 对话\n{}\n\n# 要求\n{TURN_PREFIX_PROMPT}", fitted(&plan.prefix, limit)))
}

/// One summary from the history part (or the previous summary) and the turn prefix.
pub fn merge(plan: &Plan, history: Option<String>, prefix: Option<String>) -> String {
    let history = history.or_else(|| plan.previous.clone()).unwrap_or_else(|| "（没有更早的对话）".into());
    match prefix {
        Some(prefix) => format!("{history}\n\n---\n\n**本轮较早的部分（这一轮被拆开了）：**\n\n{prefix}"),
        None => history,
    }
}
