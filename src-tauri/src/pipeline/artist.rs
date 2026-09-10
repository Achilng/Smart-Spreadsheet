use std::collections::HashSet;

/// 画师串优先保留显式画师片段；没有 artist: 时以整段正向提示词兜底。
/// 不改变 extract_artist_tags 的语义，避免把普通 Tag 当成已确认画师。
pub fn artist_string(positive_prompt: &str, character_prompt: Option<&str>) -> Option<String> {
    let combined = format!("{positive_prompt}\n{}", character_prompt.unwrap_or_default());
    let artists = extract_artist_tags(&combined);
    if artists.iter().any(|tag| tag.to_ascii_lowercase().contains("artist:")) {
        Some(artists.join("\n"))
    } else {
        let positive = positive_prompt.trim();
        (!positive.is_empty()).then(|| positive.to_owned())
    }
}

pub fn extract_artist_tags(positive_prompt: &str) -> Vec<String> {
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
    fn artist_string_falls_back_to_whole_positive_only_without_explicit_artists() {
        assert_eq!(artist_string("  painter, 1girl\n{blue eyes}  ", Some("character:name")),
            Some("painter, 1girl\n{blue eyes}".into()));
        assert_eq!(artist_string("artist collaboration, painter", None),
            Some("artist collaboration, painter".into()));
        assert_eq!(artist_string("painter, 1girl", Some("0.5::ARTIST:alice::")),
            Some("0.5::ARTIST:alice::".into()));
        assert_eq!(artist_string("artist:alice, quality, artist:bob", None),
            Some("artist:alice\nartist:bob".into()));
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
