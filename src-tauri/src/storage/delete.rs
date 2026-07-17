use std::collections::HashSet;
use std::fs;
use std::path::Path;

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
    /// 成功移入系统回收站的原始图片文件数。
    pub trashed_original_files: usize,
    /// 原始图片移入回收站失败的文件数（数据库行仍已删除）。
    pub original_file_failures: usize,
    /// 压缩包来源没有独立原文件，勾选删除原图时跳过的行数。
    pub archive_rows_skipped: usize,
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
        trash_originals: bool,
    ) -> Result<RowDeletionReport, RowDeletionError> {
        self.delete_rows_with(selection, trash_originals, |path| trash::delete(path).is_ok())
    }

    fn delete_rows_with(
        &self,
        selection: &RowSelection,
        trash_originals: bool,
        mut trash_file: impl FnMut(&Path) -> bool,
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

        let mut trashed_original_files = 0;
        let mut original_file_failures = 0;
        if trash_originals {
            let mut seen = HashSet::new();
            for original in &outcome.original_image_paths {
                if !seen.insert(original) {
                    continue;
                }
                if trash_file(Path::new(original)) {
                    trashed_original_files += 1;
                } else {
                    original_file_failures += 1;
                }
            }
        }

        // 派生图片缓存统一以 row-<行ID>- 开头，一次目录扫描按行 ID 清理。
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
            trashed_original_files,
            original_file_failures,
            archive_rows_skipped: if trash_originals {
                outcome.archive_rows
            } else {
                0
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::storage::test_fixtures;

    #[test]
    fn deletes_rows_with_stored_copies_and_thumbnails() {
        let temporary = TemporaryDelete::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let folder = test_fixtures::sample_image_folder(&temporary.root, 5);
        let outcome = directory.import_images(&folder, |_| {}).unwrap();
        assert_eq!(outcome.added, 5);

        // 行 1 的外部图片 + 文件夹导入生成的受管副本，加伪造的缩略图缓存。
        let external = directory
            .open_database()
            .unwrap()
            .row_image_locator(1)
            .unwrap()
            .image_path
            .unwrap();
        assert!(PathBuf::from(&external).is_file());
        let thumbnail = directory
            .thumbnail_cache_path()
            .join("row-1-0123456789abcdef.png");
        fs::write(&thumbnail, b"\x89PNG\r\n\x1a\nfake").unwrap();
        let unrelated = directory
            .thumbnail_cache_path()
            .join("row-2-0123456789abcdef.png");
        fs::write(&unrelated, b"\x89PNG\r\n\x1a\nfake").unwrap();

        let report = directory
            .delete_rows(&RowSelection::Explicit { row_ids: vec![1] }, false)
            .unwrap();

        assert_eq!(report.deleted_rows, 1);
        assert_eq!(report.removed_files, 1);
        assert_eq!(report.cleanup_failures, 0);
        assert_eq!(report.trashed_original_files, 0);
        assert!(!thumbnail.exists());
        assert!(unrelated.exists());
        let database = directory.open_database().unwrap();
        assert_eq!(database.library_summary().unwrap().row_count, 4);
    }

    #[test]
    fn trashes_folder_originals_and_reports_failures_and_archive_skips() {
        let temporary = TemporaryDelete::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let successful = temporary.root.join("successful.png");
        let failed = temporary.root.join("failed.png");
        let mut database = directory.open_database().unwrap();
        database
            .append_batch(
                crate::db::SourceType::Folder,
                temporary.root.to_string_lossy().as_ref(),
                &[
                    crate::db::NewRow {
                        source_ordinal: 1,
                        identity: "file:successful".into(),
                        image_path: Some(successful.to_string_lossy().into_owned()),
                        ..crate::db::NewRow::default()
                    },
                    crate::db::NewRow {
                        source_ordinal: 2,
                        identity: "file:failed".into(),
                        image_path: Some(failed.to_string_lossy().into_owned()),
                        ..crate::db::NewRow::default()
                    },
                ],
                |_| Ok(()),
            )
            .unwrap();
        database
            .append_batch(
                crate::db::SourceType::Archive,
                r"D:\pack.zip",
                &[crate::db::NewRow {
                    source_ordinal: 1,
                    identity: "archive:pack!image.png".into(),
                    ..crate::db::NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();
        drop(database);
        let mut attempted = Vec::new();

        let report = directory
            .delete_rows_with(
                &RowSelection::Explicit {
                    row_ids: vec![1, 2, 3],
                },
                true,
                |path| {
                    attempted.push(path.to_owned());
                    path == successful
                },
            )
            .unwrap();

        assert_eq!(attempted, vec![successful, failed]);
        assert_eq!(report.deleted_rows, 3);
        assert_eq!(report.trashed_original_files, 1);
        assert_eq!(report.original_file_failures, 1);
        assert_eq!(report.archive_rows_skipped, 1);
        assert_eq!(directory.open_database().unwrap().library_summary().unwrap().row_count, 0);
    }

    #[test]
    fn leaves_folder_original_untouched_when_trash_option_is_disabled() {
        let temporary = TemporaryDelete::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let original = temporary.root.join("keep-original.png");
        fs::write(&original, b"original bytes").unwrap();
        let mut database = directory.open_database().unwrap();
        database
            .append_batch(
                crate::db::SourceType::Folder,
                temporary.root.to_string_lossy().as_ref(),
                &[crate::db::NewRow {
                    source_ordinal: 1,
                    identity: "file:keep-original".into(),
                    image_path: Some(original.to_string_lossy().into_owned()),
                    ..crate::db::NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();
        drop(database);
        let mut trash_calls = 0;

        let report = directory
            .delete_rows_with(
                &RowSelection::Explicit { row_ids: vec![1] },
                false,
                |_| {
                    trash_calls += 1;
                    true
                },
            )
            .unwrap();

        assert_eq!(report.deleted_rows, 1);
        assert_eq!(trash_calls, 0);
        assert!(original.is_file());
        assert_eq!(fs::read(original).unwrap(), b"original bytes");
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
