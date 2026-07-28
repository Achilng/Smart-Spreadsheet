use super::{resolve_image_source, DataDirectory, StorageError};
use crate::pipeline::{png_text, vibe_status};

impl DataDirectory {
    /// 为历史资料库建立 VIBE 引用数量与组合签名索引：
    /// 数量缺失的行（v10 及更早）与有引用但签名缺失的行（v15 及更早）都会补齐。
    ///
    /// 文件缺失或元数据不可读时记为 0/None，避免每次启动重复扫描；后续通过
    /// “更新现有图片”重新关联原图时会写入最新的准确数量。
    pub fn backfill_vibe_statuses(&self) -> Result<usize, StorageError> {
        let mut database = self.open_database()?;
        let candidates = database.missing_vibe_statuses()?;
        let mut statuses = Vec::with_capacity(candidates.len());

        for locator in candidates {
            let status = resolve_image_source(self, &locator)
                .and_then(|path| png_text::read_png_text_chunks(path).ok())
                .map(|chunks| vibe_status(&chunks));
            // 文件不可读时保留既有数量（无则记 0），签名写空串标记“已扫描”，
            // 避免每次启动重复尝试；空串不参与“按 VIBE”聚合。
            let (count, signature) = match status {
                Some((count, signature)) => (Some(count), signature),
                None => (None, Some(String::new())),
            };
            statuses.push((locator.row_id, count, signature));
        }

        database.update_vibe_statuses(&statuses)?;
        Ok(statuses.len())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::db::{NewRow, SourceType};
    use crate::storage::test_fixtures::metadata_png_bytes;

    #[test]
    fn backfills_vibe_counts_and_marks_missing_files_as_zero() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("smart-spreadsheet-vibe-{nonce}"));
        let data = root.join("data");
        let directory = DataDirectory::initialize(&data).unwrap();

        let original = data.join("files/1/original.png");
        fs::create_dir_all(original.parent().unwrap()).unwrap();
        fs::write(
            &original,
            metadata_png_bytes(
                "artist:test",
                Some(r#"{"reference_image_multiple":[{},{}]}"#),
            ),
        )
        .unwrap();

        let mut database = directory.open_database().unwrap();
        database
            .append_batch(
                SourceType::Folder,
                r"D:\missing",
                &[
                    NewRow {
                        source_ordinal: 1,
                        identity: "file:d:\\missing\\original.png".into(),
                        stored_image_rel: Some("original.png".into()),
                        ..NewRow::default()
                    },
                    NewRow {
                        source_ordinal: 2,
                        identity: "file:d:\\missing\\gone.png".into(),
                        ..NewRow::default()
                    },
                ],
                |_| Ok(()),
            )
            .unwrap();
        drop(database);
        rusqlite::Connection::open(directory.database_path())
            .unwrap()
            .execute(
                "UPDATE rows SET vibe_reference_count = NULL, vibe_signature = NULL",
                [],
            )
            .unwrap();

        assert_eq!(directory.backfill_vibe_statuses().unwrap(), 2);
        let database = directory.open_database().unwrap();
        assert_eq!(database.row_vibe_reference_count(1).unwrap(), Some(2));
        assert_eq!(database.row_vibe_reference_count(2).unwrap(), Some(0));
        drop(database);

        let connection = rusqlite::Connection::open(directory.database_path()).unwrap();
        let signature: Option<String> = connection
            .query_row("SELECT vibe_signature FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(signature.is_some_and(|value| value.len() == 64));
        let gone_signature: Option<String> = connection
            .query_row("SELECT vibe_signature FROM rows WHERE id = 2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(gone_signature.as_deref(), Some(""));
        drop(connection);

        // 再次回填不应重复扫描：所有行的数量与签名标记都已就位。
        assert_eq!(directory.backfill_vibe_statuses().unwrap(), 0);

        let _ = fs::remove_dir_all(root);
    }

    /// v15 库有数量但缺签名：只回填有 VIBE 的行，签名与数量一致。
    #[test]
    fn backfills_signatures_for_rows_with_existing_counts() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("smart-spreadsheet-vibe-sig-{nonce}"));
        let data = root.join("data");
        let directory = DataDirectory::initialize(&data).unwrap();

        let original = data.join("files/1/original.png");
        fs::create_dir_all(original.parent().unwrap()).unwrap();
        fs::write(
            &original,
            metadata_png_bytes(
                "artist:test",
                Some(r#"{"reference_image_multiple":["AAA","BBB"]}"#),
            ),
        )
        .unwrap();

        let mut database = directory.open_database().unwrap();
        database
            .append_batch(
                SourceType::Folder,
                r"D:\images",
                &[NewRow {
                    source_ordinal: 1,
                    identity: "file:d:\\images\\original.png".into(),
                    stored_image_rel: Some("original.png".into()),
                    vibe_reference_count: 2,
                    ..NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();
        drop(database);
        rusqlite::Connection::open(directory.database_path())
            .unwrap()
            .execute("UPDATE rows SET vibe_signature = NULL", [])
            .unwrap();

        assert_eq!(directory.backfill_vibe_statuses().unwrap(), 1);
        let connection = rusqlite::Connection::open(directory.database_path()).unwrap();
        let (count, signature): (Option<u32>, Option<String>) = connection
            .query_row(
                "SELECT vibe_reference_count, vibe_signature FROM rows WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, Some(2));
        assert!(signature.is_some_and(|value| value.len() == 64));

        let _ = fs::remove_dir_all(root);
    }
}
