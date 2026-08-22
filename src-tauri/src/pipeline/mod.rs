//! NovelAI PNG 提取管线（自 Novelai工具 移植的可复用部分）。
//! 旧工具的增量缓存、输出包和导入期去重逻辑未移植：
//! SQLite 资料库取代缓存，输出包转为导出功能，去重转为库内查重。

pub mod archive;
mod artist;
pub mod cancel;
pub mod json_dedupe;
mod metadata;
mod metadata_fingerprint;
pub mod parallel;
pub mod png_text;
pub mod scan;
pub mod stealth_png;

pub use artist::extract_artist_tags;
pub use metadata::{
    NovelAiMetadata, generation_model_of, parse_novelai_metadata, vibe_reference_count,
    vibe_status,
};
pub use metadata_fingerprint::metadata_fingerprint;
