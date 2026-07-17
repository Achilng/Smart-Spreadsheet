mod delete;
mod content_hash;
mod export_images;
mod export_json;
mod export_xlsx;
mod import_images;
mod metadata_fingerprint;
mod migration;
mod perceptual_hash;
mod prompt_docs;
#[cfg(test)]
pub(crate) mod test_fixtures;

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::{Database, DatabaseError};
use crate::excel::EmbeddedImageRef;

pub use delete::{RowDeletionError, RowDeletionReport};
pub use content_hash::{ContentHashBackfillOutcome, ContentHashProgress};
pub use export_images::{
    ImageFileExportMode, ImageFilesExportError, ImageFilesExportOutcome, ImageFilesProgress,
    OriginalSourceError, resolve_locator_source as resolve_image_source, resolve_original_source,
};
pub use export_json::{JsonExportError, JsonExportOutcome, JsonExportProgress};
pub use export_xlsx::{ExportProgress, XlsxExportError, XlsxExportOutcome};
pub use import_images::{
    ExistingImageUpdateOutcome, ImageImportError, ImageImportOutcome, ImageImportProgress,
    ImageImportStage,
};
pub use migration::{MigrationOutcome, PreparedMigration};
pub use perceptual_hash::{
    PerceptualHashBackfillOutcome, PerceptualHashProgress, SimilarImageMatch,
};
pub use prompt_docs::{PromptDocAsset, PromptDocDetail, PromptDocError, PromptDocSummary};

pub(super) const FORMAT_VERSION: u32 = 1;
pub(super) const MARKER_FILE: &str = ".smart-spreadsheet-data.json";
pub(super) const DATABASE_FILE: &str = "smart-spreadsheet.sqlite3";
const REJECTED_IMAGES_DIRECTORY_SETTING: &str = "rejected_images_directory";

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("数据目录文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("数据目录标记文件无效: {0}")]
    InvalidMarker(#[from] serde_json::Error),
    #[error("数据库初始化失败: {0}")]
    Database(#[from] DatabaseError),
    #[error("数据目录路径不是文件夹: {0}")]
    NotDirectory(PathBuf),
    #[error("非空文件夹不是智能表格受管数据目录: {0}")]
    UnmanagedDirectory(PathBuf),
    #[error("缺少数据目录标记文件: {0}")]
    MissingMarker(PathBuf),
    #[error("数据目录格式版本 {found} 高于当前支持版本 {supported}")]
    UnsupportedFormatVersion { found: u32, supported: u32 },
    #[error("受管数据目录缺少必要路径: {0}")]
    MissingRequiredPath(PathBuf),
    #[error("迁移目标与当前数据目录相同: {0}")]
    SameMigrationDestination(PathBuf),
    #[error("迁移目标位于当前数据目录内部: {0}")]
    DestinationInsideSource(PathBuf),
    #[error("迁移目标文件夹不是空文件夹: {0}")]
    NonEmptyDestination(PathBuf),
    #[error("数据目录包含不支持迁移的符号链接或特殊文件: {0}")]
    UnsupportedEntry(PathBuf),
    #[error("异常图片输出路径不是文件夹: {0}")]
    RejectedImagesPathNotDirectory(PathBuf),
    #[error("迁移文件校验失败: {0}")]
    MigrationVerificationFailed(PathBuf),
    #[error("无法恢复迁移失败前的数据目录标记: {0}")]
    MarkerRestoreFailed(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDirectory {
    root: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct DataDirectoryMarker {
    format_version: u32,
}

impl DataDirectory {
    pub fn initialize(root: impl AsRef<Path>) -> Result<Self, StorageError> {
        let root = root.as_ref();
        if root.exists() && !root.is_dir() {
            return Err(StorageError::NotDirectory(root.to_owned()));
        }

        let marker_path = root.join(MARKER_FILE);
        if marker_path.exists() {
            return Self::open(root);
        }
        if root.exists() && fs::read_dir(root)?.next().is_some() {
            return Err(StorageError::UnmanagedDirectory(root.to_owned()));
        }

        fs::create_dir_all(root)?;
        fs::create_dir_all(root.join("workbook"))?;
        fs::create_dir_all(root.join("files"))?;
        fs::create_dir_all(root.join("prompt-docs"))?;
        fs::create_dir_all(root.join("cache").join("thumbnails"))?;
        fs::create_dir_all(root.join("migration"))?;
        drop(Database::open(root.join(DATABASE_FILE))?);
        write_marker(&marker_path)?;

        Self::open(root)
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::open_with_hash_progress(root, |_| {})
    }

    pub fn open_with_hash_progress(
        root: impl AsRef<Path>,
        progress: impl Fn(ContentHashProgress),
    ) -> Result<Self, StorageError> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(StorageError::NotDirectory(root.to_owned()));
        }

        let marker_path = root.join(MARKER_FILE);
        if !marker_path.is_file() {
            return Err(StorageError::MissingMarker(marker_path));
        }
        let marker: DataDirectoryMarker = serde_json::from_reader(File::open(&marker_path)?)?;
        if marker.format_version > FORMAT_VERSION {
            return Err(StorageError::UnsupportedFormatVersion {
                found: marker.format_version,
                supported: FORMAT_VERSION,
            });
        }

        for required_path in [
            root.join("workbook"),
            root.join("cache").join("thumbnails"),
            root.join("migration"),
        ] {
            if !required_path.is_dir() {
                return Err(StorageError::MissingRequiredPath(required_path));
            }
        }
        let database_path = root.join(DATABASE_FILE);
        if !database_path.is_file() {
            return Err(StorageError::MissingRequiredPath(database_path));
        }
        drop(Database::open(&database_path)?);

        // v1 受管目录没有 files/，打开时按需补建（目录格式版本保持 1）。
        fs::create_dir_all(root.join("files"))?;
        // 提示词文档是文件夹资产，不提升目录格式版本，旧目录打开时补建。
        fs::create_dir_all(root.join("prompt-docs"))?;

        let directory = Self {
            root: root.to_owned(),
        };
        directory.process_pending_embedded_extractions()?;
        directory.backfill_content_hashes(progress)?;
        directory.backfill_metadata_fingerprints()?;
        Ok(directory)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn database_path(&self) -> PathBuf {
        self.root.join(DATABASE_FILE)
    }

    pub fn source_workbook_path(&self) -> PathBuf {
        self.root.join("workbook").join("source.xlsx")
    }

    pub fn files_path(&self) -> PathBuf {
        self.root.join("files")
    }

    pub fn thumbnail_cache_path(&self) -> PathBuf {
        self.root.join("cache").join("thumbnails")
    }

    pub fn migration_path(&self) -> PathBuf {
        self.root.join("migration")
    }

    pub fn open_database(&self) -> Result<Database, StorageError> {
        Ok(Database::open(self.database_path())?)
    }

    pub fn rejected_images_directory(&self) -> Result<Option<PathBuf>, StorageError> {
        Ok(self
            .open_database()?
            .setting(REJECTED_IMAGES_DIRECTORY_SETTING)?
            .map(PathBuf::from))
    }

    pub fn default_rejected_images_directory(&self) -> PathBuf {
        self.root.join("rejected")
    }

    pub fn set_rejected_images_directory(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<PathBuf, StorageError> {
        let path = path.as_ref();
        if path.exists() && !path.is_dir() {
            return Err(StorageError::RejectedImagesPathNotDirectory(path.to_owned()));
        }
        fs::create_dir_all(path)?;
        let path = path.canonicalize()?;
        let display = canonical_display_path(&path);
        self.open_database()?
            .set_setting(REJECTED_IMAGES_DIRECTORY_SETTING, &display)?;
        Ok(PathBuf::from(display))
    }

    pub fn reset_data(&self) -> Result<(), StorageError> {
        let files_dir = self.files_path();
        if files_dir.is_dir() {
            fs::remove_dir_all(&files_dir)?;
            fs::create_dir_all(&files_dir)?;
        }

        let cache_dir = self.thumbnail_cache_path();
        if cache_dir.is_dir() {
            fs::remove_dir_all(&cache_dir)?;
            fs::create_dir_all(&cache_dir)?;
        }

        let workbook_dir = self.root.join("workbook");
        if workbook_dir.is_dir() {
            fs::remove_dir_all(&workbook_dir)?;
            fs::create_dir_all(&workbook_dir)?;
        }

        let db_path = self.database_path();
        let wal = db_path.with_extension("sqlite3-wal");
        let shm = db_path.with_extension("sqlite3-shm");
        let _ = fs::remove_file(&wal);
        let _ = fs::remove_file(&shm);
        fs::remove_file(&db_path)?;
        drop(Database::open(&db_path)?);

        self.set_rejected_images_directory(self.default_rejected_images_directory())?;

        Ok(())
    }

    /// 处理 v1→v2 迁移遗留的嵌入图提取：从旧工作簿副本批量读出嵌入图，
    /// 写入 `files/1/embedded/`（迁移产生的行固定属于批次 1）并更新行记录。
    /// 工作簿副本缺失或读取失败时清空待提取项，相关行失去嵌入图回退但不阻塞打开。
    fn process_pending_embedded_extractions(&self) -> Result<(), StorageError> {
        let mut database = self.open_database()?;
        let pending = database.pending_embedded_extractions()?;
        if pending.is_empty() {
            return Ok(());
        }

        let workbook = self.source_workbook_path();
        let mut results: Vec<(i64, Option<String>)> =
            pending.iter().map(|(row_id, _)| (*row_id, None)).collect();

        if workbook.is_file() {
            let target_dir = self.files_path().join("1").join("embedded");
            fs::create_dir_all(&target_dir)?;
            let references = pending
                .iter()
                .map(|(_, media_path)| EmbeddedImageRef {
                    source_row: 0,
                    source_column: 0,
                    media_path: media_path.clone(),
                })
                .collect::<Vec<_>>();
            let extraction = crate::excel::extract_embedded_images(
                &workbook,
                &references,
                |index, image, bytes| {
                    let row_id = pending[index].0;
                    let extension = media_extension(&image.media_path);
                    let file_name = format!("row-{row_id}.{extension}");
                    fs::write(target_dir.join(&file_name), bytes)?;
                    results[index].1 = Some(format!("files/1/embedded/{file_name}"));
                    Ok(())
                },
            );
            // 工作簿副本损坏时按“无嵌入图”降级处理，不阻塞数据目录打开。
            if extraction.is_err() {
                for result in &mut results {
                    if let Some(stored) = result.1.take() {
                        let _ = fs::remove_file(self.root.join(stored));
                    }
                }
            }
        }

        database.resolve_pending_embedded_extractions(&results)?;
        Ok(())
    }
}

/// 规范化展示路径：尽量使用绝对路径并去掉 Windows `\\?\` 前缀。
pub(super) fn canonical_display_path(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
    let text = canonical.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}

/// 受管 `files/` 下的暂存目录守卫：导入失败或回滚时清理残留；
/// 成功后随批次 ID 改名归位，守卫析构时残留的空目录无害。
pub(super) struct StagingDir {
    path: PathBuf,
}

impl StagingDir {
    pub(super) fn create(files_root: &Path) -> Result<Self, std::io::Error> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = files_root.join(format!(".staging-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(super) fn media_extension(media_path: &str) -> String {
    let extension = media_path
        .rsplit('.')
        .next()
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= 8
                && extension.chars().all(|c| c.is_ascii_alphanumeric())
        })
        .unwrap_or("png");
    extension.to_ascii_lowercase()
}

fn write_marker(path: &Path) -> Result<(), StorageError> {
    let marker = DataDirectoryMarker {
        format_version: FORMAT_VERSION,
    };
    let contents = serde_json::to_vec_pretty(&marker)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&contents)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn initializes_and_reopens_managed_directory() {
        let temporary = TemporaryDirectory::new("initialize");

        let initialized = DataDirectory::initialize(&temporary.path).unwrap();
        let reopened = DataDirectory::initialize(&temporary.path).unwrap();

        assert_eq!(initialized, reopened);
        assert!(initialized.database_path().is_file());
        assert!(
            initialized
                .source_workbook_path()
                .parent()
                .unwrap()
                .is_dir()
        );
        assert!(initialized.thumbnail_cache_path().is_dir());
        assert!(initialized.migration_path().is_dir());
        assert_eq!(
            initialized
                .open_database()
                .unwrap()
                .schema_version()
                .unwrap(),
            crate::db::CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn rejects_nonempty_unmanaged_directory() {
        let temporary = TemporaryDirectory::new("unmanaged");
        fs::create_dir_all(&temporary.path).unwrap();
        fs::write(temporary.path.join("unrelated.txt"), b"keep").unwrap();

        let error = DataDirectory::initialize(&temporary.path).unwrap_err();

        assert!(matches!(error, StorageError::UnmanagedDirectory(path) if path == temporary.path));
        assert_eq!(
            fs::read(temporary.path.join("unrelated.txt")).unwrap(),
            b"keep"
        );
    }

    #[test]
    fn rejects_newer_data_directory_format() {
        let temporary = TemporaryDirectory::new("future-format");
        let directory = DataDirectory::initialize(&temporary.path).unwrap();
        fs::write(
            directory.root().join(MARKER_FILE),
            br#"{"format_version":999}"#,
        )
        .unwrap();

        let error = DataDirectory::open(&temporary.path).unwrap_err();

        assert!(matches!(
            error,
            StorageError::UnsupportedFormatVersion {
                found: 999,
                supported: FORMAT_VERSION
            }
        ));
    }

    struct TemporaryDirectory {
        path: PathBuf,
    }

    impl TemporaryDirectory {
        fn new(label: &str) -> Self {
            let local_agent_temp = Path::new(r"D:\Agent\Agent_temp");
            let directory = if local_agent_temp.is_dir() {
                local_agent_temp.to_owned()
            } else {
                std::env::temp_dir()
            };
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            Self {
                path: directory.join(format!(
                    "smart-spreadsheet-storage-{label}-{}-{nonce}",
                    std::process::id()
                )),
            }
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
