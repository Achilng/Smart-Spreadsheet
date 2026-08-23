mod batches;
mod artist_auto_prefix;
mod compare;
mod automation_rules;
mod delete;
mod export;
mod groups;
mod hashes;
mod history;
pub mod identity;
mod image_updates;
mod images;
mod library_filters;
mod metadata_fingerprints;
mod migrations;
mod notes;
mod prompt_edit;
mod quick_edit;
mod query;
mod settings;
mod tags;

pub use batches::{AppendOutcome, BatchSummary, LibrarySummary, NewRow, SourceType};
pub use compare::{
    COMPARE_MODEL_SECTION_CAP, CompareModelSection, CompareSample, CompareSectionPage,
};
pub use artist_auto_prefix::{
    ArtistTextPrefixResult, AutoArtistCandidate, AutoArtistPrefixApplyResult,
    AutoArtistPrefixPreview,
};
pub use automation_rules::*;
pub use delete::DeleteOutcome;
pub use export::ExportRow;
pub use groups::GroupSummary;
pub use hashes::ContentHashCandidate;
pub use history::MutableRowState;
pub use image_updates::{ExistingImageTarget, ExistingImageUpdate};
pub use images::RowImageLocator;
pub use library_filters::*;
pub use migrations::CURRENT_SCHEMA_VERSION;
pub use prompt_edit::{PromptEditResult, SinglePromptEditResult};
pub use quick_edit::{
    QuickArtistPrefixApplyResult, QuickArtistPrefixChange, QuickArtistPrefixPreview,
    QuickEditCondition, QuickEditError, QuickEditTextField, QuickGroupApplyResult,
    QuickGroupChange, QuickGroupPreview, QuickTagApplyResult, QuickTagAssociation,
    QuickTagPreview,
};
pub use query::{DedupeCluster, DedupeMode, MAX_PAGE_SIZE, RowPage, RowQuery, RowRecord, SortMode, TagMatchMode, TagSummary};
pub use settings::{ImageExportRenameMode, ImageExportSettings};
pub use tags::{RowSelection, TagMutationError, TagMutationResult, TagSelectionSummary};

use std::path::Path;
use std::time::Duration;

use migrations::{
    MIGRATION_10, MIGRATION_11, MIGRATION_12, MIGRATION_14, MIGRATION_15, MIGRATION_16,
    MIGRATION_17, MIGRATION_9, MINIMUM_UPGRADABLE_SCHEMA_VERSION, SCHEMA_17,
};
use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use thiserror::Error;

const ARTIST_STRING_FORMAT_SETTING: &str = "artist_string_format_version";
const CURRENT_ARTIST_STRING_FORMAT_VERSION: &str = "2";

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
        self.backup_to_with_progress(destination, |_, _| {})?;
        Ok(())
    }

    /// 使用 SQLite Online Backup 分批复制数据库，每批回报已完成/总页数。
    /// BUSY/LOCKED 只表示源库暂时正在写入，稍后重试而不直接中止迁移。
    pub fn backup_to_with_progress(
        &self,
        destination: impl AsRef<Path>,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<(), DatabaseError> {
        let mut destination = Connection::open(destination)?;
        let backup = Backup::new(&self.connection, &mut destination)?;

        loop {
            let step = backup.step(100)?;
            let state = backup.progress();
            let total = u64::try_from(state.pagecount).unwrap_or_default();
            let remaining = u64::try_from(state.remaining).unwrap_or_default();
            progress(total.saturating_sub(remaining), total);
            match step {
                StepResult::Done => return Ok(()),
                StepResult::More => {}
                StepResult::Busy | StepResult::Locked => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                _ => std::thread::sleep(Duration::from_millis(50)),
            }
        }
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
        repair_legacy_artist_strings(&mut connection)?;
        Ok(Self {
            connection,
            query_cache: None,
        })
    }
}

/// 早期提示词编辑会把多画师串保存为逗号分隔，导致“单画师串”筛选把整行
/// 误判为一个画师。新版首次打开资料库时从现有正向/角色提示词重算这些异常行，
/// 并写入一次性格式标记；无法从提示词重建的历史值保持原样。
fn repair_legacy_artist_strings(connection: &mut Connection) -> Result<(), DatabaseError> {
    let format_version = connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [ARTIST_STRING_FORMAT_SETTING],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if format_version.as_deref() == Some(CURRENT_ARTIST_STRING_FORMAT_VERSION) {
        return Ok(());
    }

    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let candidates = {
        let mut statement = transaction.prepare(
            "SELECT id, positive_prompt, character_prompt
             FROM rows
             WHERE INSTR(COALESCE(artists, ''), ',') > 0
             ORDER BY id",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    {
        let mut update = transaction.prepare("UPDATE rows SET artists = ?2 WHERE id = ?1")?;
        for (row_id, positive_prompt, character_prompt) in candidates {
            let Some(artists) = prompt_edit::combined_artists(
                positive_prompt.as_deref().unwrap_or(""),
                character_prompt.as_deref(),
            ) else {
                continue;
            };
            update.execute(rusqlite::params![row_id, artists])?;
        }
    }
    transaction.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [
            ARTIST_STRING_FORMAT_SETTING,
            CURRENT_ARTIST_STRING_FORMAT_VERSION,
        ],
    )?;
    transaction.commit()?;
    Ok(())
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
        transaction.execute_batch(SCHEMA_17)?;
        version = 17;
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
    if version == 11 {
        transaction.execute_batch(MIGRATION_12)?;
        version = 12;
    }
    if version == 12 {
        // v13 曾只增加现已退役的外部词典缓存；从旧版直接升级时无需再创建。
        version = 13;
    }
    if version == 13 {
        transaction.execute_batch(MIGRATION_14)?;
        version = 14;
    }
    if version == 14 {
        transaction.execute_batch(MIGRATION_15)?;
        version = 15;
    }
    if version == 15 {
        transaction.execute_batch(MIGRATION_16)?;
        version = 16;
    }
    if version == 16 {
        transaction.execute_batch(MIGRATION_17)?;
        version = 17;
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
                "automation_rules",
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
    fn reopening_repairs_comma_separated_artist_strings_once() {
        let temporary = TemporaryDatabase::new();
        {
            let mut database = Database::open(&temporary.path).unwrap();
            test_support::append_rows(
                &mut database,
                &[
                    NewRow {
                        source_ordinal: 1,
                        identity: "legacy-comma-artists".into(),
                        positive_prompt: Some(
                            "1.6::artist:ibuki_satsuki::, 2::artist:satoi::, artist:z3zz4, \
                             artist:yumenouchi_chiharu, artist:hinatsu, \
                             1.2::artist:smusmuve::, artist:koujisako, \
                             2::artist:memuro::, artist:wanke"
                                .into(),
                        ),
                        artists: Some(
                            "1.6::artist:ibuki_satsuki::, 2::artist:satoi::, artist:z3zz4, \
                             artist:yumenouchi_chiharu, artist:hinatsu, \
                             1.2::artist:smusmuve::, artist:koujisako, \
                             2::artist:memuro::, artist:wanke"
                                .into(),
                        ),
                        ..NewRow::default()
                    },
                    NewRow {
                        source_ordinal: 2,
                        identity: "canonical-single-artist".into(),
                        positive_prompt: Some("artist:solo".into()),
                        artists: Some("artist:solo".into()),
                        ..NewRow::default()
                    },
                ],
            );
            database
                .connection
                .execute(
                    "DELETE FROM settings WHERE key = ?1",
                    [ARTIST_STRING_FORMAT_SETTING],
                )
                .unwrap();
        }

        let mut database = Database::open(&temporary.path).unwrap();
        let repaired: String = database
            .connection
            .query_row("SELECT artists FROM rows WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            repaired,
            "1.6::artist:ibuki_satsuki::\n2::artist:satoi::\nartist:z3zz4\n\
             artist:yumenouchi_chiharu\nartist:hinatsu\n1.2::artist:smusmuve::\n\
             artist:koujisako\n2::artist:memuro::\nartist:wanke"
        );
        assert_eq!(
            database
                .connection
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [ARTIST_STRING_FORMAT_SETTING],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            CURRENT_ARTIST_STRING_FORMAT_VERSION
        );

        let page = database
            .query_rows(&RowQuery {
                offset: 0,
                limit: 10,
                tags: Vec::new(),
                tag_mode: TagMatchMode::And,
                dedupe: DedupeMode::None,
                single_artist_only: true,
                artist_filter: String::new(),
                has_vibe: false,
                untagged_only: false,
                filters: vec![],
                group_view: false,
                hide_grouped: false,
                search: String::new(),
            })
            .unwrap();
        assert_eq!(page.rows.iter().map(|row| row.id).collect::<Vec<_>>(), vec![2]);
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
    fn upgrades_v8_through_v15_and_marks_folder_copy_as_original() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&connection);

        migrate(&mut connection).unwrap();

        let row: (String, bool, Option<String>, Option<u32>, String) = connection
            .query_row(
                "SELECT import_batches.source_type, rows.stored_image_is_original,
                        rows.metadata_fingerprint, rows.vibe_reference_count, rows.updated_at
                 FROM rows JOIN import_batches ON import_batches.id = rows.batch_id
                 WHERE rows.id = 7",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "folder".into(),
                true,
                None,
                None,
                "2026-07-01T00:00:00.000Z".into()
            )
        );
        verify_foreign_keys(&connection).unwrap();
    }

    #[test]
    fn upgrading_v14_removes_retired_artist_cache_tables() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(&schema_15_fixture()).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE artist_dictionary_names (
                    match_name TEXT PRIMARY KEY,
                    display_name TEXT NOT NULL,
                    canonical_name TEXT NOT NULL,
                    post_count INTEGER NOT NULL,
                    is_banned INTEGER NOT NULL,
                    is_deprecated INTEGER NOT NULL,
                    is_ambiguous INTEGER NOT NULL,
                    source_mask INTEGER NOT NULL
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE artist_dictionary_sync (
                    singleton INTEGER PRIMARY KEY,
                    synced_at TEXT NOT NULL,
                    tag_count INTEGER NOT NULL,
                    artist_count INTEGER NOT NULL,
                    alias_count INTEGER NOT NULL,
                    name_count INTEGER NOT NULL
                 ) STRICT;
                 INSERT INTO artist_dictionary_names VALUES
                    ('old_name', 'old_name', 'old_name', 1, 0, 0, 0, 1);
                 PRAGMA user_version = 14;",
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        assert!(!table_exists(&connection, "artist_dictionary_names"));
        assert!(!table_exists(&connection, "artist_dictionary_sync"));
        assert!(table_exists(&connection, "automation_rules"));
        assert_eq!(
            connection
                .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn upgrading_v16_adds_style_signature_column_and_index() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(&format!("{} PRAGMA user_version = 16;", schema_16_fixture()))
            .unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(
            connection
                .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
        // 既有数据在新列上保持 NULL，等待启动回填。
        connection
            .execute_batch(
                "INSERT INTO import_batches
                    (id, source_type, source_path, imported_at, added_count, skipped_count)
                 VALUES (1, 'folder', 'D:\\images', '2026-08-01T00:00:00Z', 1, 0);
                 INSERT INTO rows (id, batch_id, source_ordinal, identity, positive_prompt)
                 VALUES (7, 1, 1, 'file:d:\\images\\one.png', 'artist:test, blue hair');",
            )
            .unwrap();
        let signature: Option<String> = connection
            .query_row("SELECT style_signature FROM rows WHERE id = 7", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(signature, None);
        let index_exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_rows_style_signature'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index_exists, 1);
    }

    /// 全表重算并断言与存量签名一致：任何写 positive_prompt 后漏刷
    /// style_signature 的路径都会在此暴露。
    fn assert_all_style_signatures_consistent(database: &Database) {
        for (row_id, prompt) in database.all_row_positive_prompts().unwrap() {
            assert_eq!(
                database.row_style_signature(row_id).unwrap(),
                crate::pipeline::style_signature_of(prompt.as_deref()),
                "row {row_id} 的存量画风签名与按提示词重算的结果不一致"
            );
        }
    }

    #[test]
    fn style_signatures_stay_consistent_across_all_prompt_write_paths() {
        let mut database = Database::open_in_memory().unwrap();

        // 1. 导入新图（append_batch 的 INSERT 现算签名）。
        database
            .append_batch(
                SourceType::Folder,
                r"D:\t",
                &[
                    NewRow {
                        source_ordinal: 1,
                        identity: "file:a.png".into(),
                        positive_prompt: Some("artist:one, blue hair, solo".into()),
                        ..NewRow::default()
                    },
                    NewRow {
                        source_ordinal: 2,
                        identity: "file:b.png".into(),
                        positive_prompt: Some("artist:two, solo, red hair, very aesthetic".into()),
                        ..NewRow::default()
                    },
                    NewRow {
                        source_ordinal: 3,
                        identity: "file:c.png".into(),
                        ..NewRow::default()
                    },
                ],
                |_| Ok(()),
            )
            .unwrap();
        assert_all_style_signatures_consistent(&database);

        // 2. 详情面板单行编辑（含质量词剥离后的签名变化）。
        database
            .update_positive_prompt(1, "artist:one, azure hair, solo, best quality")
            .unwrap();
        assert_all_style_signatures_consistent(&database);

        // 3. 批量查找替换。
        database
            .find_replace_prompt(
                &RowSelection::Explicit {
                    row_ids: vec![1, 2],
                },
                "hair",
                "eyes",
            )
            .unwrap();
        assert_all_style_signatures_consistent(&database);

        // 4. 批量画师前缀。
        database
            .prepend_artist(
                &RowSelection::Explicit {
                    row_ids: vec![2],
                },
                "solo",
            )
            .unwrap();
        assert_all_style_signatures_consistent(&database);

        // 5. 快速整理画师前缀：应用 → 撤销 → 重做。
        let applied = database.apply_quick_artist_prefix("solo").unwrap();
        assert_all_style_signatures_consistent(&database);
        database
            .revert_quick_artist_prefix_changes(&applied.changes)
            .unwrap();
        assert_all_style_signatures_consistent(&database);
        database
            .reapply_quick_artist_prefix_changes(&applied.changes)
            .unwrap();
        assert_all_style_signatures_consistent(&database);

        // 6. 画师前缀修正（全库 + 导入后指定行）。
        database.apply_auto_artist_prefix(&["solo".into()]).unwrap();
        assert_all_style_signatures_consistent(&database);
        database
            .apply_confirmed_artist_prefix_to_rows(&[1, 2, 3])
            .unwrap();
        assert_all_style_signatures_consistent(&database);

        // 7. 自动规则的提示词任务。
        database
            .create_automation_rule(&AutomationRuleDraft {
                name: "append tag".into(),
                description: String::new(),
                enabled: true,
                run_on_import: true,
                run_on_update: false,
                conditions: RuleConditionSet {
                    mode: RuleMatchMode::All,
                    negate: false,
                    groups: vec![RuleConditionGroup {
                        mode: RuleMatchMode::All,
                        conditions: vec![RuleCondition::Prompt {
                            scope: PromptScope::Positive,
                            operator: PromptOperator::TextContains,
                            value: "azure".into(),
                            case_sensitive: false,
                        }],
                    }],
                },
                actions: vec![RuleAction::AppendPrompt {
                    field: PromptActionField::Positive,
                    value: "smile".into(),
                }],
            })
            .unwrap();
        database
            .execute_automation_rules(RuleExecutionTrigger::Manual, &[1])
            .unwrap();
        assert_all_style_signatures_consistent(&database);

        // 8. 撤销/重做恢复行状态。
        let before = database.get_rows_by_ids(&[1]).unwrap().remove(0);
        database
            .restore_mutable_row_states(&[MutableRowState {
                row_id: 1,
                positive_prompt: Some("artist:one, restored".into()),
                character_prompt: before.character_prompt.clone(),
                negative_prompt: before.negative_prompt.clone(),
                note: before.note.clone(),
                artists: before.artists.clone(),
                tags: before.tags.clone(),
                group_id: before.group_id,
            }])
            .unwrap();
        assert_all_style_signatures_consistent(&database);

        // 9. 更新现有图片。
        database
            .update_existing_images(&[ExistingImageUpdate {
                row_id: 1,
                identity: "file:a.png".into(),
                image_path: r"D:\t\a.png".into(),
                source_size: Some(10),
                source_mtime: Some(20),
                positive_prompt: Some("artist:one, updated prompt".into()),
                character_prompt: None,
                negative_prompt: None,
                artists: Some("artist:one".into()),
                content_hash: Some("hash".into()),
                perceptual_hash: None,
                metadata_fingerprint: None,
                stored_image_path: None,
                stored_image_is_original: true,
                vibe_reference_count: 0,
                vibe_signature: None,
                image_width: None,
                image_height: None,
                generation_model: None,
                generation_sampler: None,
                generation_steps: None,
                generation_seed: None,
                generation_scale: None,
                generation_cfg_rescale: None,
                generation_noise_schedule: None,
            }])
            .unwrap();
        assert_all_style_signatures_consistent(&database);
    }

    /// 从当前 schema 还原 v15 结构，供升级测试构造历史版本数据库。
    fn schema_15_fixture() -> String {
        schema_16_fixture()
            .replace("\n    vibe_signature TEXT,", "")
            .replace(
                "CREATE INDEX idx_rows_vibe_signature ON rows(vibe_signature)\nWHERE vibe_signature IS NOT NULL;\n",
                "",
            )
            .replace(
                "('artists', 'positivePrompt', 'vibes')",
                "('artists', 'positivePrompt')",
            )
    }

    /// 从当前 schema 还原 v16 结构，供升级测试构造历史版本数据库。
    fn schema_16_fixture() -> String {
        SCHEMA_17
            .replace("\n    style_signature TEXT,", "")
            .replace(
                "CREATE INDEX idx_rows_style_signature ON rows(style_signature)\nWHERE style_signature IS NOT NULL;\n",
                "",
            )
    }

    fn create_v9_fixture(connection: &Connection) {
        let base = schema_15_fixture();
        let schema = base
            .split("CREATE TABLE automation_rules")
            .next()
            .unwrap()
            .replace(
                ",\n    image_width INTEGER CHECK (image_width IS NULL OR image_width > 0),\n    image_height INTEGER CHECK (image_height IS NULL OR image_height > 0),\n    generation_model TEXT,\n    generation_sampler TEXT,\n    generation_steps INTEGER CHECK (generation_steps IS NULL OR generation_steps >= 0),\n    generation_seed TEXT,\n    generation_scale REAL,\n    generation_cfg_rescale REAL,\n    generation_noise_schedule TEXT",
                "",
            )
            .replace(
                ",\n    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                "",
            )
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
        let base = schema_15_fixture();
        let schema = base
            .split("CREATE TABLE automation_rules")
            .next()
            .unwrap()
            .replace(
                ",\n    image_width INTEGER CHECK (image_width IS NULL OR image_width > 0),\n    image_height INTEGER CHECK (image_height IS NULL OR image_height > 0),\n    generation_model TEXT,\n    generation_sampler TEXT,\n    generation_steps INTEGER CHECK (generation_steps IS NULL OR generation_steps >= 0),\n    generation_seed TEXT,\n    generation_scale REAL,\n    generation_cfg_rescale REAL,\n    generation_noise_schedule TEXT",
                "",
            )
            .replace(
                ",\n    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                "",
            )
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
