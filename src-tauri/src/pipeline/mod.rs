//! NovelAI PNG 提取管线（自 Novelai工具 移植的可复用部分）。
//! 旧工具的增量缓存、输出包和导入期去重逻辑未移植：
//! SQLite 资料库取代缓存，输出包转为导出功能，去重转为库内查重。

pub mod archive;
mod artist;
pub mod json_dedupe;
mod metadata;
pub mod parallel;
pub mod png_text;
pub mod scan;
pub mod similarity;

pub use artist::extract_artist_tags;
pub use metadata::{NovelAiMetadata, parse_novelai_metadata};
