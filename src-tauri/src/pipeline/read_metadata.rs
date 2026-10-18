//! Shared source precedence and diagnostics for imports, materials and backfills.
use super::{parse_novelai_metadata, png_text, stealth_png};
use image::RgbaImage;
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

pub struct MetadataRead {
    pub chunks: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

impl MetadataRead {
    pub fn is_unreadable(&self) -> bool {
        self.chunks.is_empty() && !self.warnings.is_empty()
    }
}

// Objects merge by field; arrays (including an explicit empty array) are atomic.
fn fill_missing(primary: &mut Value, fallback: Value) {
    if primary.is_null() || primary.as_str().is_some_and(|s| s.trim().is_empty()) {
        *primary = fallback;
    } else if let (Some(target), Value::Object(source)) = (primary.as_object_mut(), fallback) {
        for (key, value) in source {
            fill_missing(target.entry(key).or_insert(Value::Null), value);
        }
    }
}

pub fn read_novelai_metadata(path: &Path, decoded: Option<&RgbaImage>) -> MetadataRead {
    let mut warnings = Vec::new();
    let mut chunks = png_text::read_png_text_chunks(path).unwrap_or_else(|error| {
        warnings.push(format!("PNG 元数据读取失败：{error}"));
        BTreeMap::new()
    });
    let primary = parse_novelai_metadata(&chunks);
    let hidden = match decoded {
        Some(image) => stealth_png::read_stealth_png_metadata_from_rgba(image),
        None => stealth_png::read_stealth_png_metadata(path),
    };
    match hidden {
        Ok(Some(hidden)) => {
            for (key, value) in hidden {
                let current = chunks.entry(key.clone()).or_default();
                if current.trim().is_empty() {
                    *current = value;
                } else if key == "Comment" {
                    if let Ok(fallback) = serde_json::from_str::<Value>(&value) {
                        let mut primary = serde_json::from_str(current).unwrap_or(Value::Null);
                        fill_missing(&mut primary, fallback);
                        *current = primary.to_string();
                    }
                }
            }
            // Source aliases must not let hidden Description/Source/uc override
            // an ordinary Comment's effective prompt/model/negative caption.
            if !primary.positive_prompt.is_empty() {
                chunks.insert("Description".into(), primary.positive_prompt);
            }
            if let Some(model) = primary.generation_model {
                chunks.insert("Source".into(), model);
            }
            if !primary.negative_prompt.is_empty() {
                if let Some(mut comment) = chunks.get("Comment")
                    .and_then(|text| serde_json::from_str::<Value>(text).ok())
                    .filter(Value::is_object)
                {
                    comment["uc"] = Value::String(primary.negative_prompt);
                    chunks.insert("Comment".into(), comment.to_string());
                }
            }
        }
        Ok(None) => {}
        Err(error) => warnings.push(format!("隐藏元数据读取失败：{error}")),
    }
    MetadataRead { chunks, warnings }
}
