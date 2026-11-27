//! Markdown written by the agent → the TipTap JSON the prompt-doc editor stores.
//!
//! The editor runs StarterKit (headings 1–3, lists, quotes, code, rules, bold / italic / strike /
//! code / link marks) plus images. Anything else is flattened into text so the document always
//! opens: tables become one paragraph per row with cells joined by " | ".
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde_json::{Map, Value, json};

struct Frame {
    node: Map<String, Value>,
    /// Opened by the converter (a tight list item's text, a table row), not by the Markdown.
    implicit: bool,
}

fn node(kind: &str) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("type".into(), json!(kind));
    map
}

fn is_textblock(kind: &str) -> bool {
    matches!(kind, "paragraph" | "heading" | "codeBlock")
}

struct Builder {
    stack: Vec<Frame>,
    marks: Vec<Value>,
    table_cell: usize,
    link_text: Vec<(String, bool)>,
}

impl Builder {
    fn top_kind(&self) -> &str {
        self.stack.last().and_then(|frame| frame.node["type"].as_str()).unwrap_or("doc")
    }

    fn push(&mut self, node: Map<String, Value>, implicit: bool) {
        self.stack.push(Frame { node, implicit });
    }

    fn append(&mut self, child: Value) {
        let parent = &mut self.stack.last_mut().expect("doc frame").node;
        parent.entry("content").or_insert_with(|| json!([])).as_array_mut().expect("content array").push(child);
    }

    fn pop(&mut self) {
        let Some(mut frame) = self.stack.pop() else { return };
        if frame.node["type"] == "listItem" {
            // A list item must start with a paragraph.
            let content = frame.node.entry("content").or_insert_with(|| json!([])).as_array_mut().expect("content array");
            if content.first().is_none_or(|first| first["type"] != "paragraph") {
                content.insert(0, json!({"type": "paragraph"}));
            }
        }
        if matches!(frame.node["type"].as_str(), Some("blockquote" | "bulletList" | "orderedList"))
            && frame.node.get("content").is_none_or(|c| c.as_array().is_none_or(Vec::is_empty))
        {
            return;
        }
        self.append(Value::Object(frame.node));
    }

    fn close_implicit(&mut self) {
        while self.stack.last().is_some_and(|frame| frame.implicit) {
            self.pop();
        }
    }

    fn open_block(&mut self, node: Map<String, Value>) {
        self.close_implicit();
        if is_textblock(self.top_kind()) {
            self.pop();
        }
        self.push(node, false);
    }

    fn ensure_textblock(&mut self) {
        if !is_textblock(self.top_kind()) {
            self.push(node("paragraph"), true);
        }
    }

    fn text(&mut self, text: &str, extra: Option<Value>) {
        if text.is_empty() {
            return;
        }
        self.ensure_textblock();
        if let Some(link) = self.link_text.last_mut() {
            link.1 = true;
        }
        let mut item = json!({"type": "text", "text": text});
        if self.top_kind() != "codeBlock" {
            let marks = match extra {
                Some(code) => vec![code],
                None => self.marks.clone(),
            };
            if !marks.is_empty() {
                item["marks"] = Value::Array(marks);
            }
        }
        self.append(item);
    }

    fn hard_break(&mut self) {
        self.ensure_textblock();
        if self.top_kind() == "codeBlock" {
            self.text("\n", None);
        } else {
            self.append(json!({"type": "hardBreak"}));
        }
    }

    fn end_mark(&mut self, kind: &str) {
        if let Some(index) = self.marks.iter().rposition(|mark| mark["type"] == kind) {
            self.marks.remove(index);
        }
    }

    fn end_block(&mut self, kind: &str) {
        self.close_implicit();
        if self.top_kind() == kind {
            self.pop();
        }
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        _ => 3,
    }
}

/// TipTap JSON for a Markdown text.
pub fn to_tiptap(markdown: &str) -> Value {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut builder = Builder { stack: vec![Frame { node: node("doc"), implicit: false }], marks: Vec::new(), table_cell: 0, link_text: Vec::new() };
    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => builder.open_block(node("paragraph")),
                Tag::Heading { level, .. } => {
                    let mut heading = node("heading");
                    heading.insert("attrs".into(), json!({"level": heading_level(level)}));
                    builder.open_block(heading);
                }
                Tag::BlockQuote(_) => builder.open_block(node("blockquote")),
                Tag::CodeBlock(kind) => {
                    let language = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(info) => info.split_whitespace().next().map(str::to_owned),
                        pulldown_cmark::CodeBlockKind::Indented => None,
                    };
                    let mut code = node("codeBlock");
                    code.insert("attrs".into(), json!({"language": language}));
                    builder.open_block(code);
                }
                Tag::List(start) => {
                    let mut list = node(if start.is_some() { "orderedList" } else { "bulletList" });
                    if let Some(start) = start {
                        list.insert("attrs".into(), json!({"start": start, "type": null}));
                    }
                    builder.open_block(list);
                }
                Tag::Item => builder.open_block(node("listItem")),
                Tag::TableHead | Tag::TableRow => {
                    builder.close_implicit();
                    builder.push(node("paragraph"), true);
                    builder.table_cell = 0;
                }
                Tag::TableCell => {
                    if builder.table_cell > 0 {
                        let marks = std::mem::take(&mut builder.marks);
                        builder.text(" | ", None);
                        builder.marks = marks;
                    }
                    builder.table_cell += 1;
                }
                Tag::Emphasis => builder.marks.push(json!({"type": "italic"})),
                Tag::Strong => builder.marks.push(json!({"type": "bold"})),
                Tag::Strikethrough => builder.marks.push(json!({"type": "strike"})),
                Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                    builder.marks.push(json!({"type": "link", "attrs": {
                        "href": dest_url.to_string(), "target": "_blank", "rel": "noopener noreferrer nofollow", "class": null,
                    }}));
                    builder.link_text.push((dest_url.to_string(), false));
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => builder.end_block("paragraph"),
                TagEnd::Heading(_) => builder.end_block("heading"),
                TagEnd::BlockQuote(_) => builder.end_block("blockquote"),
                TagEnd::CodeBlock => {
                    if let Some(frame) = builder.stack.last_mut()
                        && frame.node["type"] == "codeBlock"
                        && let Some(last) = frame.node.get_mut("content").and_then(Value::as_array_mut).and_then(|c| c.last_mut())
                        && let Some(text) = last["text"].as_str()
                    {
                        let trimmed = text.trim_end_matches('\n').to_owned();
                        last["text"] = json!(trimmed);
                    }
                    if let Some(frame) = builder.stack.last_mut()
                        && let Some(content) = frame.node.get_mut("content").and_then(Value::as_array_mut)
                    {
                        content.retain(|item| item["text"].as_str().is_none_or(|text| !text.is_empty()));
                    }
                    builder.end_block("codeBlock");
                }
                TagEnd::List(_) => {
                    builder.close_implicit();
                    if matches!(builder.top_kind(), "bulletList" | "orderedList") {
                        builder.pop();
                    }
                }
                TagEnd::Item => builder.end_block("listItem"),
                TagEnd::TableHead | TagEnd::TableRow => {
                    builder.close_implicit();
                    builder.table_cell = 0;
                }
                TagEnd::Emphasis => builder.end_mark("italic"),
                TagEnd::Strong => builder.end_mark("bold"),
                TagEnd::Strikethrough => builder.end_mark("strike"),
                TagEnd::Link | TagEnd::Image => {
                    let (url, had_text) = builder.link_text.pop().unwrap_or_default();
                    if !had_text {
                        builder.text(&url, None);
                    }
                    builder.end_mark("link");
                }
                _ => {}
            },
            Event::Text(text) => builder.text(&text, None),
            Event::Code(text) => builder.text(&text, Some(json!({"type": "code"}))),
            Event::InlineMath(text) | Event::DisplayMath(text) => builder.text(&text, None),
            Event::Html(text) | Event::InlineHtml(text) => builder.text(text.trim_end_matches('\n'), None),
            Event::FootnoteReference(name) => builder.text(&format!("[^{name}]"), None),
            Event::SoftBreak | Event::HardBreak => builder.hard_break(),
            Event::Rule => {
                builder.close_implicit();
                if is_textblock(builder.top_kind()) {
                    builder.pop();
                }
                builder.append(json!({"type": "horizontalRule"}));
            }
            Event::TaskListMarker(checked) => builder.text(if checked { "☑ " } else { "☐ " }, None),
        }
    }
    while builder.stack.len() > 1 {
        builder.pop();
    }
    let mut doc = builder.stack.pop().expect("doc frame").node;
    if doc.get("content").is_none_or(|c| c.as_array().is_none_or(Vec::is_empty)) {
        doc.insert("content".into(), json!([{"type": "paragraph"}]));
    }
    Value::Object(doc)
}

/// The text the editor would copy: textblocks one per line, hard breaks as newlines.
pub fn plain_text(doc: &Value) -> String {
    fn walk(node: &Value, lines: &mut Vec<String>) {
        let kind = node["type"].as_str().unwrap_or_default();
        if is_textblock(kind) {
            let mut line = String::new();
            for child in node["content"].as_array().into_iter().flatten() {
                match child["type"].as_str() {
                    Some("text") => line.push_str(child["text"].as_str().unwrap_or_default()),
                    Some("hardBreak") => line.push('\n'),
                    _ => {}
                }
            }
            lines.push(line);
            return;
        }
        for child in node["content"].as_array().into_iter().flatten() {
            walk(child, lines);
        }
    }
    let mut lines = Vec::new();
    walk(doc, &mut lines);
    lines.join("\n")
}

/// The doc's blocks with `addition` appended; a doc that is only one empty paragraph is replaced.
pub fn append(existing: &Value, addition: &Value) -> Value {
    let mut blocks = existing["content"].as_array().cloned().unwrap_or_default();
    let empty = blocks.iter().all(|block| block["type"] == "paragraph" && block.get("content").is_none_or(|c| c.as_array().is_none_or(Vec::is_empty)));
    if empty {
        blocks.clear();
    }
    blocks.extend(addition["content"].as_array().cloned().unwrap_or_default());
    json!({"type": "doc", "content": blocks})
}
