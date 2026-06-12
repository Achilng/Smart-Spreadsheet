pub const CURRENT_SCHEMA_VERSION: u32 = 3;

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
