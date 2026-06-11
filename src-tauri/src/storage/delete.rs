use std::collections::HashSet;
use std::fs;

use thiserror::Error;

use super::{DataDirectory, StorageError};
use crate::db::{RowSelection, TagMutationError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowDeletionReport {
    pub deleted_rows: u64,
    /// 删除的受管副本文件数。
    pub removed_files: usize,
    /// 清理失败的文件数（数据库行已删除，仅文件残留）。
    pub cleanup_failures: usize,
}

#[derive(Debug, Error)]
pub enum RowDeletionError {
    #[error("应用数据目录不可用: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Selection(#[from] TagMutationError),
}

impl DataDirectory {
    /// 删除选中的行：数据库删除在单事务内完成；受管副本和缩略图缓存
    /// 随后尽力清理，清理失败只计数，不影响删除结果。
    pub fn delete_rows(
        &self,
        selection: &RowSelection,
    ) -> Result<RowDeletionReport, RowDeletionError> {
        let mut database = self.open_database()?;
        let outcome = database.delete_rows(selection)?;
        drop(database);

        let mut removed_files = 0;
        let mut cleanup_failures = 0;
        for relative in &outcome.stored_image_paths {
            let path = self.root().join(relative);
            match fs::remove_file(&path) {
                Ok(()) => removed_files += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => cleanup_failures += 1,
            }
        }

        // 缩略图缓存文件名形如 row-<行ID>-<哈希>.png，一次目录扫描按行 ID 清理。
        let deleted_ids: HashSet<i64> = outcome.deleted_row_ids.iter().copied().collect();
        if !deleted_ids.is_empty()
            && let Ok(entries) = fs::read_dir(self.thumbnail_cache_path())
        {
            for entry in entries.filter_map(Result::ok) {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let Some(row_id) = name
                    .strip_prefix("row-")
                    .and_then(|rest| rest.split('-').next())
                    .and_then(|id| id.parse::<i64>().ok())
                else {
                    continue;
                };
                if deleted_ids.contains(&row_id) && fs::remove_file(entry.path()).is_err() {
                    cleanup_failures += 1;
                }
            }
        }

        Ok(RowDeletionReport {
            deleted_rows: outcome.deleted_rows,
            removed_files,
            cleanup_failures,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn deletes_rows_with_stored_copies_and_thumbnails() {
        let temporary = TemporaryDelete::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let outcome = directory.import_workbook(sample_workbook()).unwrap();
        assert_eq!(outcome.added, 5);

        // 行 1 的受管副本与伪造的缩略图缓存。
        let stored = directory
            .open_database()
            .unwrap()
            .row_image_locator(1)
            .unwrap()
            .stored_image_path
            .unwrap();
        let stored_path = directory.root().join(&stored);
        assert!(stored_path.is_file());
        let thumbnail = directory
            .thumbnail_cache_path()
            .join("row-1-0123456789abcdef.png");
        fs::write(&thumbnail, b"\x89PNG\r\n\x1a\nfake").unwrap();
        let unrelated = directory
            .thumbnail_cache_path()
            .join("row-2-0123456789abcdef.png");
        fs::write(&unrelated, b"\x89PNG\r\n\x1a\nfake").unwrap();

        let report = directory
            .delete_rows(&RowSelection::Explicit { row_ids: vec![1] })
            .unwrap();

        assert_eq!(report.deleted_rows, 1);
        assert_eq!(report.removed_files, 1);
        assert_eq!(report.cleanup_failures, 0);
        assert!(!stored_path.exists());
        assert!(!thumbnail.exists());
        assert!(unrelated.exists());
        let database = directory.open_database().unwrap();
        assert_eq!(database.library_summary().unwrap().row_count, 4);
    }

    fn sample_workbook() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("Examples")
            .join("novelai_metadata.xlsx")
    }

    struct TemporaryDelete {
        root: PathBuf,
        data: PathBuf,
    }

    impl TemporaryDelete {
        fn new() -> Self {
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
            let root = parent.join(format!(
                "smart-spreadsheet-delete-{}-{nonce}",
                std::process::id()
            ));
            Self {
                data: root.join("data"),
                root,
            }
        }
    }

    impl Drop for TemporaryDelete {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
