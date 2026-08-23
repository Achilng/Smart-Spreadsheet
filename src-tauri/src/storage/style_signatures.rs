use serde::Serialize;

use super::{DataDirectory, StorageError};
use crate::pipeline::STYLE_SIGNATURE_VERSION;

/// 画风签名回填进度：total 为待处理行数（全库行数，含无提示词行）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleSignatureProgress {
    pub processed: usize,
    pub total: usize,
}

impl DataDirectory {
    /// 为历史资料库补齐正向提示词的画风签名，并在算法版本落后时全量重算。
    ///
    /// 计算只依赖库内 `positive_prompt` 文本，不读图片文件，数万行在秒级
    /// 完成；结果分批写库，中途失败下次启动续跑（版本标记未写入即重跑）。
    /// 库内 `style_signature_version` 与当前算法版本一致时立即返回。
    pub fn backfill_style_signatures(
        &self,
        progress: impl Fn(StyleSignatureProgress),
    ) -> Result<StyleSignatureProgress, StorageError> {
        /// 分批落库：中断可续跑（版本标记最后写入），内存占用为常数。
        const WRITE_BATCH_ROWS: usize = 500;
        let mut database = self.open_database()?;
        if database.style_signature_version()? == Some(STYLE_SIGNATURE_VERSION) {
            return Ok(StyleSignatureProgress::default());
        }
        let prompts = database.all_row_positive_prompts()?;
        let total = prompts.len();
        progress(StyleSignatureProgress {
            total,
            ..StyleSignatureProgress::default()
        });
        let progress_step = (total / 200).max(1);

        let mut batch: Vec<(i64, Option<String>)> = Vec::with_capacity(WRITE_BATCH_ROWS.min(total.max(1)));
        for (index, (row_id, positive_prompt)) in prompts.into_iter().enumerate() {
            batch.push((
                row_id,
                crate::pipeline::style_signature_of(positive_prompt.as_deref()),
            ));
            if batch.len() >= WRITE_BATCH_ROWS {
                database.update_style_signatures(&batch)?;
                batch.clear();
            }
            let processed = index + 1;
            if processed == total || processed % progress_step == 0 {
                progress(StyleSignatureProgress { processed, total });
            }
        }
        if !batch.is_empty() {
            database.update_style_signatures(&batch)?;
        }
        database.set_style_signature_version(STYLE_SIGNATURE_VERSION)?;
        Ok(StyleSignatureProgress { processed: total, total })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::db::{NewRow, SourceType};

    fn temporary_directory(tag: &str) -> (std::path::PathBuf, DataDirectory) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("smart-spreadsheet-style-{tag}-{nonce}"));
        let directory = DataDirectory::initialize(root.join("data")).unwrap();
        (root, directory)
    }

    fn append_rows(directory: &DataDirectory, rows: &[NewRow]) {
        let mut database = directory.open_database().unwrap();
        database
            .append_batch(SourceType::Folder, r"D:\test", rows, |_| Ok(()))
            .unwrap();
    }

    fn stored_signatures(directory: &DataDirectory) -> Vec<(i64, Option<String>)> {
        let database = directory.open_database().unwrap();
        database.all_row_positive_prompts().unwrap()
    }

    /// 测试辅助：全表重算并与存量签名逐一比对，任何写路径漏刷签名都会在此暴露。
    pub(crate) fn assert_signatures_consistent(directory: &DataDirectory) {
        let stored = stored_signatures(directory);
        assert!(
            stored.iter().all(|(row_id, prompt)| {
                let expected = crate::pipeline::style_signature_of(prompt.as_deref());
                let actual = directory
                    .open_database()
                    .unwrap()
                    .row_style_signature(*row_id)
                    .unwrap();
                actual == expected
            }),
            "存量画风签名与按提示词重算的结果不一致"
        );
    }

    #[test]
    fn backfills_then_is_idempotent() {
        let (root, directory) = temporary_directory("basic");
        append_rows(
            &directory,
            &[
                NewRow {
                    source_ordinal: 1,
                    identity: "file:a.png".into(),
                    positive_prompt: Some("artist:a, blue hair, very aesthetic".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 2,
                    identity: "file:b.png".into(),
                    positive_prompt: Some(", best quality, amazing quality".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 3,
                    identity: "file:c.png".into(),
                    ..NewRow::default()
                },
            ],
        );
        // 模拟 v16 旧库：清空签名与版本标记。
        drop(stored_signatures(&directory));
        {
            let connection = rusqlite::Connection::open(directory.database_path()).unwrap();
            connection
                .execute_batch(
                    "UPDATE rows SET style_signature = NULL;
                     DELETE FROM settings WHERE key = 'style_signature_version';",
                )
                .unwrap();
        }

        let events = std::sync::Mutex::new(Vec::new());
        let outcome = directory
            .backfill_style_signatures(|progress| events.lock().unwrap().push(progress))
            .unwrap();
        assert_eq!(
            outcome,
            StyleSignatureProgress {
                processed: 3,
                total: 3
            }
        );
        let signatures = stored_signatures(&directory);
        // 行 1：剥离质量词后仍有内容，得到签名；行 2：纯质量词 → NULL；行 3：无提示词 → NULL。
        let row1 = directory
            .open_database()
            .unwrap()
            .row_style_signature(1)
            .unwrap();
        assert!(row1.is_some_and(|value| value.len() == 64));
        let connection = rusqlite::Connection::open(directory.database_path()).unwrap();
        let (s2, s3): (Option<String>, Option<String>) = connection
            .query_row(
                "SELECT
                    (SELECT style_signature FROM rows WHERE id = 2),
                    (SELECT style_signature FROM rows WHERE id = 3)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(s2, None);
        assert_eq!(s3, None);
        drop(connection);
        assert_eq!(signatures.len(), 3);

        // 版本标记就位后再次回填立即返回，不重算。
        assert_eq!(
            directory.backfill_style_signatures(|_| {}).unwrap(),
            StyleSignatureProgress::default()
        );
        assert_signatures_consistent(&directory);
        let _ = fs::remove_dir_all(root);
    }

    /// 大批量分批落库且进度事件被稀释。
    #[test]
    fn large_backfill_writes_in_batches() {
        let (root, directory) = temporary_directory("large");
        let rows: Vec<NewRow> = (1..=750)
            .map(|index| NewRow {
                source_ordinal: index,
                identity: format!("file:{index}.png"),
                positive_prompt: Some(format!("artist:a, tag {index}")),
                ..NewRow::default()
            })
            .collect();
        append_rows(&directory, &rows);
        {
            let connection = rusqlite::Connection::open(directory.database_path()).unwrap();
            connection
                .execute_batch(
                    "UPDATE rows SET style_signature = NULL;
                     DELETE FROM settings WHERE key = 'style_signature_version';",
                )
                .unwrap();
        }

        let events = std::sync::Mutex::new(Vec::new());
        let outcome = directory
            .backfill_style_signatures(|progress| events.lock().unwrap().push(progress))
            .unwrap();
        assert_eq!(outcome.processed, 750);
        let events = events.into_inner().unwrap();
        assert!(events.len() < 400, "进度事件过多: {}", events.len());
        assert_eq!(
            events.last().map(|p| (p.processed, p.total)),
            Some((750, 750))
        );
        assert_eq!(
            directory.backfill_style_signatures(|_| {}).unwrap(),
            StyleSignatureProgress::default()
        );
        assert_signatures_consistent(&directory);
        let _ = fs::remove_dir_all(root);
    }

    /// 版本号递增（算法修订）会触发对已就位签名的全量重算。
    #[test]
    fn version_bump_forces_full_recalculation() {
        let (root, directory) = temporary_directory("version");
        append_rows(
            &directory,
            &[NewRow {
                source_ordinal: 1,
                identity: "file:a.png".into(),
                positive_prompt: Some("artist:a, blue hair".into()),
                ..NewRow::default()
            }],
        );
        // 先正常回填。
        directory.backfill_style_signatures(|_| {}).unwrap();
        // 模拟旧版本标记（例如 v1 算法写入的库升级到 v2）。
        {
            let mut database = directory.open_database().unwrap();
            database.set_style_signature_version(0).unwrap();
        }
        // 篡改签名后重算应恢复为正确值。
        {
            let connection = rusqlite::Connection::open(directory.database_path()).unwrap();
            connection
                .execute("UPDATE rows SET style_signature = 'stale'", [])
                .unwrap();
        }
        let outcome = directory.backfill_style_signatures(|_| {}).unwrap();
        assert_eq!(outcome.total, 1);
        assert_signatures_consistent(&directory);
        let _ = fs::remove_dir_all(root);
    }
}
