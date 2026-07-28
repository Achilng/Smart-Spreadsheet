use super::artist::extract_artist_tags;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct NovelAiMetadata {
    pub positive_prompt: String,
    pub character_prompt: String,
    pub negative_prompt: String,
    pub artist_tags: Vec<String>,
    pub vibe_reference_count: u32,
    /// VIBE 引用组合的稳定签名：逐项 SHA-256 后排序再整体哈希，
    /// 引用顺序不同不影响结果；无引用时为 None。
    pub vibe_signature: Option<String>,
    pub generation_model: Option<String>,
    pub generation_sampler: Option<String>,
    pub generation_steps: Option<u32>,
    pub generation_seed: Option<String>,
    pub generation_scale: Option<f64>,
    pub generation_cfg_rescale: Option<f64>,
    pub generation_noise_schedule: Option<String>,
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

    let positive_prompt = base_positive_prompt;
    let character_prompt = join_non_empty(
        comment_json
            .as_ref()
            .into_iter()
            .flat_map(v4_character_captions),
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

    let artist_tags = extract_artist_tags(&join_non_empty([
        positive_prompt.clone(),
        character_prompt.clone(),
    ]));
    let vibe_references = comment_json
        .as_ref()
        .and_then(|json| json.get("reference_image_multiple"))
        .and_then(Value::as_array);
    let vibe_reference_count = vibe_references.map_or(0, |references| {
        u32::try_from(references.len()).unwrap_or(u32::MAX)
    });
    let vibe_signature = vibe_references.and_then(|references| vibe_signature_of(references));
    let generation_model = first_non_empty([
        text_chunks.get("Source").cloned(),
        comment_json
            .as_ref()
            .and_then(|json| string_at_path(json, &["model"])),
        comment_json
            .as_ref()
            .and_then(|json| string_at_path(json, &["source"])),
    ]);
    let generation_sampler = comment_json
        .as_ref()
        .and_then(|json| string_at_path(json, &["sampler"]));
    let generation_steps = comment_json
        .as_ref()
        .and_then(|json| number_at_path(json, &["steps"]))
        .filter(|value| *value >= 0.0 && *value <= f64::from(u32::MAX))
        .map(|value| value as u32);
    let generation_seed = comment_json
        .as_ref()
        .and_then(|json| json.get("seed"))
        .and_then(json_scalar_to_text);
    let generation_scale = comment_json
        .as_ref()
        .and_then(|json| number_at_path(json, &["scale"]));
    let generation_cfg_rescale = comment_json
        .as_ref()
        .and_then(|json| number_at_path(json, &["cfg_rescale"]));
    let generation_noise_schedule = comment_json
        .as_ref()
        .and_then(|json| string_at_path(json, &["noise_schedule"]));

    NovelAiMetadata {
        positive_prompt,
        character_prompt,
        negative_prompt,
        artist_tags,
        vibe_reference_count,
        vibe_signature,
        generation_model,
        generation_sampler,
        generation_steps,
        generation_seed,
        generation_scale,
        generation_cfg_rescale,
        generation_noise_schedule,
    }
}

/// 只读取 VIBE 引用数量；供旧资料库索引回填使用。
pub fn vibe_reference_count(text_chunks: &BTreeMap<String, String>) -> u32 {
    vibe_status(text_chunks).0
}

/// 读取 VIBE 引用数量与组合签名；供旧资料库索引回填使用。
pub fn vibe_status(text_chunks: &BTreeMap<String, String>) -> (u32, Option<String>) {
    let Some(references) = text_chunks
        .get("Comment")
        .and_then(|comment| serde_json::from_str::<Value>(comment).ok())
        .and_then(|json| json.get("reference_image_multiple").cloned())
    else {
        return (0, None);
    };
    let Some(references) = references.as_array() else {
        return (0, None);
    };
    let count = u32::try_from(references.len()).unwrap_or(u32::MAX);
    (count, vibe_signature_of(references))
}

/// 计算 VIBE 引用组合的稳定签名：对每项引用的规范 JSON 序列化分别取
/// SHA-256，再将各项哈希排序后整体哈希。同一组引用无论顺序如何都得到
/// 相同签名；空数组返回 None（无 VIBE 不参与聚合）。
fn vibe_signature_of(references: &[Value]) -> Option<String> {
    if references.is_empty() {
        return None;
    }
    let mut item_hashes = references
        .iter()
        .map(|reference| {
            let canonical = reference.to_string();
            format!("{:x}", Sha256::digest(canonical.as_bytes()))
        })
        .collect::<Vec<_>>();
    item_hashes.sort_unstable();
    let mut hasher = Sha256::new();
    for hash in &item_hashes {
        hasher.update(hash.as_bytes());
        hasher.update([0]);
    }
    Some(format!("{:x}", hasher.finalize()))
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

fn number_at_path(root: &Value, path: &[&str]) -> Option<f64> {
    let mut current = root;
    for key in path {
        current = current.get(key)?;
    }
    current
        .as_f64()
        .or_else(|| current.as_str()?.trim().parse::<f64>().ok())
}

fn json_scalar_to_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
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
    use super::{parse_novelai_metadata, vibe_reference_count, vibe_status};
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
    fn keeps_v4_character_captions_separate_from_positive_prompt() {
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

        assert_eq!(metadata.positive_prompt, "best quality, artist:base");
        assert_eq!(
            metadata.character_prompt,
            "1girl, artist:character\nblue eyes, long hair"
        );
        assert_eq!(
            metadata.artist_tags,
            vec!["artist:base", "artist:character"]
        );
    }

    #[test]
    fn character_caption_stays_separate_when_base_is_empty() {
        let mut chunks = BTreeMap::new();
        chunks.insert(
            "Comment".to_string(),
            r#"{"v4_prompt":{"caption":{"char_captions":[{"char_caption":"solo, red hair"}]}}}"#
                .to_string(),
        );

        let metadata = parse_novelai_metadata(&chunks);

        assert_eq!(metadata.positive_prompt, "");
        assert_eq!(metadata.character_prompt, "solo, red hair");
    }

    #[test]
    fn counts_vibe_references_from_comment() {
        let mut chunks = BTreeMap::new();
        chunks.insert(
            "Comment".to_string(),
            r#"{"reference_image_multiple":[{"strength":0.5},{"strength":0.8}]}"#.to_string(),
        );

        assert_eq!(vibe_reference_count(&chunks), 2);
        assert_eq!(parse_novelai_metadata(&chunks).vibe_reference_count, 2);
    }

    #[test]
    fn vibe_signature_is_order_independent_and_none_when_empty() {
        let chunks_of = |comment: &str| {
            let mut chunks = BTreeMap::new();
            chunks.insert("Comment".to_string(), comment.to_string());
            chunks
        };

        let forward = chunks_of(r#"{"reference_image_multiple":["AAA","BBB"]}"#);
        let reversed = chunks_of(r#"{"reference_image_multiple":["BBB","AAA"]}"#);
        let different = chunks_of(r#"{"reference_image_multiple":["AAA","CCC"]}"#);
        let empty = chunks_of(r#"{"reference_image_multiple":[]}"#);
        let missing = chunks_of(r#"{"prompt":"no vibes"}"#);

        let forward_signature = parse_novelai_metadata(&forward).vibe_signature;
        assert!(forward_signature.as_ref().is_some_and(|s| s.len() == 64));
        assert_eq!(
            forward_signature,
            parse_novelai_metadata(&reversed).vibe_signature
        );
        assert_ne!(
            forward_signature,
            parse_novelai_metadata(&different).vibe_signature
        );
        assert_eq!(parse_novelai_metadata(&empty).vibe_signature, None);
        assert_eq!(parse_novelai_metadata(&missing).vibe_signature, None);

        assert_eq!(vibe_status(&forward), (2, forward_signature));
        assert_eq!(vibe_status(&missing), (0, None));
    }

    #[test]
    fn reads_generation_parameters_without_losing_large_seed() {
        let mut chunks = BTreeMap::new();
        chunks.insert("Source".into(), "NovelAI Diffusion V4.5 Full".into());
        chunks.insert(
            "Comment".into(),
            r#"{"sampler":"k_euler_ancestral","steps":28,"seed":18446744073709551615,"scale":5.5,"cfg_rescale":0.2,"noise_schedule":"karras"}"#.into(),
        );

        let metadata = parse_novelai_metadata(&chunks);

        assert_eq!(
            metadata.generation_model.as_deref(),
            Some("NovelAI Diffusion V4.5 Full")
        );
        assert_eq!(
            metadata.generation_sampler.as_deref(),
            Some("k_euler_ancestral")
        );
        assert_eq!(metadata.generation_steps, Some(28));
        assert_eq!(
            metadata.generation_seed.as_deref(),
            Some("18446744073709551615")
        );
        assert_eq!(metadata.generation_scale, Some(5.5));
        assert_eq!(metadata.generation_cfg_rescale, Some(0.2));
        assert_eq!(
            metadata.generation_noise_schedule.as_deref(),
            Some("karras")
        );
    }
}
