use rusqlite::params;

use super::{Database, DatabaseError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowTagSnapshot {
    /// xlsx 批次行对应的源 Excel 行号（即 source_ordinal）。
    pub source_row: u32,
    pub tags: Vec<String>,
}

impl Database {
    /// 按源行号导出指定批次每一行的 Tag 快照（旧版 OOXML 导出使用，仅适用于 xlsx 批次）。
    pub fn export_row_tags_for_batch(
        &self,
        batch_id: i64,
    ) -> Result<Vec<RowTagSnapshot>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT rows.source_ordinal, tags.name
             FROM rows
             LEFT JOIN row_tags ON row_tags.row_id = rows.id
             LEFT JOIN tags ON tags.id = row_tags.tag_id
             WHERE rows.batch_id = ?1
             ORDER BY rows.source_ordinal, rows.id, tags.name COLLATE BINARY",
        )?;
        let mut query = statement.query(params![batch_id])?;
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
    use super::super::test_support::{append_rows, test_rows};
    use super::super::{NewRow, SourceType};
    use super::*;

    #[test]
    fn snapshots_batch_rows_with_binary_sorted_tags() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(3));
        database
            .add_tags_to_rows(&[1], &["blue".into(), "Blue".into()])
            .unwrap();
        database.add_tags_to_rows(&[3], &["Keep".into()]).unwrap();

        assert_eq!(
            database.export_row_tags_for_batch(1).unwrap(),
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

    #[test]
    fn excludes_rows_from_other_batches() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(2));
        database
            .append_batch(
                SourceType::Folder,
                r"D:\other",
                &[NewRow {
                    source_ordinal: 9,
                    identity: "file:other".into(),
                    ..NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();

        let snapshots = database.export_row_tags_for_batch(1).unwrap();

        assert_eq!(snapshots.len(), 2);
        assert!(snapshots.iter().all(|snapshot| snapshot.source_row != 9));
    }
}
