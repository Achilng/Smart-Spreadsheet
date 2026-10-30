//! Shared prompt/artist normalization. All consumers use this parser; original text is never rewritten.
use super::style_extraction::prompt_hash;
use std::{collections::HashSet, ops::Range};

pub const DEFAULT_IGNORED_TAGS: &[&str] = &[
    "very aesthetic",
    "no text",
    "masterpiece",
    "best quality",
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
    text.split(|ch: char| ch.is_whitespace() || ch == '_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn clean_ignored_tags(tags: &[String]) -> Vec<String> {
    let mut unique = HashSet::new();
    tags.iter()
        .map(|tag| clean_tag(tag))
        .filter(|tag| !tag.is_empty() && unique.insert(tag.clone()))
        .collect()
}

pub fn validate_ignored_tags(tags: &[String]) -> Result<(), String> {
    if tags.len() > 256
        || tags.iter().any(|tag| {
            tag.chars().count() > 160
                || tag.contains([',', '{', '}', '[', ']', '\n', '\r'])
                || tag.contains("::")
        })
    {
        return Err(
            "白名单每行填写一个完整标签，不填写逗号或权重外壳；最多 256 条，每条最多 160 字。"
                .into(),
        );
    }
    Ok(())
}

pub fn rules_key(tags: &[String]) -> String {
    let mut tags = clean_ignored_tags(tags);
    tags.sort();
    tags.dedup();
    prompt_hash(&format!(
        "llm-match-v2:{}",
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
        let is_separator = |ch: char| ch.is_whitespace() || ch == '_';
        let trim = raw.trim_matches(is_separator);
        let offset = range.start + raw.len() - raw.trim_start_matches(is_separator).len();
        let mut whitespace: Option<Range<usize>> = None;
        for (i, ch) in trim.char_indices() {
            let span = offset + i..offset + i + ch.len_utf8();
            if is_separator(ch) {
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
    prompt_hash(&format!("llm-match-v2:{}:{}", result.literal, result.text))
}

/// Readable identity for artist grouping. No remaining content means no comparable style.
pub fn artist_key(artists: &str, ignored: &[String]) -> Option<String> {
    let result = mapped(artists, ignored);
    (!result.text.trim().is_empty()).then_some(result.text)
}

/// Browser Range offsets count UTF-16 code units; parser spans count UTF-8 bytes.
/// Resolve against the full string to recognize weight wrappers, but preserve
/// the user's exact selection within each tag, including deliberately partial tags.
pub fn selected_artist_tags(text: &str, start: usize, end: usize) -> Result<Vec<String>, String> {
    fn byte_offset(text: &str, offset: usize) -> Option<usize> {
        let mut units = 0;
        for (byte, ch) in text.char_indices() {
            if units == offset {
                return Some(byte);
            }
            units += ch.len_utf16();
        }
        (units == offset).then_some(text.len())
    }
    let (Some(start), Some(end)) = (byte_offset(text, start), byte_offset(text, end)) else {
        return Err("选区已失效，请重新选择文字。".into());
    };
    if start >= end {
        return Err("请先选中要加入白名单的文字。".into());
    }
    fn collect(text: &str, nodes: &[Node], selection: &Range<usize>, tags: &mut Vec<String>) {
        for node in nodes {
            match &node.body {
                Body::Group { children, .. } => collect(text, children, selection, tags),
                Body::Tag => {
                    let raw = &text[node.range.clone()];
                    let start = node.range.start + raw.len() - raw.trim_start().len();
                    let end = node.range.start + raw.trim_end().len();
                    if start < selection.end && end > selection.start {
                        tags.push(clean_tag(
                            &text[start.max(selection.start)..end.min(selection.end)],
                        ));
                    }
                }
            }
        }
    }
    let nodes = Parser { text, pos: 0 }
        .sequence("", 0)
        .ok_or("这段画师串的权重格式不完整，暂时无法安全识别 tag。")?;
    let mut tags = Vec::new();
    collect(text, &nodes, &(start..end), &mut tags);
    let tags = clean_ignored_tags(&tags);
    if tags.is_empty() {
        return Err("请先选中 tag 文字，可以跨逗号或换行选择多个。".into());
    }
    validate_ignored_tags(&tags)?;
    Ok(tags)
}

#[derive(serde::Serialize)]
pub struct ArtistHighlightSegment {
    text: String,
    ignored: bool,
}

/// Preserve every original character, marking only tags the shared parser ignores.
/// Malformed weights use literal comparison, so none of their tags are highlighted.
pub fn artist_highlight_segments(text: &str, ignored: &[String]) -> Vec<ArtistHighlightSegment> {
    fn collect(
        text: &str,
        nodes: &[Node],
        ignored: &HashSet<String>,
        ranges: &mut Vec<Range<usize>>,
    ) {
        for node in nodes {
            match &node.body {
                Body::Tag if ignored.contains(&clean_tag(&text[node.range.clone()])) => {
                    let raw = &text[node.range.clone()];
                    let start = node.range.start + raw.len() - raw.trim_start().len();
                    let end = node.range.start + raw.trim_end().len();
                    if start < end {
                        ranges.push(start..end);
                    }
                }
                Body::Group { children, .. } => collect(text, children, ignored, ranges),
                Body::Tag => {}
            }
        }
    }
    let mut ranges = Vec::new();
    let mut parser = Parser { text, pos: 0 };
    if let Some(nodes) = parser.sequence("", 0) {
        collect(
            text,
            &nodes,
            &ignored.iter().map(|tag| clean_tag(tag)).collect(),
            &mut ranges,
        );
    }
    let mut segments = Vec::new();
    let mut cursor = 0;
    for range in ranges {
        if cursor < range.start {
            segments.push(ArtistHighlightSegment {
                text: text[cursor..range.start].into(),
                ignored: false,
            });
        }
        segments.push(ArtistHighlightSegment {
            text: text[range.clone()].into(),
            ignored: true,
        });
        cursor = range.end;
    }
    if cursor < text.len() {
        segments.push(ArtistHighlightSegment {
            text: text[cursor..].into(),
            ignored: false,
        });
    }
    segments
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
