use super::{resolve_image_source, DataDirectory, StorageError};
use crate::pipeline::{png_text, vibe_reference_count};

impl DataDirectory {
    /// 为 v10 及更早资料库中的历史图片一次性建立 VIBE 引用数量索引。
    ///
    /// 文件缺失或元数据不可读时记为 0，避免每次启动重复扫描；后续通过
    /// “更新现有图片”重新关联原图时会写入最新的准确数量。
    pub fn backfill_vibe_statuses(&self) -> Result<usize, StorageError> {
        let mut database = self.open_database()?;
        let candidates = database.missing_vibe_statuses()?;
        let mut statuses = Vec::with_capacity(candidates.len());

        for locator in candidates {
            let count = resolve_image_source(self, &locator)
                .and_then(|path| png_text::read_png_text_chunks(path).ok())
                .map_or(0, |chunks| vibe_reference_count(&chunks));
            statuses.push((locator.row_id, count));
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
            .execute("UPDATE rows SET vibe_reference_count = NULL", [])
            .unwrap();

        assert_eq!(directory.backfill_vibe_statuses().unwrap(), 2);
        let database = directory.open_database().unwrap();
        assert_eq!(database.row_vibe_reference_count(1).unwrap(), Some(2));
        assert_eq!(database.row_vibe_reference_count(2).unwrap(), Some(0));

        let _ = fs::remove_dir_all(root);
    }
}
