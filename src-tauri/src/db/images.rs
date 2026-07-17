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
}

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
