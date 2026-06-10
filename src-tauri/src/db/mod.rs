mod images;
mod migrations;
mod query;
mod tags;
mod workbook;

pub use query::{MAX_PAGE_SIZE, RowPage, RowQuery, RowRecord, TagMatchMode, TagSummary};
pub use tags::{RowSelection, TagMutationError, TagMutationResult};
pub use workbook::WorkbookSummary;

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, MAIN_DB, TransactionBehavior};
use thiserror::Error;

pub use images::RowImageLocator;
pub use migrations::CURRENT_SCHEMA_VERSION;
use migrations::MIGRATION_1;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("SQLite 操作失败: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("数据库版本 {found} 高于当前支持版本 {supported}")]
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    #[error("数据库完整性检查失败: {0}")]
    IntegrityCheckFailed(String),
    #[error("工作簿行数超出 SQLite 可表示范围")]
    RowCountOverflow,
    #[error("分页大小 {requested} 无效，允许范围为 1..={maximum}")]
    InvalidPageSize { requested: u32, maximum: u32 },
    #[error("分页偏移超出 SQLite 可表示范围")]
    OffsetOverflow,
    #[error("查询计数超出可表示范围")]
    CountOverflow,
    #[error("不存在的行 ID: {0}")]
    RowNotFound(i64),
}

pub struct Database {
    connection: Connection,
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
             PRAGMA synchronous = NORMAL;",
        )?;
        migrate(&mut connection)?;
        Ok(Self { connection })
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

    if version == 0 {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(MIGRATION_1)?;
        transaction.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
        transaction.commit()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::{Connection, ErrorCode, params};

    use super::*;

    #[test]
    fn initializes_v1_schema_and_foreign_keys() {
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
            vec!["row_tags", "rows", "settings", "tags", "workbook"]
        );
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
    fn deleting_workbook_cascades_rows_and_row_tags() {
        let database = Database::open_in_memory().unwrap();
        let connection = &database.connection;
        connection
            .execute(
                "INSERT INTO workbook(id, imported_name, imported_at, sheet_name, row_count)
                 VALUES (1, 'sample.xlsx', '2026-06-10T00:00:00Z', 'NovelAI Metadata', 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO rows(workbook_id, source_row, positive_prompt) VALUES (1, 2, 'test')",
                [],
            )
            .unwrap();
        connection
            .execute("INSERT INTO tags(name) VALUES ('keep')", [])
            .unwrap();
        connection
            .execute("INSERT INTO row_tags(row_id, tag_id) VALUES (1, 1)", [])
            .unwrap();

        connection
            .execute("DELETE FROM workbook WHERE id = 1", [])
            .unwrap();

        let counts = connection
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM rows),
                    (SELECT COUNT(*) FROM row_tags),
                    (SELECT COUNT(*) FROM tags)",
                [],
                |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, u32>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(counts, (0, 0, 1));
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
