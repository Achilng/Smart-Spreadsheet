use super::{Database, DatabaseError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowTagSnapshot {
    pub source_row: u32,
    pub tags: Vec<String>,
}

impl Database {
    pub fn export_row_tags(&self) -> Result<Vec<RowTagSnapshot>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT rows.source_row, tags.name
             FROM rows
             LEFT JOIN row_tags ON row_tags.row_id = rows.id
             LEFT JOIN tags ON tags.id = row_tags.tag_id
             ORDER BY rows.source_row, rows.id, tags.name COLLATE BINARY",
        )?;
        let mut query = statement.query([])?;
        let mut snapshots: Vec<RowTagSnapshot> = Vec::new();
        while let Some(row) = query.next()? {
            let source_row: u32 = row.get(0)?;
            let tag: Option<String> = row.get(1)?;
            if snapshots
                .last()
                .is_none_or(|last| last.source_row != source_row)
            {
                snapshots.push(RowTagSnapshot {
                    source_row,
                    tags: Vec::new(),
                });
            }
            if let Some(tag) = tag {
                snapshots
                    .last_mut()
                    .expect("snapshot was inserted for current row")
                    .tags
                    .push(tag);
            }
        }
        Ok(snapshots)
    }
}

#[cfg(test)]
mod tests {
    use crate::excel::{ImportedRow, ParsedWorkbook};

    use super::*;

    #[test]
    fn snapshots_every_row_with_binary_sorted_tags() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .replace_workbook(
                "export.xlsx",
                &ParsedWorkbook {
                    sheet_name: "Sheet1".into(),
                    rows: (2..=4)
                        .map(|source_row| ImportedRow {
                            source_row,
                            time: None,
                            positive_prompt: None,
                            negative_prompt: None,
                            artists: None,
                            image_folder: None,
                            image_path: None,
                        })
                        .collect(),
                },
                &[],
            )
            .unwrap();
        database
            .add_tags_to_rows(&[1], &["blue".into(), "Blue".into()])
            .unwrap();
        database.add_tags_to_rows(&[3], &["Keep".into()]).unwrap();

        assert_eq!(
            database.export_row_tags().unwrap(),
            vec![
                RowTagSnapshot {
                    source_row: 2,
                    tags: vec!["Blue".into(), "blue".into()],
                },
                RowTagSnapshot {
                    source_row: 3,
                    tags: Vec::new(),
                },
                RowTagSnapshot {
                    source_row: 4,
                    tags: vec!["Keep".into()],
                },
            ]
        );
    }
}
