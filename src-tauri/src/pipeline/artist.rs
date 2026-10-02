use std::collections::HashSet;
use std::sync::LazyLock;

/// Prompt fragments use XML-like delimiters, not a complete XML document.
/// Keep each complete block intact so comma-spanning weights and repetitions survive.
pub fn extract_artist_blocks(prompt: &str) -> Option<Vec<String>> {
    static BLOCK: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(?is)<artist\s*>(.*?)</artist\s*>").unwrap());
    let mut found = false;
    let blocks = BLOCK
        .captures_iter(prompt)
        .filter_map(|capture| {
            found = true;
            let body = capture[1].trim();
            (!body.is_empty()).then(|| body.to_owned())
        })
        .collect();
    found.then_some(blocks)
}

/// 完整 artist 块优先，其次显式画师片段，最后以整段正向提示词兜底。
/// 标签外的旧格式沿用原规则，不把普通裸 Tag 当成已确认画师。
pub fn artist_string(positive_prompt: &str, character_prompt: Option<&str>) -> Option<String> {
    // Parse fields separately: an opening tag in one field cannot close in another.
    let positive_blocks = extract_artist_blocks(positive_prompt);
    let character_blocks = extract_artist_blocks(character_prompt.unwrap_or_default());
    if positive_blocks.is_some() || character_blocks.is_some() {
        let blocks: Vec<_> = positive_blocks
            .into_iter()
            .flatten()
            .chain(character_blocks.into_iter().flatten())
            .collect();
        return (!blocks.is_empty()).then(|| blocks.join("\n"));
    }
    let combined = format!(
        "{positive_prompt}\n{}",
        character_prompt.unwrap_or_default()
    );
    let artists = extract_legacy_artist_tags(&combined);
    if artists
        .iter()
        .any(|tag| tag.to_ascii_lowercase().contains("artist:"))
    {
        Some(artists.join("\n"))
    } else {
        let positive = positive_prompt.trim();
        (!positive.is_empty()).then(|| positive.to_owned())
    }
}

pub fn extract_artist_tags(positive_prompt: &str) -> Vec<String> {
    if let Some(blocks) = extract_artist_blocks(positive_prompt) {
        return blocks;
    }
    extract_legacy_artist_tags(positive_prompt)
}

fn extract_legacy_artist_tags(positive_prompt: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut artists = Vec::new();

    for part in positive_prompt.split([',', '\n', '\r']) {
        let trimmed = part.trim();
        if trimmed.is_empty() || !trimmed.to_lowercase().contains("artist") {
            continue;
        }

        if seen.insert(trimmed.to_string()) {
            artists.push(trimmed.to_string());
        }
    }

    artists
}

/// LLM 已确认的画风串无需 artist: 前缀。拆分仅用于随机池，保持原文存储。
/// 将跨逗号的数值权重分别包在每个片段外，避免随机抽取后出现半截权重。
pub(crate) fn llm_artist_pool_fragments(value: &str) -> Vec<String> {
    let mut weights: Vec<&str> = Vec::new();
    let mut fragments = Vec::new();
    for raw in value.split([',', '，', '\n', '\r']) {
        let mut token = raw.trim();
        loop {
            if let Some(rest) = token.strip_prefix("::") {
                weights.pop();
                token = rest.trim_start();
                continue;
            }
            if let Some((weight, rest)) = token.split_once("::")
                && weight.trim().parse::<f64>().is_ok_and(f64::is_finite)
            {
                weights.push(weight.trim());
                token = rest.trim_start();
                continue;
            }
            break;
        }
        let mut closing_count = 0;
        while closing_count < weights.len() {
            let Some(rest) = token.strip_suffix("::") else {
                break;
            };
            closing_count += 1;
            token = rest.trim_end();
        }
        if !token.is_empty() {
            let mut fragment = token.to_owned();
            for weight in weights.iter().rev() {
                fragment = format!("{weight}::{fragment}::");
            }
            fragments.push(fragment);
        }
        weights.truncate(weights.len() - closing_count);
    }
    fragments
}

#[cfg(test)]
mod tests {
    use super::{artist_string, extract_artist_tags};

    #[test]
    fn llm_pool_splits_bare_names_and_keeps_each_weight_complete() {
        let example = "0.5::ezu (e104mjd), noyu (noyu23386566)::, 0.8::fuzichoco, yumenouchi chiharu, torino aqua,, yukoring::, 0.7::sakinoji, alchemaniac::,,, year 2025, very aesthetic, masterpiece, no text,, ";
        let fragments = super::llm_artist_pool_fragments(example);
        assert_eq!(
            fragments,
            vec![
                "0.5::ezu (e104mjd)::",
                "0.5::noyu (noyu23386566)::",
                "0.8::fuzichoco::",
                "0.8::yumenouchi chiharu::",
                "0.8::torino aqua::",
                "0.8::yukoring::",
                "0.7::sakinoji::",
                "0.7::alchemaniac::",
                "year 2025",
                "very aesthetic",
                "masterpiece",
                "no text",
            ]
        );
        // 与随机画师串现有的“只用干净片段”开关保持一致。
        assert_eq!(
            fragments
                .iter()
                .filter(|s| !s.contains("::"))
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["year 2025", "very aesthetic", "masterpiece", "no text"]
        );
    }

    #[test]
    fn llm_pool_handles_nested_weights_line_breaks_and_empty_fragments() {
        assert_eq!(
            super::llm_artist_pool_fragments(" ,\r\n，"),
            Vec::<String>::new()
        );
        assert_eq!(
            super::llm_artist_pool_fragments("Alice， artist:Bob\r\n,,Alice"),
            vec!["Alice", "artist:Bob", "Alice"]
        );
        assert_eq!(
            super::llm_artist_pool_fragments("-0.5::A, 1.2::B, C::, D::, E"),
            vec![
                "-0.5::A::",
                "-0.5::1.2::B::::",
                "-0.5::1.2::C::::",
                "-0.5::D::",
                "E"
            ]
        );
        assert_eq!(
            super::llm_artist_pool_fragments("0.8::, A,\r\n B, ::, C"),
            vec!["0.8::A::", "0.8::B::", "C"]
        );
    }

    #[test]
    fn xml_artist_block_preserves_user_example_and_excludes_style() {
        let body = "1.1::parsley-f ::, 0.3::betabeet ::, 0.75::fuzichoco ::, 0.5::maccha_(mochancc) ::, 0.9::hiten_(hitenkei) ::,\r\n1.6::ciloranko ::, -5::artist_collaboration ::,";
        let prompt = format!(
            "<artist> {body} </artist>\n<style> year_2025, year_2026, fine_fabric_emphasis, detailed_background, ultra-detailed, </style> very aesthetic, amazing quality, no text"
        );
        assert_eq!(artist_string(&prompt, None).as_deref(), Some(body));
        assert_eq!(extract_artist_tags(&prompt), vec![body]);
    }

    #[test]
    fn xml_blocks_are_authoritative_and_preserve_repeated_weighted_groups() {
        let prompt = "artist:outside, <ARTIST > 0.8::a, b::, a </ARTIST> <style>artist:style</style> <artist>0.8::a, b::, a</artist>";
        assert_eq!(
            artist_string(prompt, Some("artist:outside_character <artist>c</artist>")),
            Some("0.8::a, b::, a\n0.8::a, b::, a\nc".into())
        );
        assert_eq!(artist_string("<artist> </artist> girl", None), None);
        assert!(extract_artist_tags("<artist></artist> artist:outside").is_empty());
    }

    #[test]
    fn incomplete_or_other_xml_tags_do_not_become_complete_artist_blocks() {
        assert_eq!(super::extract_artist_blocks("<artists>a</artists>"), None);
        assert_eq!(
            super::extract_artist_blocks("<artist>a, <style>sky</style>"),
            None
        );
        assert_eq!(
            artist_string("<artist>a", Some("b</artist>")),
            Some("<artist>a".into())
        );
    }

    #[test]
    fn artist_string_falls_back_to_whole_positive_only_without_explicit_artists() {
        assert_eq!(
            artist_string("  painter, 1girl\n{blue eyes}  ", Some("character:name")),
            Some("painter, 1girl\n{blue eyes}".into())
        );
        assert_eq!(
            artist_string("artist collaboration, painter", None),
            Some("artist collaboration, painter".into())
        );
        assert_eq!(
            artist_string("painter, 1girl", Some("0.5::ARTIST:alice::")),
            Some("0.5::ARTIST:alice::".into())
        );
        assert_eq!(
            artist_string("artist:alice, quality, artist:bob", None),
            Some("artist:alice\nartist:bob".into())
        );
        assert_eq!(artist_string(" \n ", Some("character:name")), None);
    }

    #[test]
    fn keeps_artist_fragments_and_deduplicates_exact_matches() {
        let tags = extract_artist_tags(
            "best quality, artist:maidcode1023, girl\n0.5::artist:xxx::, artist:maidcode1023, -3::artist collaboration::",
        );

        assert_eq!(
            tags,
            vec![
                "artist:maidcode1023",
                "0.5::artist:xxx::",
                "-3::artist collaboration::"
            ]
        );
    }

    #[test]
    fn ignores_non_artist_fragments() {
        let tags = extract_artist_tags("best quality, character:name, background");

        assert!(tags.is_empty());
    }
}
