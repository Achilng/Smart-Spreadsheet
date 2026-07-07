//! xlsx 嵌入图提取：现仅服务于 v1→v2 迁移遗留的旧工作簿副本
//! （见 `storage::process_pending_embedded_extractions`）。
//! xlsx 导入功能已退役，不再产生新的嵌入图副本。

use std::fs::File;
use std::io::Read;
use std::path::Path;

use thiserror::Error;
use zip::ZipArchive;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedImageRef {
    pub source_row: u32,
    pub source_column: u32,
    pub media_path: String,
}

#[derive(Debug, Error)]
pub enum ImageMapError {
    #[error("无法读取 Excel 文件: {0}")]
    Io(#[from] std::io::Error),
    #[error("无效的 XLSX ZIP 包: {0}")]
    Zip(#[from] zip::result::ZipError),
}

/// 批量提取嵌入图：只打开一次 ZIP，对每张成功读取的图片调用 `sink`。
/// 媒体部件缺失的图片跳过不计入返回值；`sink` 的 IO 错误会中止并向上传播。
pub fn extract_embedded_images(
    path: impl AsRef<Path>,
    images: &[EmbeddedImageRef],
    mut sink: impl FnMut(usize, &EmbeddedImageRef, &[u8]) -> std::io::Result<()>,
) -> Result<usize, ImageMapError> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut written = 0;
    let mut bytes = Vec::new();
    for (index, image) in images.iter().enumerate() {
        let mut entry = match archive.by_name(&image.media_path) {
            Ok(entry) => entry,
            Err(zip::result::ZipError::FileNotFound) => continue,
            Err(error) => return Err(error.into()),
        };
        bytes.clear();
        bytes.reserve(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        sink(index, image, &bytes)?;
        written += 1;
    }
    Ok(written)
}
