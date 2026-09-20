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

pub(crate) fn delete_prompt_tags(current: Option<&str>, targets: &str) -> Option<String> {
    let targets = parse_prompt_tokens(targets)
        .into_iter()
        .collect::<HashSet<_>>();
    let current = current?;
    let retained = current
        .split([',', '，', '\n', '\r'])
        .map(str::trim)
        .filter(|fragment| {
            !fragment.is_empty() && !targets.contains(&normalize_prompt_token(fragment))
        })
        .collect::<Vec<_>>();
    nonempty(retained.join(", "))
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
