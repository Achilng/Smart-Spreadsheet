mod batches;
mod delete;
mod export;
mod groups;
mod hashes;
mod image_updates;
pub mod identity;
mod images;
mod migrations;
mod prompt_edit;
mod query;
mod settings;
mod tags;

pub use batches::{AppendOutcome, BatchSummary, LibrarySummary, NewRow, SourceType};
pub use delete::DeleteOutcome;
pub use groups::GroupSummary;
pub use prompt_edit::{PromptEditResult, SinglePromptEditResult};
pub use query::{DedupeCluster, DedupeMode, MAX_PAGE_SIZE, RowPage, RowQuery, RowRecord, TagMatchMode, TagSummary};
pub use tags::{RowSelection, TagMutationError, TagMutationResult, TagSelectionSummary};

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, MAIN_DB, TransactionBehavior};
use thiserror::Error;

pub use export::ExportRow;
pub use hashes::ContentHashCandidate;
pub use image_updates::{ExistingImageTarget, ExistingImageUpdate};
pub use images::RowImageLocator;
pub use migrations::CURRENT_SCHEMA_VERSION;
use migrations::{MIGRATION_1, MIGRATION_2, MIGRATION_3, MIGRATION_4, MIGRATION_5, MIGRATION_6};

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("SQLite 操作失败: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("数据库版本 {found} 高于当前支持版本 {supported}")]
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    #[error("数据库完整性检查失败: {0}")]
    IntegrityCheckFailed(String),
    #[error("导入行数超出 SQLite 可表示范围")]
    RowCountOverflow,
    #[error("分页大小 {requested} 无效，允许范围为 1..={maximum}")]
    InvalidPageSize { requested: u32, maximum: u32 },
    #[error("分页偏移超出 SQLite 可表示范围")]
    OffsetOverflow,
    #[error("查询计数超出可表示范围")]
    CountOverflow,
    #[error("不存在的行 ID: {0}")]
    RowNotFound(i64),
    #[error("导入批次包含重复的行身份键: {0}")]
    DuplicateIdentityInBatch(String),
    #[error("导入批次包含空的行身份键")]
    EmptyIdentity,
    #[error("导入收尾失败: {0}")]
    BatchFinalizeFailed(String),
    #[error("分组名称不能为空")]
    EmptyGroupName,
    #[error("不存在的分组 ID: {0}")]
    GroupNotFound(i64),
}

#[derive(Debug)]
pub struct Database {
    connection: Connection,
    /// query_rows 的筛选结果缓存（物化在本连接的 temp 表里）。
    /// 任何可能影响查询结果的数据变更后必须调用 `bump_data_version` 清空。
    query_cache: Option<query::QueryCache>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DatabaseError> {
        let connection = Connection::open(path)?;
        Self::initialize(connection)
    }

    pub fn open_in_memory() -> Result<Self, DatabaseError> {
        let connection = Connection::open_in_memory()?;
        Self::initialize(connection)
    }

    /// 数据发生变更（本连接或其它连接）后调用，使筛选结果缓存失效。
    pub fn bump_data_version(&mut self) {
        self.query_cache = None;
    }

    pub fn schema_version(&self) -> Result<u32, DatabaseError> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn backup_to(&self, destination: impl AsRef<Path>) -> Result<(), DatabaseError> {
        self.connection.backup(MAIN_DB, destination, None)?;
        Ok(())
    }

    pub fn verify_integrity(&self) -> Result<(), DatabaseError> {
        let result: String = self
            .connection
            .query_row("PRAGMA integrity_check(1)", [], |row| row.get(0))?;
        if result == "ok" {
            Ok(())
        } else {
            Err(DatabaseError::IntegrityCheckFailed(result))
        }
    }

    fn initialize(mut connection: Connection) -> Result<Self, DatabaseError> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;",
        )?;
        migrate(&mut connection)?;
        Ok(Self {
            connection,
            query_cache: None,
        })
    }
}

fn migrate(connection: &mut Connection) -> Result<(), DatabaseError> {
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(DatabaseError::UnsupportedSchemaVersion {
            found: version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    if version == CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    // v2 迁移需要重建 rows 表。按 SQLite 文档要求在事务外关闭外键约束，
    // 否则 DROP 旧表会触发 row_tags 的级联删除。
    connection.pragma_update(None, "foreign_keys", false)?;
    let migration_result = apply_pending_migrations(connection, version);
    let restore_result = connection.pragma_update(None, "foreign_keys", true);
    migration_result?;
    restore_result?;
    verify_foreign_keys(connection)
}

fn apply_pending_migrations(
    connection: &mut Connection,
    from_version: u32,
) -> Result<(), DatabaseError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut version = from_version;
    if version == 0 {
        transaction.execute_batch(MIGRATION_1)?;
        version = 1;
    }
    if version == 1 {
        transaction.execute_batch(MIGRATION_2)?;
        version = 2;
    }
    if version == 2 {
        transaction.execute_batch(MIGRATION_3)?;
        version = 3;
    }
    if version == 3 {
        transaction.execute_batch(MIGRATION_4)?;
        version = 4;
    }
    if version == 4 {
        transaction.execute_batch(MIGRATION_5)?;
        version = 5;
    }
    if version == 5 {
        transaction.execute_batch(MIGRATION_6)?;
        version = 6;
    }
    debug_assert_eq!(version, CURRENT_SCHEMA_VERSION);
    transaction.pragma_update(None, "user_version", version)?;
    transaction.commit()?;
    Ok(())
}

fn verify_foreign_keys(connection: &Connection) -> Result<(), DatabaseError> {
    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    let mut violations = statement.query([])?;
    if violations.next()?.is_some() {
        return Err(DatabaseError::IntegrityCheckFailed(
            "迁移后存在失效的外键引用".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::batches::{NewRow, SourceType};
    use super::Database;

    /// 构造 `count` 行测试数据：身份键为 file:d:\test\<编号>.png，
    /// source_ordinal 与正向提示词按编号递增。
    pub(crate) fn test_rows(count: i64) -> Vec<NewRow> {
        (1..=count)
            .map(|index| NewRow {
                source_ordinal: u32::try_from(index + 1).unwrap(),
                identity: format!(r"file:d:\test\{index}.png"),
                time: Some(format!("time {index}")),
                positive_prompt: Some(format!("prompt {index}")),
                ..NewRow::default()
            })
            .collect()
    }

    /// 新建内存库并追加一个包含 `count` 行的 folder 批次。
    pub(crate) fn database_with_rows(count: i64) -> Database {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(count));
        database
    }

    pub(crate) fn append_rows(database: &mut Database, rows: &[NewRow]) {
        database
            .append_batch(SourceType::Folder, r"D:\test", rows, |_| Ok(()))
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::{Connection, ErrorCode, params};

    use super::*;

    #[test]
    fn initializes_v5_schema_and_foreign_keys() {
        let database = Database::open_in_memory().unwrap();

        assert_eq!(database.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        let foreign_keys: u32 = database
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);

        let mut tables = database
            .connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        let tables = tables
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            tables,
            vec![
                "dedupe_aliases",
                "groups",
                "import_batches",
                "pending_embedded_extractions",
                "row_tags",
                "rows",
                "settings",
                "tags"
            ]
        );

        let content_hash_column: (String, String, i64) = database
            .connection
            .query_row(
                "SELECT name, type, \"notnull\" FROM pragma_table_info('rows') WHERE name = 'content_hash'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            content_hash_column,
            ("content_hash".into(), "TEXT".into(), 0)
        );

        let content_hash_index: (String, i64, i64) = database
            .connection
            .query_row(
                "SELECT name, \"unique\", partial FROM pragma_index_list('rows') WHERE name = 'idx_rows_content_hash'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(content_hash_index, ("idx_rows_content_hash".into(), 0, 1));

        let phash_column: (String, String, i64) = database
            .connection
            .query_row(
                "SELECT name, type, \"notnull\" FROM pragma_table_info('rows') WHERE name = 'perceptual_hash'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            phash_column,
            ("perceptual_hash".into(), "TEXT".into(), 0)
        );

        let phash_index: (String, i64, i64) = database
            .connection
            .query_row(
                "SELECT name, \"unique\", partial FROM pragma_index_list('rows') WHERE name = 'idx_rows_perceptual_hash'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(phash_index, ("idx_rows_perceptual_hash".into(), 0, 1));
    }

    #[test]
    fn tag_names_are_case_sensitive_and_exact_duplicates_fail() {
        let database = Database::open_in_memory().unwrap();

        database
            .connection
            .execute(
                "INSERT INTO tags(name) VALUES (?1), (?2)",
                ["Landscape", "landscape"],
            )
            .unwrap();
        let count: u32 = database
            .connection
            .query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        let error = database
            .connection
            .execute("INSERT INTO tags(name) VALUES (?1)", ["Landscape"])
            .unwrap_err();
        assert!(matches!(
            error,
            rusqlite::Error::SqliteFailure(ref failure, _)
                if failure.code == ErrorCode::ConstraintViolation
        ));
    }

    #[test]
    fn persisted_database_reopens_without_reapplying_migration() {
        let temporary = TemporaryDatabase::new();
        {
            let database = Database::open(&temporary.path).unwrap();
            database
                .connection
                .execute(
                    "INSERT INTO settings(key, value) VALUES ('theme', 'dark')",
                    [],
                )
                .unwrap();
        }

        let database = Database::open(&temporary.path).unwrap();
        let value: String = database
            .connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params!["theme"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(database.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert_eq!(value, "dark");
    }

    #[test]
    fn rejects_database_from_a_newer_schema_version() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "user_version", 99).unwrap();

        let error = migrate(&mut connection).unwrap_err();

        assert!(matches!(
            error,
            DatabaseError::UnsupportedSchemaVersion {
                found: 99,
                supported: CURRENT_SCHEMA_VERSION
            }
        ));
    }

    #[test]
    fn upgrades_v1_database_preserving_rows_tags_and_order() {
        let temporary = TemporaryDatabase::new();
        create_v1_database(&temporary.path);

        let database = Database::open(&temporary.path).unwrap();

        assert_eq!(database.schema_version().unwrap(), 6);
        // 批次：旧工作簿转为唯一的 xlsx 批次。
        let batch: (String, String, i64) = database
            .connection
            .query_row(
                "SELECT source_type, source_path, added_count FROM import_batches",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(batch, ("xlsx".into(), "legacy.xlsx".into(), 3));

        // 行：ID 保持不变，唯一图片路径转为 file: 身份键，
        // 空路径与重复路径行退化为 xlsxrow: 身份键。
        let mut statement = database
            .connection
            .prepare(
                "SELECT id, source_ordinal, identity, positive_prompt
                 FROM rows ORDER BY id",
            )
            .unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                (
                    1,
                    2,
                    r"file:d:\images\one.png".to_owned(),
                    Some("first".to_owned())
                ),
                (2, 3, "xlsxrow:legacy.xlsx!3".to_owned(), None),
                (3, 4, "xlsxrow:legacy.xlsx!4".to_owned(), None),
            ]
        );

        // Tag 关联原样保留（行 ID 不变）。
        let tagged: i64 = database
            .connection
            .query_row(
                "SELECT row_tags.row_id FROM row_tags
                 JOIN tags ON tags.id = row_tags.tag_id
                 WHERE tags.name = 'keep'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tagged, 2);

        // 嵌入图引用进入待提取表。
        let pending: Vec<(i64, String)> = database
            .pending_embedded_extractions()
            .unwrap();
        assert_eq!(pending, vec![(1, "xl/media/image1.png".to_owned())]);
    }

    #[test]
    fn upgrades_v2_database_preserving_rows_and_tags_with_null_hash() {
        let temporary = TemporaryDatabase::new();
        create_v2_database(&temporary.path);

        let database = Database::open(&temporary.path).unwrap();

        assert_eq!(database.schema_version().unwrap(), 6);
        let row: (i64, String, Option<String>, Option<i64>) = database
            .connection
            .query_row(
                "SELECT id, positive_prompt, content_hash, group_id FROM rows",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(row, (7, "keep prompt".into(), None, None));

        let tag_name: String = database
            .connection
            .query_row(
                "SELECT tags.name FROM row_tags JOIN tags ON tags.id = row_tags.tag_id WHERE row_tags.row_id = 7",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tag_name, "keep tag");
    }

    #[test]
    fn upgrades_v3_database_adding_perceptual_hash_column() {
        let temporary = TemporaryDatabase::new();
        create_v3_database(&temporary.path);

        let database = Database::open(&temporary.path).unwrap();

        assert_eq!(database.schema_version().unwrap(), 6);
        let row: (i64, Option<String>, Option<String>) = database
            .connection
            .query_row(
                "SELECT id, content_hash, perceptual_hash FROM rows",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, (1, Some("abc123".into()), None));
    }

    #[test]
    fn upgrades_v4_database_adding_groups_table_and_group_id_column() {
        let temporary = TemporaryDatabase::new();
        create_v4_database(&temporary.path);

        let database = Database::open(&temporary.path).unwrap();

        assert_eq!(database.schema_version().unwrap(), 6);

        let tables: Vec<String> = database
            .connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(tables.contains(&"groups".to_owned()));

        let row: (i64, String, Option<i64>) = database
            .connection
            .query_row(
                "SELECT id, positive_prompt, group_id FROM rows",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, (1, "test prompt".into(), None));

        let tag_name: String = database
            .connection
            .query_row(
                "SELECT tags.name FROM row_tags JOIN tags ON tags.id = row_tags.tag_id WHERE row_tags.row_id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tag_name, "v4 tag");
    }

    /// 手工构造 v1 库：3 行数据。第 2、3 行 image_path 重复（必须退化为
    /// xlsxrow 身份键），第 1 行带嵌入图引用，第 2 行带 Tag。
    fn create_v1_database(path: &Path) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 BEGIN;
                 PRAGMA user_version = 0;
                 COMMIT;",
            )
            .unwrap();
        connection.execute_batch(MIGRATION_1).unwrap();
        connection
            .pragma_update(None, "user_version", 1)
            .unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO workbook (id, imported_name, imported_at, sheet_name, row_count)
                VALUES (1, 'legacy.xlsx', '2026-06-01T00:00:00Z', 'NovelAI Metadata', 3);
                INSERT INTO rows (workbook_id, source_row, positive_prompt, image_path, embedded_image_ref)
                VALUES (1, 2, 'first', 'D:/Images/One.PNG', 'xl/media/image1.png');
                INSERT INTO rows (workbook_id, source_row, image_path)
                VALUES (1, 3, 'D:\images\dup.png');
                INSERT INTO rows (workbook_id, source_row, image_path)
                VALUES (1, 4, 'D:\images\dup.png');
                INSERT INTO tags (name) VALUES ('keep');
                INSERT INTO row_tags (row_id, tag_id) VALUES (2, 1);
                "#,
            )
            .unwrap();
    }

    fn create_v2_database(path: &Path) {
        let connection = Connection::open(path).unwrap();
        connection.execute_batch(MIGRATION_1).unwrap();
        connection.execute_batch(MIGRATION_2).unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO import_batches
                    (id, source_type, source_path, imported_at, added_count, skipped_count)
                VALUES (1, 'folder', 'D:\legacy', '2026-06-12T00:00:00Z', 1, 0);
                INSERT INTO rows
                    (id, batch_id, source_ordinal, identity, positive_prompt, image_path)
                VALUES (7, 1, 1, 'file:d:\legacy\one.png', 'keep prompt', 'D:\legacy\one.png');
                INSERT INTO tags (id, name) VALUES (3, 'keep tag');
                INSERT INTO row_tags (row_id, tag_id) VALUES (7, 3);
                "#,
            )
            .unwrap();
    }

    fn create_v3_database(path: &Path) {
        let connection = Connection::open(path).unwrap();
        connection.execute_batch(MIGRATION_1).unwrap();
        connection.execute_batch(MIGRATION_2).unwrap();
        connection.execute_batch(MIGRATION_3).unwrap();
        connection.pragma_update(None, "user_version", 3).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO import_batches
                    (id, source_type, source_path, imported_at, added_count, skipped_count)
                VALUES (1, 'folder', 'D:\test', '2026-06-14T00:00:00Z', 1, 0);
                INSERT INTO rows
                    (id, batch_id, source_ordinal, identity, positive_prompt, content_hash)
                VALUES (1, 1, 1, 'file:d:\test\one.png', 'test prompt', 'abc123');
                "#,
            )
            .unwrap();
    }

    fn create_v4_database(path: &Path) {
        let connection = Connection::open(path).unwrap();
        connection.execute_batch(MIGRATION_1).unwrap();
        connection.execute_batch(MIGRATION_2).unwrap();
        connection.execute_batch(MIGRATION_3).unwrap();
        connection.execute_batch(MIGRATION_4).unwrap();
        connection.pragma_update(None, "user_version", 4).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO import_batches
                    (id, source_type, source_path, imported_at, added_count, skipped_count)
                VALUES (1, 'folder', 'D:\test', '2026-06-15T00:00:00Z', 1, 0);
                INSERT INTO rows
                    (id, batch_id, source_ordinal, identity, positive_prompt)
                VALUES (1, 1, 1, 'file:d:\test\one.png', 'test prompt');
                INSERT INTO tags (id, name) VALUES (1, 'v4 tag');
                INSERT INTO row_tags (row_id, tag_id) VALUES (1, 1);
                "#,
            )
            .unwrap();
    }

    struct TemporaryDatabase {
        path: PathBuf,
    }

    impl TemporaryDatabase {
        fn new() -> Self {
            let local_agent_temp = Path::new(r"D:\Agent\Agent_temp");
            let directory = if local_agent_temp.is_dir() {
                local_agent_temp.to_owned()
            } else {
                std::env::temp_dir()
            };
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            Self {
                path: directory.join(format!(
                    "smart-spreadsheet-db-{}-{nonce}.sqlite3",
                    std::process::id()
                )),
            }
        }
    }

    impl Drop for TemporaryDatabase {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_file(self.path.with_extension("sqlite3-wal"));
            let _ = fs::remove_file(self.path.with_extension("sqlite3-shm"));
        }
    }
}
