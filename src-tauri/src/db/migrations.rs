pub const CURRENT_SCHEMA_VERSION: u32 = 7;

pub(super) const MIGRATION_1: &str = r#"
CREATE TABLE workbook (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    imported_name TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    sheet_name TEXT NOT NULL,
    row_count INTEGER NOT NULL CHECK (row_count >= 0)
) STRICT;

CREATE TABLE rows (
    id INTEGER PRIMARY KEY,
    workbook_id INTEGER NOT NULL DEFAULT 1
        REFERENCES workbook(id) ON DELETE CASCADE,
    source_row INTEGER NOT NULL CHECK (source_row > 0),
    time TEXT,
    positive_prompt TEXT,
    negative_prompt TEXT,
    artists TEXT,
    image_folder TEXT,
    image_path TEXT,
    embedded_image_ref TEXT,
    UNIQUE (workbook_id, source_row)
) STRICT;

CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL COLLATE BINARY,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (name)
) STRICT;

CREATE TABLE row_tags (
    row_id INTEGER NOT NULL REFERENCES rows(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (row_id, tag_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_rows_workbook_source_row ON rows(workbook_id, source_row);
CREATE INDEX idx_row_tags_tag_row ON row_tags(tag_id, row_id);
"#;

// v2：单工作簿模式改为追加式资料库。
// - workbook 单例表替换为 import_batches 批次表。
// - rows 增加批次、身份键（增量跳过）、受管副本路径和元数据失败标记；行 ID 保持不变，
//   row_tags 关联无需重建。
// - 旧行的嵌入图引用移入 pending_embedded_extractions，由存储层在打开数据目录时
//   从工作簿副本提取到受管 files/ 目录。
// - 身份键规则与 Rust 端 identity 模块保持一致：路径取 TRIM 后做 '/'→'\' 替换和
//   ASCII 小写化（SQLite LOWER 仅处理 ASCII，与 to_ascii_lowercase 行为一致）。
//   image_path 在旧表中重复或为空的行退化为 xlsxrow:<小写文件名>!<源行号>。
pub(super) const MIGRATION_2: &str = r#"
CREATE TABLE import_batches (
    id INTEGER PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('xlsx', 'folder', 'archive')),
    source_path TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    added_count INTEGER NOT NULL CHECK (added_count >= 0),
    skipped_count INTEGER NOT NULL CHECK (skipped_count >= 0)
) STRICT;

INSERT INTO import_batches (id, source_type, source_path, imported_at, added_count, skipped_count)
SELECT 1, 'xlsx', imported_name, imported_at, row_count, 0 FROM workbook;

CREATE TABLE rows_v2 (
    id INTEGER PRIMARY KEY,
    batch_id INTEGER NOT NULL REFERENCES import_batches(id),
    source_ordinal INTEGER NOT NULL CHECK (source_ordinal > 0),
    identity TEXT NOT NULL,
    source_size INTEGER,
    source_mtime INTEGER,
    time TEXT,
    positive_prompt TEXT,
    negative_prompt TEXT,
    artists TEXT,
    image_folder TEXT,
    image_path TEXT,
    stored_image_path TEXT,
    metadata_failed INTEGER NOT NULL DEFAULT 0 CHECK (metadata_failed IN (0, 1)),
    UNIQUE (identity)
) STRICT;

WITH path_keys AS (
    SELECT id, 'file:' || LOWER(REPLACE(TRIM(image_path), '/', '\')) AS key
    FROM rows
    WHERE TRIM(COALESCE(image_path, '')) <> ''
),
unique_keys AS (
    SELECT key FROM path_keys GROUP BY key HAVING COUNT(*) = 1
)
INSERT INTO rows_v2 (id, batch_id, source_ordinal, identity, time, positive_prompt,
                     negative_prompt, artists, image_folder, image_path)
SELECT r.id, 1, r.source_row,
       COALESCE(
           (SELECT pk.key
            FROM path_keys pk
            JOIN unique_keys uk ON uk.key = pk.key
            WHERE pk.id = r.id),
           'xlsxrow:' || LOWER((SELECT imported_name FROM workbook)) || '!' || r.source_row),
       r.time, r.positive_prompt, r.negative_prompt, r.artists, r.image_folder, r.image_path
FROM rows r;

CREATE TABLE pending_embedded_extractions (
    row_id INTEGER PRIMARY KEY,
    media_path TEXT NOT NULL
) STRICT, WITHOUT ROWID;

INSERT INTO pending_embedded_extractions (row_id, media_path)
SELECT id, TRIM(embedded_image_ref)
FROM rows
WHERE TRIM(COALESCE(embedded_image_ref, '')) <> '';

DROP TABLE rows;
ALTER TABLE rows_v2 RENAME TO rows;
DROP TABLE workbook;

CREATE INDEX idx_rows_batch ON rows(batch_id);
"#;

// v3：为按图片文件内容全库去重增加 SHA-256 哈希列。
// 历史行在存储层打开数据目录后按可读图片来源补算；无法读取的行保持 NULL。
// 索引必须允许重复值，迁移期间和补算完成前都可能暂时存在相同内容的历史行。
pub(super) const MIGRATION_3: &str = r#"
ALTER TABLE rows ADD COLUMN content_hash TEXT;
CREATE INDEX idx_rows_content_hash ON rows(content_hash)
WHERE content_hash IS NOT NULL;
"#;

// v4：为以图搜图增加感知哈希列（64 位 pHash，存储为 16 字符十六进制字符串）。
// 历史行在用户手动触发刷新后补算；无法读取的行保持 NULL。
pub(super) const MIGRATION_4: &str = r#"
ALTER TABLE rows ADD COLUMN perceptual_hash TEXT;
CREATE INDEX idx_rows_perceptual_hash ON rows(perceptual_hash)
WHERE perceptual_hash IS NOT NULL;
"#;

// v5：持久化分组系统。每行可属于至多一个用户自定义分组，删除分组时成员变为未分组。
pub(super) const MIGRATION_5: &str = r#"
CREATE TABLE groups (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

ALTER TABLE rows ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;
CREATE INDEX idx_rows_group_id ON rows(group_id) WHERE group_id IS NOT NULL;
"#;

// v6：重复聚合 cluster 的自定义别名。
pub(super) const MIGRATION_6: &str = r#"
CREATE TABLE dedupe_aliases (
    mode TEXT NOT NULL CHECK (mode IN ('artists', 'positivePrompt')),
    key TEXT NOT NULL,
    alias TEXT NOT NULL,
    PRIMARY KEY (mode, key)
) STRICT, WITHOUT ROWID;
"#;

// v7：NovelAI V4 角色描述与基础正向提示词分开存储。
// 历史数据不做启发式拆分，避免把用户原有的换行提示词误判为角色描述；
// 用户可通过“更新现有图片”从原 PNG 元数据准确回填。
pub(super) const MIGRATION_7: &str = r#"
ALTER TABLE rows ADD COLUMN character_prompt TEXT;
"#;
