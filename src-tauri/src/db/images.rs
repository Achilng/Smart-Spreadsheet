use rusqlite::OptionalExtension;

use super::{Database, DatabaseError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowImageLocator {
    pub row_id: i64,
    pub image_path: Option<String>,
    pub embedded_image_ref: Option<String>,
}

impl Database {
    pub fn row_image_locator(&self, row_id: i64) -> Result<RowImageLocator, DatabaseError> {
        self.connection
            .query_row(
                "SELECT id, image_path, embedded_image_ref FROM rows WHERE id = ?1",
                [row_id],
                |row| {
                    Ok(RowImageLocator {
                        row_id: row.get(0)?,
                        image_path: row.get(1)?,
                        embedded_image_ref: row.get(2)?,
                    })
                },
            )
            .optional()?
            .ok_or(DatabaseError::RowNotFound(row_id))
    }
}

#[cfg(test)]
mod tests {
    use crate::excel::{EmbeddedImageRef, ImportedRow, ParsedWorkbook};

    use super::*;

    #[test]
    fn returns_path_and_embedded_reference_for_one_row() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .replace_workbook(
                "images.xlsx",
                &ParsedWorkbook {
                    sheet_name: "Sheet1".into(),
                    rows: vec![ImportedRow {
                        source_row: 2,
                        time: None,
                        positive_prompt: None,
                        negative_prompt: None,
                        artists: None,
                        image_folder: None,
                        image_path: Some(r"D:\images\sample.png".into()),
                    }],
                },
                &[EmbeddedImageRef {
                    source_row: 2,
                    source_column: 1,
                    media_path: "xl/media/image1.png".into(),
                }],
            )
            .unwrap();

        assert_eq!(
            database.row_image_locator(1).unwrap(),
            RowImageLocator {
                row_id: 1,
                image_path: Some(r"D:\images\sample.png".into()),
                embedded_image_ref: Some("xl/media/image1.png".into()),
            }
        );
        assert!(matches!(
            database.row_image_locator(2),
            Err(DatabaseError::RowNotFound(2))
        ));
    }
}
