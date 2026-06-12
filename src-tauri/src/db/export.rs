use std::collections::HashMap;

use rusqlite::TransactionBehavior;

use super::tags::{
    TARGET_ROWS_TABLE, TagMutationError, create_selection_rows, drop_selection_tables,
};
use super::{Database, RowSelection};

/// 导出快照中的一行：完整字段 + 二进制序排序的 Tag。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRow {
    pub id: i64,
    pub time: Option<String>,
    pub positive_prompt: Option<String>,
    pub negative_prompt: Option<String>,
    pub artists: Option<String>,
    pub image_folder: Option<String>,
    pub image_path: Option<String>,
    pub stored_image_path: Option<String>,
    pub tags: Vec<String>,
}

impl Database {
    /// 按入库顺序返回选中行的导出快照（xlsx / 智绘姬 JSON / 图片输出包共用）。
    pub fn export_rows(
        &mut self,
        selection: &RowSelection,
    ) -> Result<Vec<ExportRow>, TagMutationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        create_selection_rows(&transaction, selection)?;

        let mut rows: Vec<ExportRow> = Vec::new();
        {
            let mut statement = transaction.prepare(&format!(
                "SELECT rows.id, rows.time, rows.positive_prompt, rows.negative_prompt,
                        rows.artists, rows.image_folder, rows.image_path, rows.stored_image_path
                 FROM {TARGET_ROWS_TABLE} AS target
                 JOIN rows ON rows.id = target.id
                 ORDER BY rows.id"
            ))?;
            let mut matched = statement.query([])?;
            while let Some(row) = matched.next()? {
                rows.push(ExportRow {
                    id: row.get(0)?,
                    time: row.get(1)?,
                    positive_prompt: row.get(2)?,
                    negative_prompt: row.get(3)?,
                    artists: row.get(4)?,
                    image_folder: row.get(5)?,
                    image_path: row.get(6)?,
                    stored_image_path: row.get(7)?,
                    tags: Vec::new(),
                });
            }
        }

        {
            let index_by_id: HashMap<i64, usize> = rows
                .iter()
                .enumerate()
                .map(|(index, row)| (row.id, index))
                .collect();
            let mut statement = transaction.prepare(&format!(
                "SELECT row_tags.row_id, tags.name
                 FROM {TARGET_ROWS_TABLE} AS target
                 JOIN row_tags ON row_tags.row_id = target.id
                 JOIN tags ON tags.id = row_tags.tag_id
                 ORDER BY row_tags.row_id, tags.name COLLATE BINARY"
            ))?;
            let mut tag_rows = statement.query([])?;
            while let Some(tag_row) = tag_rows.next()? {
                let row_id: i64 = tag_row.get(0)?;
                let tag: String = tag_row.get(1)?;
                if let Some(index) = index_by_id.get(&row_id) {
                    rows[*index].tags.push(tag);
                }
            }
        }

        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{append_rows, test_rows};
    use super::super::TagMatchMode;
    use super::*;

    #[test]
    fn exports_selected_rows_in_library_order_with_sorted_tags() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(4));
        database
            .add_tags_to_rows(&[1], &["blue".into(), "Blue".into()])
            .unwrap();
        database.add_tags_to_rows(&[3], &["Keep".into()]).unwrap();

        let rows = database
            .export_rows(&RowSelection::Explicit {
                row_ids: vec![3, 1],
            })
            .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, 1);
        assert_eq!(rows[0].tags, vec!["Blue", "blue"]);
        assert_eq!(rows[0].positive_prompt.as_deref(), Some("prompt 1"));
        assert_eq!(rows[1].id, 3);
        assert_eq!(rows[1].tags, vec!["Keep"]);
    }

    #[test]
    fn exports_filtered_selection_with_empty_filter_as_whole_library() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(3));

        let rows = database
            .export_rows(&RowSelection::Filtered {
                tags: Vec::new(),
                tag_mode: TagMatchMode::And,
                excluded_row_ids: Vec::new(),
            })
            .unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }
}
