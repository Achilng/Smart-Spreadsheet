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

#[cfg(test)]
mod tests {
    use super::{artist_string, extract_artist_tags};

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
