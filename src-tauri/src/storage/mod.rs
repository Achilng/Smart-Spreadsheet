mod migration;

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::{Database, DatabaseError};

pub use migration::MigrationOutcome;

pub(super) const FORMAT_VERSION: u32 = 1;
pub(super) const MARKER_FILE: &str = ".smart-spreadsheet-data.json";
pub(super) const DATABASE_FILE: &str = "smart-spreadsheet.sqlite3";

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
        fs::create_dir_all(root.join("cache").join("thumbnails"))?;
        fs::create_dir_all(root.join("migration"))?;
        drop(Database::open(root.join(DATABASE_FILE))?);
        write_marker(&marker_path)?;

        Self::open(root)
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self, StorageError> {
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

        Ok(Self {
            root: root.to_owned(),
        })
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

    pub fn thumbnail_cache_path(&self) -> PathBuf {
        self.root.join("cache").join("thumbnails")
    }

    pub fn migration_path(&self) -> PathBuf {
        self.root.join("migration")
    }

    pub fn open_database(&self) -> Result<Database, StorageError> {
        Ok(Database::open(self.database_path())?)
    }
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
