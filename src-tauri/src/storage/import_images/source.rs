use super::progress::ProgressReporter;
use super::types::{ImageImportError, ImageImportProgress, ImageImportStage};
use crate::db::SourceType;
use crate::pipeline::archive::{archive_extension, extract_archive};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 运行临时目录（压缩包解压用），位于 `D:\Agent\Agent_temp`，结束时清理。
pub(super) struct RunTempDir {
    pub(super) path: PathBuf,
}

impl RunTempDir {
    pub(super) fn create() -> Result<Self, std::io::Error> {
        let parent = Path::new(r"D:\Agent\Agent_temp");
        let parent = if parent.is_dir() {
            parent.join("smart-spreadsheet-import")
        } else {
            std::env::temp_dir().join("smart-spreadsheet-import")
        };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = parent.join(format!("{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RunTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(super) fn prepare_source(
    input: &Path,
    reporter: &ProgressReporter<impl Fn(ImageImportProgress) + Sync>,
) -> Result<(PathBuf, SourceType, Option<RunTempDir>), ImageImportError> {
    let is_archive = input.is_file() && archive_extension(input).is_some();
    let mut run_temp: Option<RunTempDir> = None;
    let (scan_root, source_type) = if is_archive {
        reporter.emit(ImageImportStage::Extracting, 0, 0, true);
        let temp = RunTempDir::create()?;
        let extract_dir = temp.path().join("archive");
        extract_archive(input, &extract_dir)?;
        run_temp = Some(temp);
        (extract_dir, SourceType::Archive)
    } else {
        (input.to_owned(), SourceType::Folder)
    };
    Ok((scan_root, source_type, run_temp))
}

pub(super) fn source_identity_root(
    input: &Path,
    input_display: &str,
    source_type: SourceType,
) -> String {
    if source_type == SourceType::Archive {
        input_display.to_owned()
    } else if input.is_file() {
        // 单 PNG：display 即文件本身，identity 基于其所在目录拼文件名。
        Path::new(&input_display)
            .parent()
            .map(|parent| parent.display().to_string())
            .unwrap_or_else(|| input_display.to_owned())
    } else {
        input_display.to_owned()
    }
}
