//! Shared prompt and artist text operations; no database or UI dependencies.
use super::artist_string;
use std::collections::HashSet;

pub(crate) fn normalize_artist_name(artist_name: &str) -> &str {
    artist_name
        .trim()
        .strip_prefix("artist:")
        .unwrap_or(artist_name.trim())
        .trim()
}

pub(crate) fn prefix_artist_tag_in_prompt(prompt: &str, artist_name: &str) -> Option<String> {
    let mut changed = false;
    let mut result = String::with_capacity(prompt.len() + "artist:".len());
    let mut start = 0;

    for (index, ch) in prompt.char_indices() {
        if matches!(ch, ',' | '\n' | '\r') {
            append_prefixed_fragment(
                &mut result,
                &prompt[start..index],
                artist_name,
                &mut changed,
            );
            result.push(ch);
            start = index + ch.len_utf8();
        }
    }
    append_prefixed_fragment(&mut result, &prompt[start..], artist_name, &mut changed);

    changed.then_some(result)
}

pub(crate) fn normalized_bare_tag_in_fragment(fragment: &str) -> Option<String> {
    let token = fragment.trim();
    let bare = bare_tag_in_token(token)?;
    let normalized = bare.to_lowercase();
    (!normalized.starts_with("artist:")).then_some(normalized)
}

pub(crate) fn normalized_explicit_artist_tag_in_fragment(fragment: &str) -> Option<String> {
    let token = fragment.trim();
    let name = explicit_artist_name_in_token(token)?.trim();
    (!name.is_empty()).then(|| name.to_lowercase())
}

pub(crate) fn prefix_known_artist_tags_in_prompt(
    prompt: &str,
    selected_names: &HashSet<String>,
) -> Option<(String, Vec<String>)> {
    let mut changed = false;
    let mut matched_names = HashSet::new();
    let mut result = String::with_capacity(prompt.len() + "artist:".len());
    let mut start = 0;

    for (index, ch) in prompt.char_indices() {
        if matches!(ch, ',' | '\n' | '\r') {
            append_known_artist_fragment(
                &mut result,
                &prompt[start..index],
                selected_names,
                &mut matched_names,
                &mut changed,
            );
            result.push(ch);
            start = index + ch.len_utf8();
        }
    }
    append_known_artist_fragment(
        &mut result,
        &prompt[start..],
        selected_names,
        &mut matched_names,
        &mut changed,
    );

    changed.then(|| {
        let mut matched_names = matched_names.into_iter().collect::<Vec<_>>();
        matched_names.sort();
        (result, matched_names)
    })
}

fn append_known_artist_fragment(
    result: &mut String,
    fragment: &str,
    selected_names: &HashSet<String>,
    matched_names: &mut HashSet<String>,
    changed: &mut bool,
) {
    let start = fragment
        .find(|ch: char| !ch.is_whitespace())
        .unwrap_or(fragment.len());
    let end = fragment
        .rfind(|ch: char| !ch.is_whitespace())
        .map(|index| index + fragment[index..].chars().next().unwrap().len_utf8())
        .unwrap_or(start);
    let token = &fragment[start..end];
    if let Some((rewritten, matched_name)) = prefix_known_artist_tag_in_token(token, selected_names)
    {
        result.push_str(&fragment[..start]);
        result.push_str(&rewritten);
        result.push_str(&fragment[end..]);
        matched_names.insert(matched_name);
        *changed = true;
    } else {
        result.push_str(fragment);
    }
}

fn prefix_known_artist_tag_in_token(
    token: &str,
    selected_names: &HashSet<String>,
) -> Option<(String, String)> {
    if token.is_empty() {
        return None;
    }
    if let Some((inner, suffix)) = split_novelai_closer(token)
        && let Some((rewritten, matched_name)) =
            prefix_known_artist_tag_in_token(inner, selected_names)
    {
        return Some((format!("{rewritten}{suffix}"), matched_name));
    }
    if let Some((prefix, inner, suffix)) = split_novelai_weight(token)
        && let Some((rewritten, matched_name)) =
            prefix_known_artist_tag_in_token(inner, selected_names)
    {
        return Some((format!("{prefix}{rewritten}{suffix}"), matched_name));
    }
    if let Some((open, inner, close)) = split_outer_wrapper(token)
        && let Some((rewritten, matched_name)) =
            prefix_known_artist_tag_in_token(inner, selected_names)
    {
        return Some((format!("{open}{rewritten}{close}"), matched_name));
    }
    if let Some((name, weight)) = split_colon_weight(token)
        && let Some((rewritten, matched_name)) =
            prefix_known_artist_tag_in_token(name, selected_names)
    {
        return Some((format!("{rewritten}{weight}"), matched_name));
    }

    let matched_name = token.to_lowercase();
    if matched_name.starts_with("artist:") || !selected_names.contains(&matched_name) {
        return None;
    }
    Some((format!("artist:{token}"), matched_name))
}

fn bare_tag_in_token(token: &str) -> Option<&str> {
    if token.is_empty() {
        return None;
    }
    if let Some((inner, _)) = split_novelai_closer(token) {
        return bare_tag_in_token(inner);
    }
    if let Some((_, inner, _)) = split_novelai_weight(token) {
        return bare_tag_in_token(inner);
    }
    if let Some((_, inner, _)) = split_outer_wrapper(token) {
        return bare_tag_in_token(inner);
    }
    if let Some((name, _)) = split_colon_weight(token) {
        return bare_tag_in_token(name);
    }
    Some(token)
}

fn explicit_artist_name_in_token(token: &str) -> Option<&str> {
    if token.is_empty() {
        return None;
    }
    if let Some((inner, _)) = split_novelai_closer(token) {
        return explicit_artist_name_in_token(inner);
    }
    if let Some((_, inner, _)) = split_novelai_weight(token) {
        return explicit_artist_name_in_token(inner);
    }
    if let Some((_, inner, _)) = split_outer_wrapper(token) {
        return explicit_artist_name_in_token(inner);
    }
    if let Some((name, _)) = split_colon_weight(token) {
        return explicit_artist_name_in_token(name);
    }
    token
        .get(.."artist:".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("artist:"))
        .then(|| &token["artist:".len()..])
}

fn append_prefixed_fragment(
    result: &mut String,
    fragment: &str,
    artist_name: &str,
    changed: &mut bool,
) {
    if let Some(rewritten) = prefix_artist_tag_in_fragment(fragment, artist_name) {
        result.push_str(&rewritten);
        *changed = true;
    } else {
        result.push_str(fragment);
    }
}

fn prefix_artist_tag_in_fragment(fragment: &str, artist_name: &str) -> Option<String> {
    let start = fragment
        .find(|ch: char| !ch.is_whitespace())
        .unwrap_or(fragment.len());
    let end = fragment
        .rfind(|ch: char| !ch.is_whitespace())
        .map(|index| index + fragment[index..].chars().next().unwrap().len_utf8())
        .unwrap_or(start);
    let token = &fragment[start..end];
    let rewritten = prefix_artist_tag_in_token(token, artist_name)?;

    Some(format!(
        "{}{}{}",
        &fragment[..start],
        rewritten,
        &fragment[end..]
    ))
}

fn prefix_artist_tag_in_token(token: &str, artist_name: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }

    if let Some((inner, suffix)) = split_novelai_closer(token)
        && let Some(rewritten) = prefix_artist_tag_in_token(inner, artist_name)
    {
        return Some(format!("{rewritten}{suffix}"));
    }

    if let Some((prefix, inner, suffix)) = split_novelai_weight(token)
        && let Some(rewritten) = prefix_artist_tag_in_token(inner, artist_name)
    {
        return Some(format!("{prefix}{rewritten}{suffix}"));
    }

    if let Some((open, inner, close)) = split_outer_wrapper(token)
        && let Some(rewritten) = prefix_artist_tag_in_token(inner, artist_name)
    {
        return Some(format!("{open}{rewritten}{close}"));
    }

    if let Some((name, weight)) = split_colon_weight(token)
        && let Some(rewritten) = prefix_artist_tag_in_token(name, artist_name)
    {
        return Some(format!("{rewritten}{weight}"));
    }

    (token == artist_name).then(|| format!("artist:{token}"))
}

fn split_novelai_closer(token: &str) -> Option<(&str, &str)> {
    // Numerical emphasis can span comma-delimited tags, leaving its closing `::`
    // attached to a fragment whose opening weight appeared in an earlier fragment.
    let without_closer = token.strip_suffix("::")?;
    let inner = without_closer.trim_end();
    (!inner.is_empty()).then_some((inner, &token[inner.len()..]))
}

fn split_novelai_weight(token: &str) -> Option<(&str, &str, &str)> {
    let weight_end = token.find("::")? + "::".len();
    let rest = &token[weight_end..];
    if rest.is_empty() {
        return None;
    }

    let (inner, suffix) = rest
        .strip_suffix("::")
        .map_or((rest, ""), |inner| (inner, "::"));
    if inner.is_empty() {
        return None;
    }

    Some((&token[..weight_end], inner, suffix))
}

fn split_outer_wrapper(token: &str) -> Option<(char, &str, char)> {
    let open = token.chars().next()?;
    let close = token.chars().next_back()?;
    if !matches!((open, close), ('(', ')') | ('{', '}') | ('[', ']')) {
        return None;
    }

    let start = open.len_utf8();
    let end = token.len() - close.len_utf8();
    Some((open, &token[start..end], close))
}

fn split_colon_weight(token: &str) -> Option<(&str, &str)> {
    let index = token.rfind(':')?;
    let weight = &token[index + ':'.len_utf8()..];
    if weight.is_empty() || weight.parse::<f32>().is_err() {
        return None;
    }

    let name = &token[..index];
    (!name.is_empty()).then_some((name, &token[index..]))
}

pub(crate) fn combined_artists(
    positive_prompt: &str,
    character_prompt: Option<&str>,
) -> Option<String> {
    artist_string(positive_prompt, character_prompt)
}

pub(crate) fn normalize_prompt_token(raw: &str) -> String {
    let mut token = raw.trim();
    loop {
        let before = token;
        if let Some(stripped) = token.strip_suffix("::") {
            token = stripped.trim();
        }
        if let Some(index) = token.find("::") {
            let prefix = token[..index].trim();
            if prefix.is_empty() || prefix.parse::<f32>().is_ok() {
                token = token[index + 2..].trim();
            }
        }
        if let Some((open, close)) = token.chars().next().zip(token.chars().next_back())
            && matches!((open, close), ('(', ')') | ('{', '}') | ('[', ']'))
        {
            token = token[open.len_utf8()..token.len() - close.len_utf8()].trim();
        }
        if let Some(index) = token.rfind(':') {
            let weight = token[index + 1..].trim();
            if !weight.is_empty() && weight.parse::<f32>().is_ok() {
                token = token[..index].trim();
            }
        }
        if token == before {
            break;
        }
    }
    token.to_lowercase()
}
