use std::collections::BTreeMap;

use serde_json::Value;
use sha2::{Digest, Sha256};

const FINGERPRINT_VERSION: &[u8] = b"novelai-metadata-v1\0";

/// 为 NovelAI PNG 的完整生成元数据生成稳定指纹。
///
/// `Description` 保留原始文本；`Comment` 若为 JSON，则先按结构重新序列化，
/// 从而忽略对象键顺序和无意义的 JSON 空白。两者均缺失时不生成指纹。
pub fn metadata_fingerprint(text_chunks: &BTreeMap<String, String>) -> Option<String> {
    let description = text_chunks.get("Description");
    let comment = text_chunks.get("Comment");
    if description.is_none() && comment.is_none() {
        return None;
    }

    let canonical_comment = comment.map(|raw| {
        serde_json::from_str::<Value>(raw)
            .ok()
            .map(sort_json_keys)
            .and_then(|value| serde_json::to_string(&value).ok())
            .unwrap_or_else(|| raw.clone())
    });

    let mut hasher = Sha256::new();
    hasher.update(FINGERPRINT_VERSION);
    update_part(&mut hasher, description.map(String::as_bytes));
    update_part(&mut hasher, canonical_comment.as_deref().map(str::as_bytes));
    Some(format!("{:x}", hasher.finalize()))
}

fn sort_json_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries = map.into_iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            let mut sorted = serde_json::Map::new();
            for (key, value) in entries {
                sorted.insert(key, sort_json_keys(value));
            }
            Value::Object(sorted)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(sort_json_keys).collect()),
        other => other,
    }
}

fn update_part(hasher: &mut Sha256, value: Option<&[u8]>) {
    match value {
        Some(bytes) => {
            hasher.update([1]);
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        }
        None => hasher.update([0]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_comment_json_key_order_and_whitespace() {
        let first = BTreeMap::from([
            ("Description".into(), "artist:test".into()),
            ("Comment".into(), r#"{"seed":1,"steps":28}"#.into()),
        ]);
        let second = BTreeMap::from([
            ("Description".into(), "artist:test".into()),
            ("Comment".into(), "{ \"steps\": 28, \"seed\": 1 }".into()),
        ]);

        assert_eq!(metadata_fingerprint(&first), metadata_fingerprint(&second));
    }

    #[test]
    fn changes_when_seed_or_description_changes() {
        let base = BTreeMap::from([
            ("Description".into(), "artist:test".into()),
            ("Comment".into(), r#"{"seed":1}"#.into()),
        ]);
        let other_seed = BTreeMap::from([
            ("Description".into(), "artist:test".into()),
            ("Comment".into(), r#"{"seed":2}"#.into()),
        ]);
        let other_prompt = BTreeMap::from([
            ("Description".into(), "artist:other".into()),
            ("Comment".into(), r#"{"seed":1}"#.into()),
        ]);

        assert_ne!(
            metadata_fingerprint(&base),
            metadata_fingerprint(&other_seed)
        );
        assert_ne!(
            metadata_fingerprint(&base),
            metadata_fingerprint(&other_prompt)
        );
    }

    #[test]
    fn requires_novelai_text_metadata() {
        assert_eq!(metadata_fingerprint(&BTreeMap::new()), None);
    }
}
