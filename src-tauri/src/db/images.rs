use rusqlite::{OptionalExtension, params};

use super::{Database, DatabaseError, SourceType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowImageLocator {
    pub row_id: i64,
    pub image_path: Option<String>,
    /// 受管副本相对数据目录根的路径（压缩包提取副本或 xlsx 嵌入图）。
    pub stored_image_path: Option<String>,
    /// 所属批次的来源类型：压缩包副本是完整原件，xlsx 嵌入图只是无元数据缩略图。
    pub source_type: SourceType,
}

impl Database {
    pub fn row_image_locator(&self, row_id: i64) -> Result<RowImageLocator, DatabaseError> {
        self.connection
            .query_row(
                "SELECT rows.id, rows.image_path, rows.stored_image_path, batches.source_type
                 FROM rows JOIN import_batches AS batches ON batches.id = rows.batch_id
                 WHERE rows.id = ?1",
                [row_id],
                |row| {
                    Ok((
                        RowImageLocator {
                            row_id: row.get(0)?,
                            image_path: row.get(1)?,
                            stored_image_path: row.get(2)?,
                            source_type: SourceType::Folder,
                        },
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or(DatabaseError::RowNotFound(row_id))
            .and_then(|(mut locator, source_type)| {
                locator.source_type = SourceType::from_str(&source_type)?;
                Ok(locator)
            })
    }

    /// v1→v2 迁移遗留的待提取嵌入图（行 ID 升序）。
    pub fn pending_embedded_extractions(&self) -> Result<Vec<(i64, String)>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT row_id, media_path FROM pending_embedded_extractions ORDER BY row_id",
        )?;
        let pending = statement
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(pending)
    }

    /// 记录嵌入图提取结果并清空对应待提取项。
    /// `stored_image_path` 为 None 表示提取失败或无数据可提取，该行不再保留嵌入图回退。
    pub fn resolve_pending_embedded_extractions(
        &mut self,
        results: &[(i64, Option<String>)],
    ) -> Result<(), DatabaseError> {
        let transaction = self.connection.transaction()?;
        {
            let mut update = transaction
                .prepare("UPDATE rows SET stored_image_path = ?2 WHERE id = ?1")?;
            let mut clear = transaction
                .prepare("DELETE FROM pending_embedded_extractions WHERE row_id = ?1")?;
            for (row_id, stored_image_path) in results {
                if let Some(stored) = stored_image_path {
                    update.execute(params![row_id, stored])?;
                }
                clear.execute([row_id])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::append_rows;
    use super::super::{NewRow, SourceType};
    use super::*;

    #[test]
    fn returns_path_and_stored_copy_for_one_row() {
        let mut database = Database::open_in_memory().unwrap();
        let row = NewRow {
            source_ordinal: 1,
            identity: r"archive:d:\pack.zip!sample.png".into(),
            image_path: Some(r"D:\Packs\pack.zip > sample.png".into()),
            stored_image_rel: Some("sample.png".into()),
            ..NewRow::default()
        };
        database
            .append_batch(SourceType::Archive, r"D:\Packs\pack.zip", &[row], |_| Ok(()))
            .unwrap();

        assert_eq!(
            database.row_image_locator(1).unwrap(),
            RowImageLocator {
                row_id: 1,
                image_path: Some(r"D:\Packs\pack.zip > sample.png".into()),
                stored_image_path: Some("files/1/sample.png".into()),
                source_type: SourceType::Archive,
            }
        );
        assert!(matches!(
            database.row_image_locator(2),
            Err(DatabaseError::RowNotFound(2))
        ));
    }

    #[test]
    fn resolves_pending_extractions_and_updates_rows() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .append_batch(
                SourceType::Xlsx,
                "legacy.xlsx",
                &super::super::test_support::test_rows(2),
                |_| Ok(()),
            )
            .unwrap();
        database
            .connection
            .execute_batch(
                "INSERT INTO pending_embedded_extractions (row_id, media_path)
                 VALUES (1, 'xl/media/image1.png'), (2, 'xl/media/image2.png');",
            )
            .unwrap();

        database
            .resolve_pending_embedded_extractions(&[
                (1, Some("files/1/embedded/row-1.png".to_owned())),
                (2, None),
            ])
            .unwrap();

        assert!(database.pending_embedded_extractions().unwrap().is_empty());
        assert_eq!(
            database.row_image_locator(1).unwrap().stored_image_path,
            Some("files/1/embedded/row-1.png".to_owned())
        );
        assert_eq!(
            database.row_image_locator(2).unwrap().stored_image_path,
            None
        );
    }
}
