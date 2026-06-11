//! PNG 来源扫描（自 Novelai工具 移植并简化）。
//! 跳过旧版 Novelai工具 留下的缓存目录和输出包目录，避免把导出副本当作原图导入。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use thiserror::Error;
use walkdir::WalkDir;

/// 旧版 Novelai工具 的增量缓存目录名。
const LEGACY_CACHE_DIR_NAME: &str = ".novelai_metadata_cache";
/// 旧版 Novelai工具 输出包目录中的标记文件名。
const LEGACY_OUTPUT_MARKER_FILE_NAME: &str = ".novelai_metadata_output";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceImage {
    pub absolute_path: PathBuf,
    /// 相对扫描根目录的路径（单文件输入时为文件名），用于展示与压缩包身份键。
    pub relative_path: String,
    pub size: u64,
    pub modified_nanos: Option<i64>,
    pub created: Option<SystemTime>,
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("无法扫描输入: {0}")]
    Io(#[from] std::io::Error),
    #[error("无法扫描目录: {0}")]
    Walk(#[from] walkdir::Error),
    #[error("输入路径必须是文件夹、PNG 文件或受支持的压缩包: {0}")]
    UnsupportedInput(PathBuf),
}

pub fn is_png(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
}

/// 递归收集输入（文件夹或单个 PNG）下的全部 PNG，按相对路径排序保证扫描顺序稳定。
pub fn collect_png_files(input_path: &Path) -> Result<Vec<SourceImage>, ScanError> {
    if input_path.is_file() {
        if !is_png(input_path) {
            return Err(ScanError::UnsupportedInput(input_path.to_owned()));
        }
        let metadata = fs::metadata(input_path)?;
        let relative_path = input_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| input_path.display().to_string());
        return Ok(vec![source_image(input_path, relative_path, &metadata)]);
    }
    if !input_path.is_dir() {
        return Err(ScanError::UnsupportedInput(input_path.to_owned()));
    }

    let mut images = Vec::new();
    for entry in WalkDir::new(input_path)
        .into_iter()
        .filter_entry(|entry| !should_skip_entry(entry))
    {
        let entry = entry?;
        if !entry.file_type().is_file() || !is_png(entry.path()) {
            continue;
        }

        let relative_path = entry
            .path()
            .strip_prefix(input_path)
            .unwrap_or_else(|_| entry.path())
            .display()
            .to_string();
        // Windows 上 walkdir 的 metadata 来自目录遍历结果，单次调用同时取大小、修改和创建时间。
        let metadata = entry.metadata()?;
        images.push(source_image(entry.path(), relative_path, &metadata));
    }

    images.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(images)
}

fn source_image(path: &Path, relative_path: String, metadata: &fs::Metadata) -> SourceImage {
    SourceImage {
        absolute_path: path.to_owned(),
        relative_path,
        size: metadata.len(),
        modified_nanos: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|duration| i64::try_from(duration.as_nanos()).ok()),
        created: metadata.created().ok(),
    }
}

fn should_skip_entry(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 || !entry.file_type().is_dir() {
        return false;
    }
    let file_name = entry.file_name().to_string_lossy();
    if file_name.eq_ignore_ascii_case(LEGACY_CACHE_DIR_NAME) {
        return true;
    }
    entry.path().join(LEGACY_OUTPUT_MARKER_FILE_NAME).exists()
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn collects_nested_pngs_in_stable_order_and_skips_legacy_dirs() {
        let root = test_root("scan");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::create_dir_all(root.join(LEGACY_CACHE_DIR_NAME)).unwrap();
        fs::create_dir_all(root.join("old-output")).unwrap();
        fs::write(root.join("b.png"), b"fake").unwrap();
        fs::write(root.join("nested").join("a.png"), b"fake").unwrap();
        fs::write(root.join("not-image.txt"), b"text").unwrap();
        fs::write(root.join(LEGACY_CACHE_DIR_NAME).join("cached.png"), b"fake").unwrap();
        fs::write(
            root.join("old-output").join(LEGACY_OUTPUT_MARKER_FILE_NAME),
            b"marker",
        )
        .unwrap();
        fs::write(root.join("old-output").join("copy.png"), b"fake").unwrap();

        let images = collect_png_files(&root).unwrap();

        let relative: Vec<&str> = images
            .iter()
            .map(|image| image.relative_path.as_str())
            .collect();
        assert_eq!(relative, vec!["b.png", "nested\\a.png"]);
        assert!(images.iter().all(|image| image.size > 0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn accepts_single_png_and_rejects_other_files() {
        let root = test_root("scan-single");
        fs::create_dir_all(&root).unwrap();
        let png = root.join("single.png");
        fs::write(&png, b"fake").unwrap();
        let text = root.join("note.txt");
        fs::write(&text, b"text").unwrap();

        let images = collect_png_files(&png).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].relative_path, "single.png");

        assert!(matches!(
            collect_png_files(&text),
            Err(ScanError::UnsupportedInput(_))
        ));
        let _ = fs::remove_dir_all(&root);
    }

    fn test_root(label: &str) -> PathBuf {
        let parent = Path::new(r"D:\Agent\Agent_temp");
        let parent = if parent.is_dir() {
            parent.to_owned()
        } else {
            std::env::temp_dir()
        };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        parent.join(format!(
            "smart-spreadsheet-scan-{label}-{}-{nonce}",
            std::process::id()
        ))
    }
}
