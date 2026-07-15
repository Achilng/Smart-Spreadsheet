use std::collections::HashSet;

use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::{Database, DatabaseError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    Xlsx,
    Folder,
    Archive,
}

impl SourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
            Self::Folder => "folder",
            Self::Archive => "archive",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self, DatabaseError> {
        match value {
            "xlsx" => Ok(Self::Xlsx),
            "folder" => Ok(Self::Folder),
            "archive" => Ok(Self::Archive),
            other => Err(DatabaseError::IntegrityCheckFailed(format!(
                "未知的批次来源类型: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchSummary {
    pub id: i64,
    pub source_type: SourceType,
    pub source_path: String,
    pub imported_at: String,
    pub added_count: u64,
    pub skipped_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySummary {
    pub row_count: u64,
    pub batch_count: u64,
    pub last_batch: Option<BatchSummary>,
}

/// 追加导入的一行候选数据。`identity` 为增量跳过的身份键（见 `identity` 模块）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewRow {
    pub source_ordinal: u32,
    pub identity: String,
    pub source_size: Option<i64>,
    pub source_mtime: Option<i64>,
    pub content_hash: Option<String>,
    pub perceptual_hash: Option<String>,
    pub time: Option<String>,
    pub positive_prompt: Option<String>,
    pub character_prompt: Option<String>,
    pub negative_prompt: Option<String>,
    pub note: Option<String>,
    pub artists: Option<String>,
    pub image_folder: Option<String>,
    pub image_path: Option<String>,
    /// 受管副本相对批次目录的路径；入库时组装为 `files/<批次ID>/<此路径>`。
    pub stored_image_rel: Option<String>,
    pub metadata_failed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendOutcome {
    pub batch_id: i64,
    pub added: u64,
    pub skipped_existing: u64,
    pub skipped_content: u64,
    /// 身份键已存在但文件大小/修改时间与库中记录不一致的数量（仅供提示，不改动已入库行）。
    pub changed_existing: u64,
}

impl Database {
    /// 追加一个导入批次：先按身份键跳过，再按内容哈希跳过，其余按给定顺序插入。
    ///
    /// `finalize` 在批次行写入之后、事务提交之前调用（参数为批次 ID），
    /// 供调用方完成依赖批次 ID 的文件落位（如重命名暂存目录）；
    /// 返回错误时整个批次回滚。
    pub fn append_batch(
        &mut self,
        source_type: SourceType,
        source_path: &str,
        rows: &[NewRow],
        finalize: impl FnOnce(i64) -> Result<(), String>,
    ) -> Result<AppendOutcome, DatabaseError> {
        let mut seen = HashSet::with_capacity(rows.len());
        for row in rows {
            if row.identity.trim().is_empty() {
                return Err(DatabaseError::EmptyIdentity);
            }
            if !seen.insert(row.identity.as_str()) {
                return Err(DatabaseError::DuplicateIdentityInBatch(
                    row.identity.clone(),
                ));
            }
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO import_batches (source_type, source_path, imported_at, added_count, skipped_count)
             VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 0, 0)",
            params![source_type.as_str(), source_path],
        )?;
        let batch_id = transaction.last_insert_rowid();

        let mut added: u64 = 0;
        let mut skipped_existing: u64 = 0;
        let mut skipped_content: u64 = 0;
        let mut changed_existing: u64 = 0;
        {
            let mut seen_content = transaction
                .prepare("SELECT content_hash FROM rows WHERE content_hash IS NOT NULL")?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<HashSet<_>, _>>()?;
            let mut find_existing = transaction.prepare(
                "SELECT source_size, source_mtime FROM rows WHERE identity = ?1",
            )?;
            let mut insert = transaction.prepare(
                "INSERT INTO rows (
                    batch_id, source_ordinal, identity, source_size, source_mtime,
                    time, positive_prompt, character_prompt, negative_prompt, note, artists,
                    image_folder, image_path, stored_image_path, metadata_failed,
                    content_hash, perceptual_hash
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            )?;
            for row in rows {
                let existing = find_existing
                    .query_row([&row.identity], |stored| {
                        Ok((
                            stored.get::<_, Option<i64>>(0)?,
                            stored.get::<_, Option<i64>>(1)?,
                        ))
                    })
                    .optional()?;
                if let Some((stored_size, stored_mtime)) = existing {
                    skipped_existing += 1;
                    let candidate_known =
                        row.source_size.is_some() || row.source_mtime.is_some();
                    if candidate_known
                        && (stored_size != row.source_size || stored_mtime != row.source_mtime)
                    {
                        changed_existing += 1;
                    }
                    continue;
                }
                if row
                    .content_hash
                    .as_ref()
                    .is_some_and(|hash| !seen_content.insert(hash.clone()))
                {
                    skipped_content += 1;
                    continue;
                }

                let stored_image_path = row
                    .stored_image_rel
                    .as_deref()
                    .map(|relative| format!("files/{batch_id}/{relative}"));
                insert.execute(params![
                    batch_id,
                    row.source_ordinal,
                    row.identity,
                    row.source_size,
                    row.source_mtime,
                    row.time,
                    row.positive_prompt,
                    row.character_prompt,
                    row.negative_prompt,
                    row.note,
                    row.artists,
                    row.image_folder,
                    row.image_path,
                    stored_image_path,
                    row.metadata_failed,
                    row.content_hash,
                    row.perceptual_hash,
                ])?;
                added += 1;
            }
        }

        let added_count =
            i64::try_from(added).map_err(|_| DatabaseError::RowCountOverflow)?;
        let skipped_count = i64::try_from(skipped_existing + skipped_content)
            .map_err(|_| DatabaseError::RowCountOverflow)?;
        transaction.execute(
            "UPDATE import_batches SET added_count = ?1, skipped_count = ?2 WHERE id = ?3",
            params![added_count, skipped_count, batch_id],
        )?;

        finalize(batch_id).map_err(DatabaseError::BatchFinalizeFailed)?;
        transaction.commit()?;

        Ok(AppendOutcome {
            batch_id,
            added,
            skipped_existing,
            skipped_content,
            changed_existing,
        })
    }

    /// 返回候选身份键中已经存在于库中的子集（供导入前决定哪些行需要落位文件）。
    pub fn existing_identities(
        &mut self,
        candidates: &[String],
    ) -> Result<HashSet<String>, DatabaseError> {
        const CANDIDATES_TABLE: &str = "temp.identity_candidates";
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(&format!(
            "DROP TABLE IF EXISTS {CANDIDATES_TABLE};
             CREATE TEMP TABLE {CANDIDATES_TABLE} (
                 identity TEXT PRIMARY KEY
             ) STRICT, WITHOUT ROWID;"
        ))?;
        {
            let mut insert = transaction.prepare(&format!(
                "INSERT OR IGNORE INTO {CANDIDATES_TABLE}(identity) VALUES (?1)"
            ))?;
            for candidate in candidates {
                insert.execute([candidate])?;
            }
        }
        let existing = {
            let mut statement = transaction.prepare(&format!(
                "SELECT rows.identity
                 FROM rows
                 JOIN {CANDIDATES_TABLE} AS candidates
                   ON candidates.identity = rows.identity"
            ))?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<HashSet<_>, _>>()?
        };
        transaction.execute_batch(&format!("DROP TABLE {CANDIDATES_TABLE};"))?;
        transaction.commit()?;
        Ok(existing)
    }

    pub fn list_batches(&self) -> Result<Vec<BatchSummary>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT id, source_type, source_path, imported_at, added_count, skipped_count
             FROM import_batches
             ORDER BY id DESC",
        )?;
        let stored = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        stored.into_iter().map(batch_summary_from_stored).collect()
    }

    pub fn row_ids_for_batch(&self, batch_id: i64) -> Result<Vec<i64>, DatabaseError> {
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM import_batches WHERE id = ?1)",
            [batch_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(DatabaseError::BatchNotFound(batch_id));
        }
        let mut statement = self
            .connection
            .prepare("SELECT id FROM rows WHERE batch_id = ?1 ORDER BY id")?;
        Ok(statement
            .query_map([batch_id], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete_batch_if_empty(&mut self, batch_id: i64) -> Result<bool, DatabaseError> {
        let deleted = self.connection.execute(
            "DELETE FROM import_batches
             WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM rows WHERE batch_id = ?1)",
            [batch_id],
        )?;
        Ok(deleted > 0)
    }

    pub fn library_summary(&self) -> Result<LibrarySummary, DatabaseError> {
        let row_count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM rows", [], |row| row.get(0))?;
        let batch_count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM import_batches",
            [],
            |row| row.get(0),
        )?;
        let last_batch = self
            .connection
            .query_row(
                "SELECT id, source_type, source_path, imported_at, added_count, skipped_count
                 FROM import_batches
                 ORDER BY id DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()?
            .map(batch_summary_from_stored)
            .transpose()?;
        Ok(LibrarySummary {
            row_count: u64::try_from(row_count).map_err(|_| DatabaseError::CountOverflow)?,
            batch_count: u64::try_from(batch_count)
                .map_err(|_| DatabaseError::CountOverflow)?,
            last_batch,
        })
    }
}

fn batch_summary_from_stored(
    stored: (i64, String, String, String, i64, i64),
) -> Result<BatchSummary, DatabaseError> {
    let (id, source_type, source_path, imported_at, added_count, skipped_count) = stored;
    Ok(BatchSummary {
        id,
        source_type: SourceType::from_str(&source_type)?,
        source_path,
        imported_at,
        added_count: u64::try_from(added_count).map_err(|_| DatabaseError::CountOverflow)?,
        skipped_count: u64::try_from(skipped_count)
            .map_err(|_| DatabaseError::CountOverflow)?,
    })
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{append_rows, test_rows};
    use super::*;

    fn row(identity: &str, ordinal: u32) -> NewRow {
        NewRow {
            source_ordinal: ordinal,
            identity: identity.to_owned(),
            ..NewRow::default()
        }
    }

    fn hashed_row(identity: &str, ordinal: u32, content_hash: &str) -> NewRow {
        NewRow {
            content_hash: Some(content_hash.to_owned()),
            ..row(identity, ordinal)
        }
    }

    #[test]
    fn appends_rows_and_reports_library_summary() {
        let mut database = Database::open_in_memory().unwrap();

        let outcome = database
            .append_batch(
                SourceType::Folder,
                r"D:\library",
                &test_rows(3),
                |_| Ok(()),
            )
            .unwrap();

        assert_eq!(outcome.added, 3);
        assert_eq!(outcome.skipped_existing, 0);
        let summary = database.library_summary().unwrap();
        assert_eq!(summary.row_count, 3);
        assert_eq!(summary.batch_count, 1);
        let last = summary.last_batch.unwrap();
        assert_eq!(last.source_type, SourceType::Folder);
        assert_eq!(last.source_path, r"D:\library");
        assert_eq!(last.added_count, 3);
    }

    #[test]
    fn skips_existing_identities_and_appends_only_new_rows() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(10));

        // 模拟“10 行库再导入 5 张图的压缩包，其中 3 张已存在”：
        let candidates = vec![
            row(r"file:d:\test\1.png", 1),
            row(r"file:d:\test\2.png", 2),
            row(r"file:d:\test\3.png", 3),
            row(r"archive:d:\new.zip!a.png", 4),
            row(r"archive:d:\new.zip!b.png", 5),
        ];
        let outcome = database
            .append_batch(SourceType::Archive, r"D:\new.zip", &candidates, |_| Ok(()))
            .unwrap();

        assert_eq!(outcome.added, 2);
        assert_eq!(outcome.skipped_existing, 3);
        assert_eq!(outcome.changed_existing, 0);
        assert_eq!(database.library_summary().unwrap().row_count, 12);

        // 同一来源再导一次：全部跳过，行数不变。
        let repeat = database
            .append_batch(SourceType::Archive, r"D:\new.zip", &candidates, |_| Ok(()))
            .unwrap();
        assert_eq!(repeat.added, 0);
        assert_eq!(repeat.skipped_existing, 5);
        assert_eq!(database.library_summary().unwrap().row_count, 12);
    }

    #[test]
    fn skips_content_duplicates_after_identity_check() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .append_batch(
                SourceType::Folder,
                r"D:\first",
                &[hashed_row("file:first", 1, "same")],
                |_| Ok(()),
            )
            .unwrap();

        let candidates = vec![
            // 身份键优先：即使哈希也是重复，仍计入 skipped_existing。
            hashed_row("file:first", 1, "same"),
            // 与库内内容重复。
            hashed_row("file:second", 2, "same"),
            // 本批次首个新内容入库，后一个同哈希候选跳过。
            hashed_row("file:third", 3, "new"),
            hashed_row("file:fourth", 4, "new"),
            // 无哈希候选不参与内容去重。
            row("file:unknown-a", 5),
            row("file:unknown-b", 6),
        ];
        let outcome = database
            .append_batch(SourceType::Folder, r"D:\second", &candidates, |_| Ok(()))
            .unwrap();

        assert_eq!(outcome.added, 3);
        assert_eq!(outcome.skipped_existing, 1);
        assert_eq!(outcome.skipped_content, 2);
        assert_eq!(database.library_summary().unwrap().row_count, 4);
        assert_eq!(
            database.library_summary().unwrap().last_batch.unwrap().skipped_count,
            3
        );
    }

    #[test]
    fn reports_changed_existing_rows_without_modifying_them() {
        let mut database = Database::open_in_memory().unwrap();
        let original = NewRow {
            source_ordinal: 1,
            identity: r"file:d:\test\a.png".into(),
            source_size: Some(100),
            source_mtime: Some(1_000),
            positive_prompt: Some("original".into()),
            ..NewRow::default()
        };
        database
            .append_batch(SourceType::Folder, r"D:\test", &[original], |_| Ok(()))
            .unwrap();

        let changed = NewRow {
            source_ordinal: 1,
            identity: r"file:d:\test\a.png".into(),
            source_size: Some(250),
            source_mtime: Some(2_000),
            positive_prompt: Some("changed".into()),
            ..NewRow::default()
        };
        let outcome = database
            .append_batch(SourceType::Folder, r"D:\test", &[changed], |_| Ok(()))
            .unwrap();

        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.skipped_existing, 1);
        assert_eq!(outcome.changed_existing, 1);
        let stored: String = database
            .connection
            .query_row("SELECT positive_prompt FROM rows", [], |row| row.get(0))
            .unwrap();
        assert_eq!(stored, "original");
    }

    #[test]
    fn composes_stored_image_path_with_allocated_batch_id() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(1));

        let with_copy = NewRow {
            source_ordinal: 1,
            identity: r"archive:d:\pack.zip!a.png".into(),
            stored_image_rel: Some("a.png".into()),
            ..NewRow::default()
        };
        let outcome = database
            .append_batch(SourceType::Archive, r"D:\pack.zip", &[with_copy], |_| Ok(()))
            .unwrap();

        let stored: String = database
            .connection
            .query_row(
                "SELECT stored_image_path FROM rows WHERE batch_id = ?1",
                [outcome.batch_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, format!("files/{}/a.png", outcome.batch_id));
    }

    #[test]
    fn rejects_duplicate_and_empty_identities_within_a_batch() {
        let mut database = Database::open_in_memory().unwrap();

        let duplicate = database.append_batch(
            SourceType::Folder,
            r"D:\test",
            &[row("file:a", 1), row("file:a", 2)],
            |_| Ok(()),
        );
        assert!(matches!(
            duplicate,
            Err(DatabaseError::DuplicateIdentityInBatch(identity)) if identity == "file:a"
        ));

        let empty = database.append_batch(
            SourceType::Folder,
            r"D:\test",
            &[row("  ", 1)],
            |_| Ok(()),
        );
        assert!(matches!(empty, Err(DatabaseError::EmptyIdentity)));
        assert_eq!(database.library_summary().unwrap().batch_count, 0);
    }

    #[test]
    fn finalize_failure_rolls_back_the_entire_batch() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(2));

        let result = database.append_batch(
            SourceType::Folder,
            r"D:\more",
            &[row("file:new", 1)],
            |_| Err("无法重命名暂存目录".to_owned()),
        );

        assert!(matches!(
            result,
            Err(DatabaseError::BatchFinalizeFailed(message)) if message.contains("暂存")
        ));
        let summary = database.library_summary().unwrap();
        assert_eq!(summary.row_count, 2);
        assert_eq!(summary.batch_count, 1);
    }

    #[test]
    fn filters_existing_identities_for_candidates() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(3));

        let existing = database
            .existing_identities(&[
                r"file:d:\test\1.png".to_owned(),
                r"file:d:\test\3.png".to_owned(),
                "file:unknown".to_owned(),
            ])
            .unwrap();

        assert_eq!(existing.len(), 2);
        assert!(existing.contains(r"file:d:\test\1.png"));
        assert!(existing.contains(r"file:d:\test\3.png"));
    }

    #[test]
    fn lists_batches_in_reverse_import_order() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(2));
        database
            .append_batch(
                SourceType::Xlsx,
                r"D:\legacy.xlsx",
                &[row("xlsxrow:legacy.xlsx!2", 2)],
                |_| Ok(()),
            )
            .unwrap();

        let batches = database.list_batches().unwrap();

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].source_type, SourceType::Xlsx);
        assert_eq!(batches[1].source_type, SourceType::Folder);
    }
}
