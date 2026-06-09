use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::migration::files_equal;
use super::{DataDirectory, StorageError};
use crate::db::DatabaseError;
use crate::excel::{ImageMapError, ImportError, map_embedded_images, read_fixed_workbook};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    pub sheet_name: String,
    pub row_count: usize,
    pub embedded_image_count: usize,
    pub previous_copy_cleanup: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum WorkbookImportError {
    #[error("导入文件必须是 .xlsx: {0}")]
    InvalidExtension(PathBuf),
    #[error("导入文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("应用数据目录不可用: {0}")]
    Storage(#[from] StorageError),
    #[error("Excel 解析失败: {0}")]
    Excel(#[from] ImportError),
    #[error("嵌入图片解析失败: {0}")]
    Images(#[from] ImageMapError),
    #[error("数据库写入失败: {0}")]
    Database(#[from] DatabaseError),
    #[error("导入副本字节校验失败: {0}")]
    CopyVerificationFailed(PathBuf),
    #[error("导入失败且无法恢复此前工作簿副本: {0}")]
    RollbackFailed(PathBuf),
}

impl DataDirectory {
    pub fn import_workbook(
        &self,
        source: impl AsRef<Path>,
    ) -> Result<ImportOutcome, WorkbookImportError> {
        let source = source.as_ref();
        let is_xlsx = source
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xlsx"));
        if !is_xlsx {
            return Err(WorkbookImportError::InvalidExtension(source.to_owned()));
        }

        let (staging, mut staging_file) = create_workbook_temporary_file(self, "importing")?;
        let mut staging_guard = FileGuard::new(staging.clone());
        let mut source_file = File::open(source)?;
        io::copy(&mut source_file, &mut staging_file)?;
        staging_file.sync_all()?;
        drop(staging_file);
        if !files_equal(source, &staging)? {
            return Err(WorkbookImportError::CopyVerificationFailed(staging));
        }

        let parsed = read_fixed_workbook(&staging)?;
        let images = map_embedded_images(&staging, &parsed.sheet_name)?;
        let imported_name = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "imported.xlsx".to_owned());
        let current = self.source_workbook_path();
        let backup = if current.exists() {
            let backup = available_workbook_temporary_path(self, "previous")?;
            fs::rename(&current, &backup)?;
            Some(backup)
        } else {
            None
        };

        if let Err(error) = fs::rename(&staging, &current) {
            restore_previous_copy(&current, backup.as_deref())?;
            return Err(error.into());
        }
        staging_guard.disarm();

        let database_result = (|| {
            let mut database = self.open_database()?;
            database.replace_workbook(&imported_name, &parsed, &images)?;
            Ok::<(), WorkbookImportError>(())
        })();
        if let Err(error) = database_result {
            restore_previous_copy(&current, backup.as_deref())?;
            return Err(error);
        }

        let previous_copy_cleanup = backup.and_then(|path| match fs::remove_file(&path) {
            Ok(()) => None,
            Err(_) => Some(path),
        });

        Ok(ImportOutcome {
            sheet_name: parsed.sheet_name,
            row_count: parsed.rows.len(),
            embedded_image_count: images.len(),
            previous_copy_cleanup,
        })
    }
}

fn create_workbook_temporary_file(
    directory: &DataDirectory,
    label: &str,
) -> Result<(PathBuf, File), WorkbookImportError> {
    let workbook_directory = directory
        .source_workbook_path()
        .parent()
        .expect("managed workbook path must have a parent")
        .to_owned();
    for attempt in 0..100_u32 {
        let path = workbook_directory.join(format!(
            ".source.xlsx.{label}-{}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(WorkbookImportError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "无法创建唯一的工作簿暂存路径",
    )))
}

fn available_workbook_temporary_path(
    directory: &DataDirectory,
    label: &str,
) -> Result<PathBuf, WorkbookImportError> {
    let workbook_directory = directory
        .source_workbook_path()
        .parent()
        .expect("managed workbook path must have a parent")
        .to_owned();
    for attempt in 0..100_u32 {
        let path = workbook_directory.join(format!(
            ".source.xlsx.{label}-{}-{attempt}",
            std::process::id()
        ));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(WorkbookImportError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "无法创建唯一的工作簿备份路径",
    )))
}

fn restore_previous_copy(current: &Path, backup: Option<&Path>) -> Result<(), WorkbookImportError> {
    if current.exists() && fs::remove_file(current).is_err() {
        return Err(WorkbookImportError::RollbackFailed(current.to_owned()));
    }
    if let Some(backup) = backup
        && fs::rename(backup, current).is_err()
    {
        return Err(WorkbookImportError::RollbackFailed(current.to_owned()));
    }
    Ok(())
}

struct FileGuard {
    path: PathBuf,
    active: bool,
}

impl FileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, active: true }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use super::*;

    #[test]
    fn imports_sample_into_managed_copy_and_database() {
        let temporary = TemporaryImport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let source = sample_workbook();
        let source_before = fs::read(&source).unwrap();

        let outcome = directory.import_workbook(&source).unwrap();

        assert_eq!(outcome.sheet_name, "NovelAI Metadata");
        assert_eq!(outcome.row_count, 5);
        assert_eq!(outcome.embedded_image_count, 5);
        assert!(outcome.previous_copy_cleanup.is_none());
        assert_eq!(fs::read(&source).unwrap(), source_before);
        assert_eq!(
            fs::read(directory.source_workbook_path()).unwrap(),
            source_before
        );

        let connection = Connection::open(directory.database_path()).unwrap();
        let stored: (u32, u32) = connection
            .query_row(
                "SELECT workbook.row_count,
                    (SELECT COUNT(*) FROM rows WHERE embedded_image_ref IS NOT NULL)
                 FROM workbook",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (5, 5));
    }

    #[test]
    fn malformed_replacement_keeps_previous_workbook_and_database() {
        let temporary = TemporaryImport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        directory.import_workbook(sample_workbook()).unwrap();
        let workbook_before = fs::read(directory.source_workbook_path()).unwrap();
        let invalid = temporary.root.join("invalid.xlsx");
        fs::write(&invalid, b"not an xlsx file").unwrap();

        assert!(directory.import_workbook(&invalid).is_err());

        assert_eq!(
            fs::read(directory.source_workbook_path()).unwrap(),
            workbook_before
        );
        let connection = Connection::open(directory.database_path()).unwrap();
        let stored: (String, u32) = connection
            .query_row("SELECT imported_name, row_count FROM workbook", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(stored, ("novelai_metadata.xlsx".into(), 5));
    }

    fn sample_workbook() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("Examples")
            .join("novelai_metadata.xlsx")
    }

    struct TemporaryImport {
        root: PathBuf,
        data: PathBuf,
    }

    impl TemporaryImport {
        fn new() -> Self {
            let local_agent_temp = Path::new(r"D:\Agent\Agent_temp");
            let parent = if local_agent_temp.is_dir() {
                local_agent_temp.to_owned()
            } else {
                std::env::temp_dir()
            };
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            let root = parent.join(format!(
                "smart-spreadsheet-import-{}-{nonce}",
                std::process::id()
            ));
            Self {
                data: root.join("data"),
                root,
            }
        }
    }

    impl Drop for TemporaryImport {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
