use rusqlite::TransactionBehavior;

use super::tags::{
    TARGET_ROWS_TABLE, TagMutationError, create_selection_rows, drop_selection_tables,
};
use super::{Database, DatabaseError, RowSelection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteOutcome {
    pub deleted_rows: u64,
    /// 被删除的行 ID（升序），供存储层清理对应缩略图缓存。
    pub deleted_row_ids: Vec<i64>,
    /// 被删除行引用的受管副本相对路径，供存储层删除文件。
    pub stored_image_paths: Vec<String>,
    /// 文件夹/单 PNG 来源行引用的原始图片路径，供存储层按需移入回收站。
    pub original_image_paths: Vec<String>,
    /// 压缩包来源行没有独立原文件；请求删除原图时用于结果汇总。
    pub archive_rows: usize,
}

impl Database {
    /// 删除选中的行。Tag 关联随行级联删除，Tag 定义保留（与零关联 Tag 持久保留的规则一致）。
    pub fn delete_rows(
        &mut self,
        selection: &RowSelection,
    ) -> Result<DeleteOutcome, TagMutationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        create_selection_rows(&transaction, selection)?;

        let mut deleted_row_ids = Vec::new();
        let mut stored_image_paths = Vec::new();
        let mut original_image_paths = Vec::new();
        let mut archive_rows = 0;
        {
            let mut statement = transaction.prepare(&format!(
                "SELECT rows.id, rows.stored_image_path, batches.source_type, rows.image_path
                 FROM {TARGET_ROWS_TABLE} AS target
                 JOIN rows ON rows.id = target.id
                 JOIN import_batches AS batches ON batches.id = rows.batch_id
                 ORDER BY rows.id"
            ))?;
            let mut matched = statement.query([])?;
            while let Some(row) = matched.next()? {
                deleted_row_ids.push(row.get::<_, i64>(0)?);
                if let Some(stored) = row.get::<_, Option<String>>(1)? {
                    stored_image_paths.push(stored);
                }
                match row.get::<_, String>(2)?.as_str() {
                    "folder" => {
                        if let Some(path) = row
                            .get::<_, Option<String>>(3)?
                            .filter(|path| !path.trim().is_empty())
                        {
                            original_image_paths.push(path);
                        }
                    }
                    "archive" => archive_rows += 1,
                    _ => {}
                }
            }
        }

        transaction.execute(
            &format!(
                "DELETE FROM pending_embedded_extractions
                 WHERE row_id IN (SELECT id FROM {TARGET_ROWS_TABLE})"
            ),
            [],
        )?;
        let deleted = transaction.execute(
            &format!("DELETE FROM rows WHERE id IN (SELECT id FROM {TARGET_ROWS_TABLE})"),
            [],
        )?;
        drop_selection_tables(&transaction)?;
        transaction.commit()?;

        Ok(DeleteOutcome {
            deleted_rows: u64::try_from(deleted)
                .map_err(|_| TagMutationError::Database(DatabaseError::CountOverflow))?,
            deleted_row_ids,
            stored_image_paths,
            original_image_paths,
            archive_rows,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{append_rows, database_with_rows, test_rows};
    use super::super::{NewRow, SourceType, TagMatchMode};
    use super::*;

    #[test]
    fn deletes_explicit_rows_and_keeps_tag_definitions() {
        let mut database = database_with_rows(4);
        database
            .add_tags_to_rows(&[1, 2], &["Keep".into()])
            .unwrap();

        let outcome = database
            .delete_rows(&RowSelection::Explicit { row_ids: vec![1, 3] })
            .unwrap();

        assert_eq!(outcome.deleted_rows, 2);
        assert_eq!(outcome.deleted_row_ids, vec![1, 3]);
        assert_eq!(database.library_summary().unwrap().row_count, 2);
        // Tag 定义保留，关联只剩第 2 行。
        let tags = database.list_tags().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "Keep");
        assert_eq!(tags[0].row_count, 1);
    }

    #[test]
    fn unknown_explicit_row_rolls_back_entire_deletion() {
        let mut database = database_with_rows(2);

        let error = database
            .delete_rows(&RowSelection::Explicit {
                row_ids: vec![1, 999],
            })
            .unwrap_err();

        assert!(matches!(error, TagMutationError::UnknownRows(rows) if rows == vec![999]));
        assert_eq!(database.library_summary().unwrap().row_count, 2);
    }

    #[test]
    fn deletes_filtered_selection_with_exclusions() {
        let mut database = database_with_rows(5);
        database
            .add_tags_to_rows(&[1, 2, 3], &["Red".into()])
            .unwrap();

        let outcome = database
            .delete_rows(&RowSelection::Filtered {
                tags: vec!["Red".into()],
                tag_mode: TagMatchMode::And,
                dedupe: crate::db::DedupeMode::None,
                single_artist_only: false,
                excluded_row_ids: vec![2],
            })
            .unwrap();

        assert_eq!(outcome.deleted_rows, 2);
        assert_eq!(outcome.deleted_row_ids, vec![1, 3]);
        assert_eq!(database.library_summary().unwrap().row_count, 3);
    }

    #[test]
    fn returns_stored_image_paths_for_cleanup() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(1));
        let with_copy = NewRow {
            source_ordinal: 1,
            identity: r"archive:d:\pack.zip!a.png".into(),
            stored_image_rel: Some("a.png".into()),
            ..NewRow::default()
        };
        let appended = database
            .append_batch(SourceType::Archive, r"D:\pack.zip", &[with_copy], |_| Ok(()))
            .unwrap();

        let outcome = database
            .delete_rows(&RowSelection::Explicit { row_ids: vec![2] })
            .unwrap();

        assert_eq!(
            outcome.stored_image_paths,
            vec![format!("files/{}/a.png", appended.batch_id)]
        );
    }

    #[test]
    fn returns_folder_original_paths_and_archive_row_count() {
        let mut database = Database::open_in_memory().unwrap();
        let folder_row = NewRow {
            source_ordinal: 1,
            identity: r"file:d:\photos\source.png".into(),
            image_path: Some(r"D:\photos\source.png".into()),
            ..NewRow::default()
        };
        database
            .append_batch(SourceType::Folder, r"D:\photos", &[folder_row], |_| Ok(()))
            .unwrap();
        let archive_row = NewRow {
            source_ordinal: 1,
            identity: r"archive:d:\pack.zip!nested.png".into(),
            image_path: Some(r"D:\Agent\Agent_temp\extracted\nested.png".into()),
            stored_image_rel: Some("nested.png".into()),
            ..NewRow::default()
        };
        database
            .append_batch(SourceType::Archive, r"D:\pack.zip", &[archive_row], |_| Ok(()))
            .unwrap();

        let outcome = database
            .delete_rows(&RowSelection::Explicit { row_ids: vec![1, 2] })
            .unwrap();

        assert_eq!(outcome.original_image_paths, vec![r"D:\photos\source.png"]);
        assert_eq!(outcome.archive_rows, 1);
    }

    #[test]
    fn deleted_identity_can_be_imported_again() {
        let mut database = database_with_rows(2);
        database
            .delete_rows(&RowSelection::Explicit { row_ids: vec![1] })
            .unwrap();

        let outcome = database
            .append_batch(
                SourceType::Folder,
                r"D:\test",
                &test_rows(2),
                |_| Ok(()),
            )
            .unwrap();

        assert_eq!(outcome.added, 1);
        assert_eq!(outcome.skipped_existing, 1);
        assert_eq!(database.library_summary().unwrap().row_count, 2);
    }
}
