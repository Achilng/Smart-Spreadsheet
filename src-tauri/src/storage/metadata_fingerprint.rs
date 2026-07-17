use super::{DataDirectory, StorageError, resolve_original_source};
use crate::pipeline::{metadata_fingerprint, png_text};

impl DataDirectory {
    /// 为仍可读取完整原件的历史行回填 NovelAI 完整元数据指纹。
    ///
    /// 历史缩略图未标记为完整原件，因此不会被误用于回填。
    pub fn backfill_metadata_fingerprints(&self) -> Result<usize, StorageError> {
        let mut database = self.open_database()?;
        let candidates = database.missing_metadata_fingerprints()?;
        let mut fingerprints = Vec::new();

        for locator in candidates {
            let Ok(path) = resolve_original_source(self, &locator) else {
                continue;
            };
            let Ok(chunks) = png_text::read_png_text_chunks(path) else {
                continue;
            };
            if let Some(fingerprint) = metadata_fingerprint(&chunks) {
                fingerprints.push((locator.row_id, fingerprint));
            }
        }

        database.update_metadata_fingerprints(&fingerprints)?;
        Ok(fingerprints.len())
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
    fn backfills_from_managed_original_but_not_legacy_thumbnail() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("smart-spreadsheet-metadata-fingerprint-{nonce}"));
        let data = root.join("data");
        let directory = DataDirectory::initialize(&data).unwrap();

        let original_relative = "files/1/original.png";
        let original = data.join(original_relative);
        fs::create_dir_all(original.parent().unwrap()).unwrap();
        fs::write(
            &original,
            metadata_png_bytes("artist:test", Some(r#"{"seed":7,"steps":28}"#)),
        )
        .unwrap();

        let thumbnail_relative = "files/2/thumbnail.png";
        let thumbnail = data.join(thumbnail_relative);
        fs::create_dir_all(thumbnail.parent().unwrap()).unwrap();
        fs::write(&thumbnail, metadata_png_bytes("artist:test", None)).unwrap();

        let mut database = directory.open_database().unwrap();
        database
            .append_batch(
                SourceType::Folder,
                r"D:\missing",
                &[NewRow {
                    source_ordinal: 1,
                    identity: "file:d:\\missing\\original.png".into(),
                    image_path: Some(r"D:\missing\original.png".into()),
                    stored_image_rel: Some("original.png".into()),
                    ..NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();
        database
            .append_batch(
                SourceType::Legacy,
                "旧版导入来源",
                &[NewRow {
                    source_ordinal: 1,
                    identity: "legacy-row:1".into(),
                    image_path: Some(r"D:\missing\legacy.png".into()),
                    stored_image_rel: Some("thumbnail.png".into()),
                    ..NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();
        drop(database);

        assert_eq!(directory.backfill_metadata_fingerprints().unwrap(), 1);
        let remaining = directory
            .open_database()
            .unwrap()
            .missing_metadata_fingerprints()
            .unwrap();
        assert_eq!(
            remaining
                .into_iter()
                .map(|row| row.row_id)
                .collect::<Vec<_>>(),
            vec![2]
        );

        let _ = fs::remove_dir_all(root);
    }
}
