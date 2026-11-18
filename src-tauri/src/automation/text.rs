use crate::pipeline::prompt_text::normalize_prompt_token;
use regex::RegexBuilder;
use std::collections::HashSet;

pub(crate) fn parse_prompt_tokens(value: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    value
        .split([',', '，', '\n', '\r'])
        .map(normalize_prompt_token)
        .filter(|token| !token.is_empty() && seen.insert(token.clone()))
        .collect()
}

pub(crate) fn prompt_token_set(value: &str) -> HashSet<String> {
    value
        .split([',', '，', '\n', '\r'])
        .map(normalize_prompt_token)
        .filter(|token| !token.is_empty())
        .collect()
}

/// 规则匹配只移除权重外壳并忽略大小写，不内置角色或语义别名。

pub(crate) fn artist_set(value: Option<&str>) -> HashSet<String> {
    value
        .unwrap_or("")
        .split([',', '，', '\n', '\r'])
        .map(normalize_artist)
        .filter(|artist| !artist.is_empty())
        .collect()
}

pub(crate) fn normalize_artist(value: &str) -> String {
    let normalized = normalize_prompt_token(value);
    normalized
        .strip_prefix("artist:")
        .unwrap_or(&normalized)
        .trim()
        .to_owned()
}

pub(crate) fn normalized_strings(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .flat_map(|value| value.split([',', '，', '\n', '\r']))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .collect()
}

pub(crate) fn append_prompt(current: Option<&str>, value: &str) -> Option<String> {
    let additions = value
        .split([',', '，', '\n', '\r'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if additions.is_empty() {
        return current.map(str::to_owned);
    }
    let mut available = prompt_token_set(current.unwrap_or(""));
    let missing = additions
        .into_iter()
        .filter(|addition| available.insert(normalize_prompt_token(addition)))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return current.map(str::to_owned);
    }
    let addition = missing.join(", ");
    nonempty(
        match current.map(str::trim).filter(|value| !value.is_empty()) {
            Some(current) => format!("{current}, {addition}"),
            None => addition,
        },
    )
}

/// Splits a prompt into fragments and the separators between them (`,` `，` and line breaks),
/// so that `fragments.concat()` interleaved with `separators` gives back the original text.
fn split_keeping_separators(text: &str) -> (Vec<&str>, Vec<&str>) {
    let mut fragments = Vec::new();
    let mut separators = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        let end = match ch {
            '\r' if chars.peek().is_some_and(|(_, next)| *next == '\n') => {
                chars.next();
                index + 2
            }
            ',' | '，' | '\n' | '\r' => index + ch.len_utf8(),
            _ => continue,
        };
        fragments.push(&text[start..index]);
        separators.push(&text[index..end]);
        start = end;
    }
    fragments.push(&text[start..]);
    (fragments, separators)
}

fn closer_of(opener: &str) -> &'static str {
    match opener {
        "{" => "}",
        "[" => "]",
        "(" => ")",
        _ => "::",
    }
}

/// Weight and emphasis syntax at the edges of one fragment: `2::` / `{` / `[` / `(` that open a group,
/// `::` / `}` / `]` / `)` that close one. A bracket only counts when it has no partner inside the
/// fragment, so `name (artist)` keeps its parentheses. Groups opened and closed in the same fragment
/// cancel out; what is left spans neighbouring tags. Returns (still-open, core, still-closed).
fn split_markers(fragment: &str) -> (Vec<&str>, &str, Vec<&str>) {
    let unmatched = |text: &str, open: char, close: char, opening: bool| {
        let (opens, closes) = (text.matches(open).count(), text.matches(close).count());
        if opening { opens > closes } else { closes > opens }
    };
    let mut rest = fragment.trim();
    let mut leading = Vec::new();
    // `::tag` closes the group opened before it, then starts the next tag.
    let mut trailing = Vec::new();
    if let Some(stripped) = rest.strip_prefix("::") {
        trailing.push("::");
        rest = stripped.trim_start();
    }
    loop {
        if let Some(open) = rest.chars().next().filter(|ch| matches!(ch, '{' | '[' | '('))
            && unmatched(rest, open, match open { '{' => '}', '[' => ']', _ => ')' }, true)
        {
            leading.push(&rest[..1]);
            rest = rest[1..].trim_start();
            continue;
        }
        if let Some(index) = rest.find("::")
            && rest[..index].trim().parse::<f32>().is_ok()
        {
            leading.push(&rest[..index + 2]);
            rest = rest[index + 2..].trim_start();
            continue;
        }
        break;
    }
    // Suffix closers are found outermost first; keep them in text order after a leading `::`.
    let head = trailing.len();
    loop {
        if let Some(stripped) = rest.strip_suffix("::") {
            trailing.insert(head, &rest[stripped.len()..]);
            rest = stripped.trim_end();
            continue;
        }
        if let Some(close) = rest.chars().next_back().filter(|ch| matches!(ch, '}' | ']' | ')')) {
            let open = match close { '}' => '{', ']' => '[', _ => '(' };
            if unmatched(rest, open, close, false) {
                trailing.insert(head, &rest[rest.len() - 1..]);
                rest = rest[..rest.len() - 1].trim_end();
                continue;
            }
        }
        break;
    }
    // Pair the innermost opener with the innermost closer while they belong together.
    while let (Some(open), Some(close)) = (leading.last(), trailing.first())
        && closer_of(open) == *close
    {
        leading.pop();
        trailing.remove(0);
    }
    (leading, rest, trailing)
}

/// A tag without the weight / bracket markers it shares with neighbouring tags.
pub(crate) fn tag_core(fragment: &str) -> &str {
    split_markers(fragment).1
}

#[derive(Clone, Copy, PartialEq)]
enum TagEdit {
    Keep,
    Delete,
    /// The tag opens a group spanning later tags: keep its opener in front of the next tag.
    Opener,
    /// The tag closes a group spanning earlier tags: keep its closer after the previous tag.
    Closer,
}

/// Deletes whole tags in place. Everything else — other tags, separators, line breaks, Chinese commas
/// inside text — stays exactly as written. Weight / bracket groups that span several tags keep their
/// markers, so `2::masterpiece, best quality::` minus masterpiece becomes `2::best quality::`.
pub(crate) fn delete_prompt_tags(current: Option<&str>, targets: &str) -> Option<String> {
    let targets = parse_prompt_tokens(targets)
        .into_iter()
        .collect::<HashSet<_>>();
    let current = current?;
    let (fragments, separators) = split_keeping_separators(current);
    let mut markers = vec![String::new(); fragments.len()];
    // For openers: the closing markers that would end exactly the groups they open.
    let mut expected = vec![String::new(); fragments.len()];
    let mut edits = fragments
        .iter()
        .enumerate()
        .map(|(index, fragment)| {
            let (leading, core, trailing) = split_markers(fragment);
            if core.is_empty() || !targets.contains(&normalize_prompt_token(core)) {
                return TagEdit::Keep;
            }
            match (leading.is_empty(), trailing.is_empty()) {
                (true, true) => TagEdit::Delete,
                (false, true) => {
                    markers[index] = leading.concat();
                    expected[index] = leading.iter().rev().map(|open| closer_of(open)).collect();
                    TagEdit::Opener
                }
                (true, false) => {
                    markers[index] = trailing.concat();
                    TagEdit::Closer
                }
                (false, false) => TagEdit::Keep,
            }
        })
        .collect::<Vec<_>>();
    if edits.iter().all(|edit| *edit == TagEdit::Keep) {
        return Some(current.to_owned());
    }
    let present = |edits: &[TagEdit], index: usize| {
        edits[index] != TagEdit::Delete && !fragments[index].trim().is_empty()
    };
    // A group whose every tag is deleted disappears with its markers; markers with nothing to attach
    // to leave their tag alone rather than break the syntax. The closer may also lead the next kept
    // fragment (`2::masterpiece, best quality, ::ultra-detailed`), then it goes and that tag stays.
    let mut rewritten: Vec<Option<String>> = vec![None; fragments.len()];
    for index in 0..edits.len() {
        match edits[index] {
            TagEdit::Opener => match (index + 1..edits.len()).find(|&next| present(&edits, next)) {
                Some(next) if edits[next] == TagEdit::Closer && expected[index] == markers[next] => {
                    edits[index] = TagEdit::Delete;
                    edits[next] = TagEdit::Delete;
                }
                Some(next)
                    if edits[next] == TagEdit::Keep
                        && fragments[next].trim_start().starts_with(expected[index].as_str()) =>
                {
                    let fragment = fragments[next];
                    let indent = &fragment[..fragment.len() - fragment.trim_start().len()];
                    let rest = fragment.trim_start()[expected[index].len()..].trim_start();
                    rewritten[next] = Some(format!("{indent}{rest}"));
                    edits[index] = TagEdit::Delete;
                }
                Some(_) => {}
                None => edits[index] = TagEdit::Keep,
            },
            TagEdit::Closer if !(0..index).any(|previous| present(&edits, previous)) => edits[index] = TagEdit::Keep,
            _ => {}
        }
    }
    // Tokens alternate fragment, separator, fragment, …; `None` drops a token.
    let mut tokens = Vec::with_capacity(fragments.len() * 2);
    for (index, fragment) in fragments.iter().enumerate() {
        tokens.push(Some(rewritten[index].take().unwrap_or_else(|| fragment.to_string())));
        if let Some(separator) = separators.get(index) {
            tokens.push(Some(separator.to_string()));
        }
    }
    let is_line = |token: &Option<String>| token.as_deref().is_some_and(|text| text.contains(['\n', '\r']));
    for (index, edit) in edits.iter().enumerate() {
        let at = index * 2;
        let before = (at > 0 && tokens[at - 1].is_some()).then(|| at - 1);
        let after = (at + 1 < tokens.len() && tokens[at + 1].is_some()).then_some(at + 1);
        match edit {
            TagEdit::Keep => {}
            TagEdit::Delete => {
                tokens[at] = None;
                // Drop one neighbouring separator; prefer losing a comma over a line break.
                let drop = match (before, after) {
                    (Some(b), Some(a)) if is_line(&tokens[b]) && !is_line(&tokens[a]) => Some(a),
                    (Some(b), _) => Some(b),
                    (None, a) => a,
                };
                if let Some(drop) = drop {
                    tokens[drop] = None;
                    // The separator after went, so the next tag now follows the one before: drop its indent.
                    if drop == at + 1
                        && let Some(Some(next)) = tokens.get_mut(at + 2)
                    {
                        *next = next.trim_start().to_owned();
                    }
                }
            }
            TagEdit::Opener => {
                let indent = &fragments[index][..fragments[index].len() - fragments[index].trim_start().len()];
                tokens[at] = Some(format!("{indent}{}", markers[index]));
                if let Some(after) = after {
                    tokens[after] = None;
                }
            }
            TagEdit::Closer => {
                let trimmed = fragments[index].trim_end();
                tokens[at] = Some(format!("{}{}", markers[index], &fragments[index][trimmed.len()..]));
                if let Some(before) = before {
                    tokens[before] = None;
                }
            }
        }
    }
    // Glue kept markers to their neighbouring tag.
    for (index, edit) in edits.iter().enumerate() {
        let neighbour = match edit {
            TagEdit::Opener => (index * 2 + 1..tokens.len()).find(|&at| tokens[at].is_some()),
            TagEdit::Closer => (0..index * 2).rev().find(|&at| tokens[at].is_some()),
            _ => None,
        };
        if let Some(text) = neighbour.and_then(|at| tokens[at].as_mut()) {
            *text = if *edit == TagEdit::Opener { text.trim_start() } else { text.trim_end() }.to_owned();
        }
    }
    nonempty(tokens.into_iter().flatten().collect::<String>().trim().to_owned())
}

pub(crate) fn replace_text(value: &str, find: &str, replace: &str, case_sensitive: bool) -> String {
    if case_sensitive {
        return value.replace(find, replace);
    }
    RegexBuilder::new(&regex::escape(find))
        .case_insensitive(true)
        .build()
        .map(|regex| regex.replace_all(value, replace).into_owned())
        .unwrap_or_else(|_| value.to_owned())
}

pub(crate) fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}
