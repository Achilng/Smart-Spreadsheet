mod batches;
mod delete;
mod export;
mod groups;
mod hashes;
mod history;
pub mod identity;
mod image_updates;
mod images;
mod metadata_fingerprints;
mod migrations;
mod notes;
mod prompt_edit;
mod quick_edit;
mod query;
mod settings;
mod tags;

pub use batches::{AppendOutcome, BatchSummary, LibrarySummary, NewRow, SourceType};
pub use delete::DeleteOutcome;
pub use export::ExportRow;
pub use groups::GroupSummary;
pub use hashes::ContentHashCandidate;
pub use history::MutableRowState;
pub use image_updates::{ExistingImageTarget, ExistingImageUpdate};
pub use images::RowImageLocator;
pub use migrations::CURRENT_SCHEMA_VERSION;
pub use prompt_edit::{PromptEditResult, SinglePromptEditResult};
pub use quick_edit::{
    QuickEditCondition, QuickEditError, QuickEditTextField, QuickGroupApplyResult,
    QuickGroupChange, QuickGroupPreview, QuickTagApplyResult, QuickTagAssociation, QuickTagPreview,
};
pub use query::{DedupeCluster, DedupeMode, MAX_PAGE_SIZE, RowPage, RowQuery, RowRecord, TagMatchMode, TagSummary};
pub use tags::{RowSelection, TagMutationError, TagMutationResult, TagSelectionSummary};

use std::path::Path;
use std::time::Duration;

use migrations::{
    MIGRATION_9, MIGRATION_10, MIGRATION_11, MINIMUM_UPGRADABLE_SCHEMA_VERSION, SCHEMA_11,
};
use rusqlite::{Connection, MAIN_DB, TransactionBehavior};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("SQLite 操作失败: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("数据库版本 {found} 高于当前支持版本 {supported}")]
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    #[error("数据库版本 {found} 过旧；当前版本只支持从 v{minimum} 及以上升级，请先使用旧版应用升级数据库")]
    LegacySchemaVersion { found: u32, minimum: u32 },
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
    #[error("不存在的导入批次 ID: {0}")]
    BatchNotFound(i64),
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
    if version != 0 && version < MINIMUM_UPGRADABLE_SCHEMA_VERSION {
        return Err(DatabaseError::LegacySchemaVersion {
            found: version,
            minimum: MINIMUM_UPGRADABLE_SCHEMA_VERSION,
        });
    }

    // v10 会重建 import_batches。必须在事务外关闭外键约束，
    // 避免 DROP 旧表影响 rows 中保留的外键引用。
    connection.pragma_update(None, "foreign_keys", false)?;
    let migration_result = apply_pending_migrations(connection, version);
    let restore_result = connection.pragma_update(None, "foreign_keys", true);
    migration_result?;
    restore_result?;
    verify_foreign_keys(connection)
}

fn apply_pending_migrations(connection: &mut Connection, from_version: u32) -> Result<(), DatabaseError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut version = from_version;
    if version == 0 {
        transaction.execute_batch(SCHEMA_11)?;
        version = 11;
    }
    if version == 8 {
        transaction.execute_batch(MIGRATION_9)?;
        version = 9;
    }
    if version == 9 {
        transaction.execute_batch(MIGRATION_10)?;
        version = 10;
    }
    if version == 10 {
        transaction.execute_batch(MIGRATION_11)?;
        version = 11;
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
    use super::Database;
    use super::batches::{NewRow, SourceType};

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
    fn initializes_current_schema_without_legacy_tables() {
        let database = Database::open_in_memory().unwrap();

        assert_eq!(database.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        let foreign_keys: u32 = database
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);

        let tables = database
            .connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap()
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
                "row_tags",
                "rows",
                "settings",
                "tags"
            ]
        );
    }

    #[test]
    fn source_types_only_accept_current_runtime_and_legacy_values() {
        let database = Database::open_in_memory().unwrap();
        database
            .connection
            .execute(
                "INSERT INTO import_batches
                 (source_type, source_path, imported_at, added_count, skipped_count)
                 VALUES ('legacy', 'old source', '2026-07-17T00:00:00Z', 0, 0)",
                [],
            )
            .unwrap();

        let error = database
            .connection
            .execute(
                "INSERT INTO import_batches
                 (source_type, source_path, imported_at, added_count, skipped_count)
                 VALUES ('xlsx', 'removed', '2026-07-17T00:00:00Z', 0, 0)",
                [],
            )
            .unwrap_err();
        assert!(matches!(
            error,
            rusqlite::Error::SqliteFailure(ref failure, _)
                if failure.code == ErrorCode::ConstraintViolation
        ));
    }

    #[test]
    fn tag_names_are_case_sensitive_and_exact_duplicates_fail() {
        let database = Database::open_in_memory().unwrap();
        database
            .connection
            .execute("INSERT INTO tags(name) VALUES (?1), (?2)", ["Landscape", "landscape"])
            .unwrap();

        let count: u32 = database
            .connection
            .query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn persisted_database_reopens_without_reapplying_migration() {
        let temporary = TemporaryDatabase::new();
        {
            let database = Database::open(&temporary.path).unwrap();
            database
                .connection
                .execute("INSERT INTO settings(key, value) VALUES ('theme', 'dark')", [])
                .unwrap();
        }

        let database = Database::open(&temporary.path).unwrap();
        let value: String = database
            .connection
            .query_row("SELECT value FROM settings WHERE key = ?1", params!["theme"], |row| {
                row.get(0)
            })
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
    fn rejects_pre_v8_database_with_upgrade_guidance() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "user_version", 7).unwrap();

        let error = migrate(&mut connection).unwrap_err();
        assert!(matches!(
            error,
            DatabaseError::LegacySchemaVersion {
                found: 7,
                minimum: MINIMUM_UPGRADABLE_SCHEMA_VERSION
            }
        ));
        assert!(error.to_string().contains("请先使用旧版应用升级数据库"));
    }

    #[test]
    fn upgrades_v9_preserving_rows_tags_and_archiving_old_source_type() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v9_fixture(&connection);

        migrate(&mut connection).unwrap();

        let batch: (String, String) = connection
            .query_row(
                "SELECT source_type, source_path FROM import_batches WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(batch, ("legacy".into(), "legacy source".into()));

        let tagged: String = connection
            .query_row(
                "SELECT tags.name FROM row_tags
                 JOIN tags ON tags.id = row_tags.tag_id
                 WHERE row_tags.row_id = 7",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tagged, "keep");
        assert!(!table_exists(&connection, "pending_embedded_extractions"));
        verify_foreign_keys(&connection).unwrap();
    }

    #[test]
    fn upgrades_v8_through_v11_and_marks_folder_copy_as_original() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&connection);

        migrate(&mut connection).unwrap();

        let row: (String, bool, Option<String>, Option<u32>) = connection
            .query_row(
                "SELECT import_batches.source_type, rows.stored_image_is_original,
                        rows.metadata_fingerprint, rows.vibe_reference_count
                 FROM rows JOIN import_batches ON import_batches.id = rows.batch_id
                 WHERE rows.id = 7",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(row, ("folder".into(), true, None, None));
        verify_foreign_keys(&connection).unwrap();
    }

    fn create_v9_fixture(connection: &Connection) {
        let schema = SCHEMA_11
            .replace("('legacy', 'folder', 'archive')", "('xlsx', 'folder', 'archive')")
            .replace(
                ",\n    vibe_reference_count INTEGER\n        CHECK (vibe_reference_count IS NULL OR vibe_reference_count >= 0)",
                "",
            );
        connection.execute_batch(&schema).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE pending_embedded_extractions (
                    row_id INTEGER PRIMARY KEY,
                    media_path TEXT NOT NULL
                 ) STRICT, WITHOUT ROWID;
                 INSERT INTO import_batches
                    (id, source_type, source_path, imported_at, added_count, skipped_count)
                 VALUES (1, 'xlsx', 'legacy source', '2026-07-01T00:00:00Z', 1, 0);
                 INSERT INTO rows
                    (id, batch_id, source_ordinal, identity, positive_prompt)
                 VALUES (7, 1, 2, 'legacy-row:7', 'keep prompt');
                 INSERT INTO tags (id, name) VALUES (3, 'keep');
                 INSERT INTO row_tags (row_id, tag_id) VALUES (7, 3);
                 INSERT INTO pending_embedded_extractions (row_id, media_path)
                 VALUES (7, 'old-media.png');",
            )
            .unwrap();
        connection.pragma_update(None, "user_version", 9).unwrap();
    }

    fn create_v8_fixture(connection: &Connection) {
        let schema = SCHEMA_11
            .replace(
                "('legacy', 'folder', 'archive')",
                "('xlsx', 'folder', 'archive')",
            )
            .replace(
                ",\n    vibe_reference_count INTEGER\n        CHECK (vibe_reference_count IS NULL OR vibe_reference_count >= 0)",
                "",
            )
            .replace(
                "    note TEXT,\n    metadata_fingerprint TEXT,\n    stored_image_is_original INTEGER NOT NULL DEFAULT 0\n        CHECK (stored_image_is_original IN (0, 1))\n",
                "    note TEXT\n",
            )
            .replace(
                "CREATE INDEX idx_rows_metadata_fingerprint ON rows(metadata_fingerprint)\nWHERE metadata_fingerprint IS NOT NULL;\n",
                "",
            );
        connection.execute_batch(&schema).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE pending_embedded_extractions (
                    row_id INTEGER PRIMARY KEY,
                    media_path TEXT NOT NULL
                 ) STRICT, WITHOUT ROWID;
                 INSERT INTO import_batches
                    (id, source_type, source_path, imported_at, added_count, skipped_count)
                 VALUES (1, 'folder', 'D:\\images', '2026-07-01T00:00:00Z', 1, 0);
                 INSERT INTO rows
                    (id, batch_id, source_ordinal, identity, stored_image_path)
                 VALUES (7, 1, 1, 'file:d:\\images\\one.png', 'files/1/one.png');",
            )
            .unwrap();
        connection.pragma_update(None, "user_version", 8).unwrap();
    }

    fn table_exists(connection: &Connection, table: &str) -> bool {
        connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
                 )",
                [table],
                |row| row.get(0),
            )
            .unwrap()
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
                path: directory.join(format!("smart-spreadsheet-db-{}-{nonce}.sqlite3", std::process::id())),
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
