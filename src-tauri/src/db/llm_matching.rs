//! LLM reuse keys. Only the approved tags and separators are ignored; original text is never rewritten.
use super::style_extraction::prompt_hash;
use std::{collections::HashSet, ops::Range};

pub const DEFAULT_IGNORED_TAGS: &[&str] = &[
    "very aesthetic",
    "very_aesthetic",
    "no text",
    "no_text",
    "masterpiece",
    "best quality",
    "best_quality",
    "amazing quality",
    "absurdres",
    "the image is very aesthetic",
    "highres",
];

pub fn default_ignored_tags() -> Vec<String> {
    DEFAULT_IGNORED_TAGS
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

pub fn clean_tag(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn rules_key(tags: &[String]) -> String {
    let mut tags = tags.iter().map(|s| clean_tag(s)).collect::<Vec<_>>();
    tags.sort();
    tags.dedup();
    prompt_hash(&format!(
        "llm-match-v1:{}",
        serde_json::to_string(&tags).unwrap()
    ))
}

struct Node {
    range: Range<usize>,
    body: Body,
}
enum Body {
    Tag,
    Group {
        open: Range<usize>,
        children: Vec<Node>,
        close: Range<usize>,
    },
}
struct Parser<'a> {
    text: &'a str,
    pos: usize,
}
impl Parser<'_> {
    fn sequence(&mut self, close: &str, depth: usize) -> Option<Vec<Node>> {
        if depth > 64 {
            return None;
        }
        let mut nodes = Vec::new();
        loop {
            while self.pos < self.text.len() {
                let ch = self.text[self.pos..].chars().next()?;
                if ch.is_whitespace() || ch == ',' {
                    self.pos += ch.len_utf8();
                } else {
                    break;
                }
            }
            if self.pos == self.text.len() {
                return close.is_empty().then_some(nodes);
            }
            let rest = &self.text[self.pos..];
            if !close.is_empty() && rest.starts_with(close) {
                return Some(nodes);
            }
            if rest.starts_with("::") || rest.starts_with('}') || rest.starts_with(']') {
                return None;
            }
            let start = self.pos;
            let group = if rest.starts_with('{') {
                Some((1, "}"))
            } else if rest.starts_with('[') {
                Some((1, "]"))
            } else {
                rest.find("::").and_then(|index| {
                    rest[..index]
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|n| n.is_finite())
                        .map(|_| (index + 2, "::"))
                })
            };
            let body = if let Some((size, end)) = group {
                self.pos += size;
                let open = start..self.pos;
                let children = self.sequence(end, depth + 1)?;
                let closing = self.pos..self.pos + end.len();
                self.pos = closing.end;
                Body::Group {
                    open,
                    children,
                    close: closing,
                }
            } else {
                while self.pos < self.text.len() {
                    let current = &self.text[self.pos..];
                    let ch = current.chars().next()?;
                    if ch == '\\' {
                        self.pos += 1;
                        if let Some(escaped) = self.text[self.pos..].chars().next() {
                            self.pos += escaped.len_utf8();
                        }
                        continue;
                    }
                    if ch == ','
                        || ch == '\n'
                        || ch == '\r'
                        || (!close.is_empty() && current.starts_with(close))
                    {
                        break;
                    }
                    // Malformed wrappers use literal matching rather than losing a weight.
                    if current.starts_with("::") || matches!(ch, '{' | '}' | '[' | ']') {
                        return None;
                    }
                    self.pos += ch.len_utf8();
                }
                Body::Tag
            };
            if self.pos == start {
                return None;
            }
            nodes.push(Node {
                range: start..self.pos,
                body,
            });
            // A closed weight may be followed by punctuation/space, not an unseparated tag.
            if let Some(ch) = self.text[self.pos..].chars().next() {
                if !ch.is_whitespace()
                    && ch != ','
                    && (close.is_empty() || !self.text[self.pos..].starts_with(close))
                {
                    return None;
                }
            }
        }
    }
}

#[derive(Default)]
struct Mapped {
    text: String,
    chars: Vec<Range<usize>>,
    literal: bool,
}
impl Mapped {
    fn push(&mut self, ch: char, range: Range<usize>) {
        self.text.push(ch);
        self.chars.push(range);
    }
    fn raw(&mut self, text: &str, range: Range<usize>) {
        let raw = &text[range.clone()];
        let trim = raw.trim();
        let offset = range.start + raw.len() - raw.trim_start().len();
        let mut whitespace: Option<Range<usize>> = None;
        for (i, ch) in trim.char_indices() {
            let span = offset + i..offset + i + ch.len_utf8();
            if ch.is_whitespace() {
                if let Some(last) = whitespace.as_mut() {
                    last.end = span.end;
                } else {
                    whitespace = Some(span);
                }
            } else {
                if let Some(space) = whitespace.take() {
                    self.push(' ', space);
                }
                self.push(ch, span);
            }
        }
    }
    fn append(&mut self, other: Mapped) {
        self.text.push_str(&other.text);
        self.chars.extend(other.chars);
    }
}
fn render(text: &str, nodes: &[Node], ignored: &HashSet<String>) -> Mapped {
    let mut result = Mapped::default();
    let mut previous_end = None;
    for node in nodes {
        let mut part = Mapped::default();
        match &node.body {
            Body::Tag => {
                if !ignored.contains(&clean_tag(&text[node.range.clone()])) {
                    part.raw(text, node.range.clone());
                }
            }
            Body::Group {
                open,
                children,
                close,
            } => {
                let content = render(text, children, ignored);
                if !content.text.is_empty() {
                    part.raw(text, open.clone());
                    part.append(content);
                    part.raw(text, close.clone());
                }
            }
        }
        if part.text.is_empty() {
            continue;
        }
        if let Some(end) = previous_end {
            let separator = text[end..node.range.start]
                .char_indices()
                .find(|(_, ch)| matches!(ch, ',' | '\n' | '\r'))
                .map(|(index, ch)| end + index..end + index + ch.len_utf8())
                .unwrap_or(end..end);
            result.push(',', separator);
        }
        result.append(part);
        previous_end = Some(node.range.end);
    }
    result
}
fn mapped(text: &str, ignored: &[String]) -> Mapped {
    let mut parser = Parser { text, pos: 0 };
    if let Some(nodes) = parser.sequence("", 0) {
        render(
            text,
            &nodes,
            &ignored.iter().map(|s| clean_tag(s)).collect(),
        )
    } else {
        let mut result = Mapped {
            literal: true,
            ..Default::default()
        };
        for (index, ch) in text.char_indices() {
            result.push(ch, index..index + ch.len_utf8());
        }
        result
    }
}
pub fn key(prompt: &str, ignored: &[String]) -> String {
    let result = mapped(prompt, ignored);
    prompt_hash(&format!("llm-match-v1:{}:{}", result.literal, result.text))
}

/// Map a validated continuous result to the corresponding original characters of the new prompt.
/// Ignored words are never copied from the source, and the destination remains a literal substring.
pub fn project(
    source: &str,
    artists: &str,
    destination: &str,
    ignored: &[String],
) -> Option<String> {
    let start = source.find(artists)?;
    if source == destination {
        return Some(artists.to_owned());
    }
    let from = mapped(source, ignored);
    let to = mapped(destination, ignored);
    if from.literal != to.literal || from.text != to.text {
        return None;
    }
    if artists.is_empty() {
        return Some(String::new());
    }
    let end = start + artists.len();
    let indices = from
        .chars
        .iter()
        .enumerate()
        .filter(|(_, span)| span.start >= start && span.end <= end && !span.is_empty())
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    match (indices.first(), indices.last()) {
        (Some(&first), Some(&last)) => {
            Some(destination[to.chars[first].start..to.chars[last].end].to_owned())
        }
        _ => Some(String::new()),
    }
}
