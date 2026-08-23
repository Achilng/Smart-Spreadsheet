use rusqlite::OptionalExtension;

use super::{Database, DatabaseError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowImageLocator {
    pub row_id: i64,
    pub image_path: Option<String>,
    /// 受管副本相对数据目录根的路径。
    pub stored_image_path: Option<String>,
    /// 该受管副本是否为完整原件；历史缩略图副本为 false。
    pub stored_image_is_original: bool,
}

impl Database {
    pub fn row_image_locator(&self, row_id: i64) -> Result<RowImageLocator, DatabaseError> {
        self.connection
            .query_row(
                "SELECT id, image_path, stored_image_path, stored_image_is_original
                 FROM rows WHERE id = ?1",
                [row_id],
                |row| {
                    Ok(RowImageLocator {
                        row_id: row.get(0)?,
                        image_path: row.get(1)?,
                        stored_image_path: row.get(2)?,
                        stored_image_is_original: row.get(3)?,
                    })
                },
            )
            .optional()?
            .ok_or(DatabaseError::RowNotFound(row_id))
    }

    pub fn missing_vibe_statuses(&self) -> Result<Vec<RowImageLocator>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT id, image_path, stored_image_path, stored_image_is_original
             FROM rows
             WHERE vibe_reference_count IS NULL
                OR (vibe_reference_count > 0 AND vibe_signature IS NULL)
                OR generation_model IS NULL
             ORDER BY id",
        )?;
        Ok(statement
            .query_map([], |row| {
                Ok(RowImageLocator {
                    row_id: row.get(0)?,
                    image_path: row.get(1)?,
                    stored_image_path: row.get(2)?,
                    stored_image_is_original: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    /// 批量写入 VIBE 状态与作画模型。`count` 为 None 时保留既有数量（文件已
    /// 不可读的签名回填场景），行内无既有数量则记 0；签名为空字符串表示
    /// “已扫描但无法取得”，避免每次启动重复扫描，且不参与聚合。`model`
    /// 仅在行内尚无模型名（NULL）时写入，空字符串同样是“已扫描”标记，
    /// 既有真实模型名不会被覆盖。
    pub fn update_vibe_statuses(
        &mut self,
        statuses: &[(i64, Option<u32>, Option<String>, String)],
    ) -> Result<(), DatabaseError> {
        let transaction = self.connection.transaction()?;
        {
            let mut update = transaction.prepare(
                "UPDATE rows SET
                    vibe_reference_count = COALESCE(?2, vibe_reference_count, 0),
                    vibe_signature = ?3,
                    generation_model = COALESCE(generation_model, ?4)
                 WHERE id = ?1",
            )?;
            for (row_id, count, signature, model) in statuses {
                update.execute(rusqlite::params![row_id, count, signature, model])?;
            }
        }
        transaction.commit()?;
        self.bump_data_version();
        Ok(())
    }

    pub fn row_vibe_reference_count(
        &self,
        row_id: i64,
    ) -> Result<Option<u32>, DatabaseError> {
        self.connection
            .query_row(
                "SELECT vibe_reference_count FROM rows WHERE id = ?1",
                [row_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(DatabaseError::RowNotFound(row_id))
    }

    /// 画风签名回填候选：全库行的 id 与正向提示词（无提示词行为 None）。
    pub fn all_row_positive_prompts(&self) -> Result<Vec<(i64, Option<String>)>, DatabaseError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, positive_prompt FROM rows ORDER BY id")?;
        Ok(statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn row_style_signature(&self, row_id: i64) -> Result<Option<String>, DatabaseError> {
        self.connection
            .query_row(
                "SELECT style_signature FROM rows WHERE id = ?1",
                [row_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(DatabaseError::RowNotFound(row_id))
    }

    /// 批量写入画风签名（回填/全量重算使用）。
    pub fn update_style_signatures(
        &mut self,
        signatures: &[(i64, Option<String>)],
    ) -> Result<(), DatabaseError> {
        let transaction = self.connection.transaction()?;
        {
            let mut update = transaction
                .prepare("UPDATE rows SET style_signature = ?2 WHERE id = ?1")?;
            for (row_id, signature) in signatures {
                update.execute(rusqlite::params![row_id, signature])?;
            }
        }
        transaction.commit()?;
        self.bump_data_version();
        Ok(())
    }

    /// 库内记录的画风签名算法版本；从未回填时为 None。
    pub fn style_signature_version(&self) -> Result<Option<u32>, DatabaseError> {
        Ok(self
            .setting(STYLE_SIGNATURE_VERSION_SETTING)?
            .and_then(|value| value.parse().ok()))
    }

    pub fn set_style_signature_version(&mut self, version: u32) -> Result<(), DatabaseError> {
        self.set_setting(STYLE_SIGNATURE_VERSION_SETTING, &version.to_string())
    }
}

const STYLE_SIGNATURE_VERSION_SETTING: &str = "style_signature_version";

#[cfg(test)]
mod tests {
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
                stored_image_is_original: true,
            }
        );
        assert!(matches!(
            database.row_image_locator(2),
            Err(DatabaseError::RowNotFound(2))
        ));
    }
}
