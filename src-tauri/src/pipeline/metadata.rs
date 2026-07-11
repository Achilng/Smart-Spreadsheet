use super::artist::extract_artist_tags;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NovelAiMetadata {
    pub positive_prompt: String,
    pub negative_prompt: String,
    pub artist_tags: Vec<String>,
}

pub fn parse_novelai_metadata(text_chunks: &BTreeMap<String, String>) -> NovelAiMetadata {
    let comment_json = text_chunks
        .get("Comment")
        .and_then(|comment| serde_json::from_str::<Value>(comment).ok());

    let base_positive_prompt = first_non_empty([
        text_chunks.get("Description").cloned(),
        comment_json
            .as_ref()
            .and_then(|json| string_at_path(json, &["prompt"])),
        comment_json
            .as_ref()
            .and_then(|json| string_at_path(json, &["v4_prompt", "caption", "base_caption"])),
        comment_json
            .as_ref()
            .and_then(|json| find_string_key(json.get("v4_prompt")?, "base_caption")),
    ])
    .unwrap_or_default();

    let positive_prompt = join_non_empty(
        std::iter::once(base_positive_prompt).chain(
            comment_json
                .as_ref()
                .into_iter()
                .flat_map(v4_character_captions),
        ),
    );

    let negative_prompt = first_non_empty([
        comment_json
            .as_ref()
            .and_then(|json| string_at_path(json, &["uc"])),
        comment_json.as_ref().and_then(|json| {
            string_at_path(json, &["v4_negative_prompt", "caption", "base_caption"])
        }),
        comment_json
            .as_ref()
            .and_then(|json| find_string_key(json.get("v4_negative_prompt")?, "base_caption")),
    ])
    .unwrap_or_default();

    let artist_tags = extract_artist_tags(&positive_prompt);

    NovelAiMetadata {
        positive_prompt,
        negative_prompt,
        artist_tags,
    }
}

fn first_non_empty<const N: usize>(values: [Option<String>; N]) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

fn v4_character_captions(root: &Value) -> impl Iterator<Item = String> + '_ {
    root.get("v4_prompt")
        .and_then(|value| value.get("caption"))
        .and_then(|value| value.get("char_captions"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("char_caption"))
        .filter_map(json_value_to_text)
}

fn join_non_empty(values: impl IntoIterator<Item = String>) -> String {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn string_at_path(root: &Value, path: &[&str]) -> Option<String> {
    let mut current = root;
    for key in path {
        current = current.get(key)?;
    }

    json_value_to_text(current)
}

fn find_string_key(root: &Value, target_key: &str) -> Option<String> {
    match root {
        Value::Object(map) => {
            if let Some(value) = map.get(target_key).and_then(json_value_to_text) {
                return Some(value);
            }

            map.values()
                .find_map(|value| find_string_key(value, target_key))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_string_key(value, target_key)),
        _ => None,
    }
}

fn json_value_to_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(values) => {
            let parts = values
                .iter()
                .filter_map(json_value_to_text)
                .filter(|text| !text.trim().is_empty())
                .collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_novelai_metadata;
    use std::collections::BTreeMap;

    #[test]
    fn description_takes_priority_for_positive_prompt() {
        let mut chunks = BTreeMap::new();
        chunks.insert(
            "Description".to_string(),
            "artist:primary, high detail".to_string(),
        );
        chunks.insert(
            "Comment".to_string(),
            r#"{"prompt":"artist:secondary","uc":"low quality"}"#.to_string(),
        );

        let metadata = parse_novelai_metadata(&chunks);

        assert_eq!(metadata.positive_prompt, "artist:primary, high detail");
        assert_eq!(metadata.negative_prompt, "low quality");
        assert_eq!(metadata.artist_tags, vec!["artist:primary"]);
    }

    #[test]
    fn reads_prompt_and_uc_from_comment_json() {
        let mut chunks = BTreeMap::new();
        chunks.insert(
            "Comment".to_string(),
            r#"{"prompt":"best quality, artist:abc","uc":"bad hands"}"#.to_string(),
        );

        let metadata = parse_novelai_metadata(&chunks);

        assert_eq!(metadata.positive_prompt, "best quality, artist:abc");
        assert_eq!(metadata.negative_prompt, "bad hands");
        assert_eq!(metadata.artist_tags, vec!["artist:abc"]);
    }

    #[test]
    fn reads_v4_caption_structures() {
        let mut chunks = BTreeMap::new();
        chunks.insert(
            "Comment".to_string(),
            r#"{
              "v4_prompt":{"caption":{"base_caption":"artist:v4, scenic"}},
              "v4_negative_prompt":{"caption":{"base_caption":"worst quality"}}
            }"#
            .to_string(),
        );

        let metadata = parse_novelai_metadata(&chunks);

        assert_eq!(metadata.positive_prompt, "artist:v4, scenic");
        assert_eq!(metadata.negative_prompt, "worst quality");
        assert_eq!(metadata.artist_tags, vec!["artist:v4"]);
    }

    #[test]
    fn appends_v4_character_captions_to_positive_prompt() {
        let mut chunks = BTreeMap::new();
        chunks.insert(
            "Comment".to_string(),
            r#"{
              "prompt":"best quality, artist:base",
              "v4_prompt":{"caption":{
                "base_caption":"best quality, artist:base",
                "char_captions":[
                  {"char_caption":"1girl, artist:character", "centers":[{"x":0.5,"y":0.5}]},
                  {"char_caption":"  "},
                  {"char_caption":"blue eyes, long hair"}
                ]
              }}
            }"#
            .to_string(),
        );

        let metadata = parse_novelai_metadata(&chunks);

        assert_eq!(
            metadata.positive_prompt,
            "best quality, artist:base\n1girl, artist:character\nblue eyes, long hair"
        );
        assert_eq!(
            metadata.artist_tags,
            vec!["artist:base", "artist:character"]
        );
    }

    #[test]
    fn character_caption_is_positive_prompt_when_base_is_empty() {
        let mut chunks = BTreeMap::new();
        chunks.insert(
            "Comment".to_string(),
            r#"{"v4_prompt":{"caption":{"char_captions":[{"char_caption":"solo, red hair"}]}}}"#
                .to_string(),
        );

        let metadata = parse_novelai_metadata(&chunks);

        assert_eq!(metadata.positive_prompt, "solo, red hair");
    }
}
