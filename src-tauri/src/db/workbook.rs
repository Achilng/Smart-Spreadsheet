use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};

use super::{Database, DatabaseError};
use crate::excel::{EmbeddedImageRef, ParsedWorkbook};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbookSummary {
    pub imported_name: String,
    pub imported_at: String,
    pub sheet_name: String,
    pub row_count: u64,
}

impl Database {
    pub fn workbook_summary(&self) -> Result<Option<WorkbookSummary>, DatabaseError> {
        let stored = self
            .connection
            .query_row(
                "SELECT imported_name, imported_at, sheet_name, row_count
                 FROM workbook WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;
        stored
            .map(|(imported_name, imported_at, sheet_name, row_count)| {
                Ok(WorkbookSummary {
                    imported_name,
                    imported_at,
                    sheet_name,
                    row_count: u64::try_from(row_count)
                        .map_err(|_| DatabaseError::CountOverflow)?,
                })
            })
            .transpose()
    }

    pub fn replace_workbook(
        &mut self,
        imported_name: &str,
        workbook: &ParsedWorkbook,
        images: &[EmbeddedImageRef],
    ) -> Result<(), DatabaseError> {
        let image_by_row: HashMap<u32, &str> = images
            .iter()
            .filter(|image| image.source_column == 1)
            .map(|image| (image.source_row, image.media_path.as_str()))
            .collect();
        let row_count =
            i64::try_from(workbook.rows.len()).map_err(|_| DatabaseError::RowCountOverflow)?;
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM workbook", [])?;
        transaction.execute("DELETE FROM tags", [])?;
        transaction.execute(
            "INSERT INTO workbook(id, imported_name, imported_at, sheet_name, row_count)
             VALUES (1, ?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?2, ?3)",
            params![imported_name, workbook.sheet_name, row_count],
        )?;

        {
            let mut insert = transaction.prepare(
                "INSERT INTO rows(
                    workbook_id, source_row, time, positive_prompt, negative_prompt,
                    artists, image_folder, image_path, embedded_image_ref
                 ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for row in &workbook.rows {
                insert.execute(params![
                    row.source_row,
                    row.time,
                    row.positive_prompt,
                    row.negative_prompt,
                    row.artists,
                    row.image_folder,
                    row.image_path,
                    image_by_row.get(&row.source_row).copied(),
                ])?;
            }
        }

        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::excel::ImportedRow;

    use super::*;

    fn row(source_row: u32, prompt: &str) -> ImportedRow {
        ImportedRow {
            source_row,
            time: Some("2026-06-10".into()),
            positive_prompt: Some(prompt.into()),
            negative_prompt: None,
            artists: None,
            image_folder: None,
            image_path: None,
        }
    }

    #[test]
    fn stores_rows_and_embedded_image_references() {
        let mut database = Database::open_in_memory().unwrap();
        let workbook = ParsedWorkbook {
            sheet_name: "NovelAI Metadata".into(),
            rows: vec![row(2, "first"), row(3, "second")],
        };
        let images = vec![EmbeddedImageRef {
            source_row: 2,
            source_column: 1,
            media_path: "xl/media/image1.png".into(),
        }];

        database
            .replace_workbook("sample.xlsx", &workbook, &images)
            .unwrap();

        let stored: (String, u32, String) = database
            .connection
            .query_row(
                "SELECT workbook.imported_name, workbook.row_count, rows.embedded_image_ref
                 FROM workbook JOIN rows ON rows.workbook_id = workbook.id
                 WHERE rows.source_row = 2",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            stored,
            ("sample.xlsx".into(), 2, "xl/media/image1.png".into())
        );
    }

    #[test]
    fn failed_replacement_rolls_back_previous_workbook_and_tags() {
        let mut database = Database::open_in_memory().unwrap();
        let previous = ParsedWorkbook {
            sheet_name: "NovelAI Metadata".into(),
            rows: vec![row(2, "previous")],
        };
        database
            .replace_workbook("previous.xlsx", &previous, &[])
            .unwrap();
        database
            .connection
            .execute("INSERT INTO tags(name) VALUES ('existing')", [])
            .unwrap();

        let invalid = ParsedWorkbook {
            sheet_name: "NovelAI Metadata".into(),
            rows: vec![row(2, "new"), row(2, "duplicate")],
        };
        assert!(
            database
                .replace_workbook("invalid.xlsx", &invalid, &[])
                .is_err()
        );

        let stored: (String, String, u32) = database
            .connection
            .query_row(
                "SELECT workbook.imported_name, rows.positive_prompt,
                    (SELECT COUNT(*) FROM tags)
                 FROM workbook JOIN rows ON rows.workbook_id = workbook.id",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(stored, ("previous.xlsx".into(), "previous".into(), 1));
    }
}
