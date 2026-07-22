pub const CURRENT_SCHEMA_VERSION: u32 = 13;
pub const MINIMUM_UPGRADABLE_SCHEMA_VERSION: u32 = 8;

/// 新资料库直接创建当前结构，不再重放早期工作簿/XLSX 导入迁移。
pub(super) const SCHEMA_13: &str = r#"
CREATE TABLE import_batches (
    id INTEGER PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('legacy', 'folder', 'archive')),
    source_path TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    added_count INTEGER NOT NULL CHECK (added_count >= 0),
    skipped_count INTEGER NOT NULL CHECK (skipped_count >= 0)
) STRICT;

CREATE TABLE groups (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE TABLE rows (
    id INTEGER PRIMARY KEY,
    batch_id INTEGER NOT NULL REFERENCES import_batches(id),
    source_ordinal INTEGER NOT NULL CHECK (source_ordinal > 0),
    identity TEXT NOT NULL UNIQUE,
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
    content_hash TEXT,
    perceptual_hash TEXT,
    group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL,
    character_prompt TEXT,
    note TEXT,
    metadata_fingerprint TEXT,
    stored_image_is_original INTEGER NOT NULL DEFAULT 0
        CHECK (stored_image_is_original IN (0, 1)),
    vibe_reference_count INTEGER
        CHECK (vibe_reference_count IS NULL OR vibe_reference_count >= 0),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
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

CREATE TABLE dedupe_aliases (
    mode TEXT NOT NULL CHECK (mode IN ('artists', 'positivePrompt')),
    key TEXT NOT NULL,
    alias TEXT NOT NULL,
    PRIMARY KEY (mode, key)
) STRICT, WITHOUT ROWID;

CREATE TABLE artist_dictionary_names (
    match_name TEXT PRIMARY KEY COLLATE BINARY,
    display_name TEXT NOT NULL,
    canonical_name TEXT NOT NULL,
    post_count INTEGER NOT NULL CHECK (post_count >= 0),
    is_banned INTEGER NOT NULL CHECK (is_banned IN (0, 1)),
    is_deprecated INTEGER NOT NULL CHECK (is_deprecated IN (0, 1)),
    is_ambiguous INTEGER NOT NULL CHECK (is_ambiguous IN (0, 1)),
    source_mask INTEGER NOT NULL CHECK (source_mask > 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE artist_dictionary_sync (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    synced_at TEXT NOT NULL,
    tag_count INTEGER NOT NULL CHECK (tag_count >= 0),
    artist_count INTEGER NOT NULL CHECK (artist_count >= 0),
    alias_count INTEGER NOT NULL CHECK (alias_count >= 0),
    name_count INTEGER NOT NULL CHECK (name_count >= 0)
) STRICT;

CREATE INDEX idx_rows_batch ON rows(batch_id);
CREATE INDEX idx_row_tags_tag_row ON row_tags(tag_id, row_id);
CREATE INDEX idx_rows_content_hash ON rows(content_hash)
WHERE content_hash IS NOT NULL;
CREATE INDEX idx_rows_perceptual_hash ON rows(perceptual_hash)
WHERE perceptual_hash IS NOT NULL;
CREATE INDEX idx_rows_group_id ON rows(group_id)
WHERE group_id IS NOT NULL;
CREATE INDEX idx_rows_metadata_fingerprint ON rows(metadata_fingerprint)
WHERE metadata_fingerprint IS NOT NULL;
CREATE INDEX idx_rows_updated_at ON rows(updated_at DESC, id DESC);
CREATE INDEX idx_artist_dictionary_canonical
ON artist_dictionary_names(canonical_name, match_name);

CREATE TRIGGER touch_row_after_user_edit
AFTER UPDATE OF positive_prompt, character_prompt, negative_prompt, note, group_id ON rows
WHEN OLD.positive_prompt IS NOT NEW.positive_prompt
  OR OLD.character_prompt IS NOT NEW.character_prompt
  OR OLD.negative_prompt IS NOT NEW.negative_prompt
  OR OLD.note IS NOT NEW.note
  OR OLD.group_id IS NOT NEW.group_id
BEGIN
    UPDATE rows SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER touch_row_after_tag_add
AFTER INSERT ON row_tags
BEGIN
    UPDATE rows SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.row_id;
END;

CREATE TRIGGER touch_row_after_tag_remove
AFTER DELETE ON row_tags
BEGIN
    UPDATE rows SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = OLD.row_id;
END;
"#;

/// v8 → v9：为原图搬家后的安全重新关联增加元数据指纹，并记录受管副本是否为原件。
pub(super) const MIGRATION_9: &str = r#"
ALTER TABLE rows ADD COLUMN metadata_fingerprint TEXT;
CREATE INDEX idx_rows_metadata_fingerprint ON rows(metadata_fingerprint)
WHERE metadata_fingerprint IS NOT NULL;

ALTER TABLE rows ADD COLUMN stored_image_is_original INTEGER NOT NULL DEFAULT 0
CHECK (stored_image_is_original IN (0, 1));
UPDATE rows
SET stored_image_is_original = 1
WHERE stored_image_path IS NOT NULL
  AND batch_id IN (
      SELECT id FROM import_batches WHERE source_type IN ('folder', 'archive')
  );
"#;

/// v9 → v10：把历史电子表格批次归档为只读 legacy 来源，并移除已经退役的
/// 内嵌缩略图提取队列表。迁移完成后运行时不再包含 XLSX 导入概念。
pub(super) const MIGRATION_10: &str = r#"
CREATE TABLE import_batches_v10 (
    id INTEGER PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('legacy', 'folder', 'archive')),
    source_path TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    added_count INTEGER NOT NULL CHECK (added_count >= 0),
    skipped_count INTEGER NOT NULL CHECK (skipped_count >= 0)
) STRICT;

INSERT INTO import_batches_v10
    (id, source_type, source_path, imported_at, added_count, skipped_count)
SELECT id,
       CASE WHEN source_type = 'xlsx' THEN 'legacy' ELSE source_type END,
       source_path, imported_at, added_count, skipped_count
FROM import_batches;

DROP TABLE import_batches;
ALTER TABLE import_batches_v10 RENAME TO import_batches;
DROP TABLE IF EXISTS pending_embedded_extractions;
"#;

/// v10 → v11：缓存 NovelAI Comment 中的 VIBE 引用数量，使筛选、分页和批量
/// 操作都能走同一个数据库结果集。历史行在数据目录打开后从图片文件回填。
pub(super) const MIGRATION_11: &str = r#"
ALTER TABLE rows ADD COLUMN vibe_reference_count INTEGER
CHECK (vibe_reference_count IS NULL OR vibe_reference_count >= 0);
"#;

/// v11 → v12：记录每张图片最近一次用户可见编辑时间，供资料库按“最近更新”
/// 排序。历史记录以最初导入时间回填；提示词、备注、分组与 Tag 变更自动触碰时间。
pub(super) const MIGRATION_12: &str = r#"
ALTER TABLE rows ADD COLUMN updated_at TEXT;
UPDATE rows
SET updated_at = COALESCE(
    strftime(
        '%Y-%m-%dT%H:%M:%fZ',
        (SELECT import_batches.imported_at
         FROM import_batches
         WHERE import_batches.id = rows.batch_id)
    ),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
);
CREATE INDEX idx_rows_updated_at ON rows(updated_at DESC, id DESC);

CREATE TRIGGER touch_row_after_user_edit
AFTER UPDATE OF positive_prompt, character_prompt, negative_prompt, note, group_id ON rows
WHEN OLD.positive_prompt IS NOT NEW.positive_prompt
  OR OLD.character_prompt IS NOT NEW.character_prompt
  OR OLD.negative_prompt IS NOT NEW.negative_prompt
  OR OLD.note IS NOT NEW.note
  OR OLD.group_id IS NOT NEW.group_id
BEGIN
    UPDATE rows SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER touch_row_after_tag_add
AFTER INSERT ON row_tags
BEGIN
    UPDATE rows SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.row_id;
END;

CREATE TRIGGER touch_row_after_tag_remove
AFTER DELETE ON row_tags
BEGIN
    UPDATE rows SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = OLD.row_id;
END;
"#;

/// v12 → v13：缓存 Danbooru 画师 Tag、现存画师的其它名称及历史别名，供全库
/// 裸画师 Tag 扫描使用。词典只保存公开 API 元数据，不保存作品或画师页面内容。
pub(super) const MIGRATION_13: &str = r#"
CREATE TABLE artist_dictionary_names (
    match_name TEXT PRIMARY KEY COLLATE BINARY,
    display_name TEXT NOT NULL,
    canonical_name TEXT NOT NULL,
    post_count INTEGER NOT NULL CHECK (post_count >= 0),
    is_banned INTEGER NOT NULL CHECK (is_banned IN (0, 1)),
    is_deprecated INTEGER NOT NULL CHECK (is_deprecated IN (0, 1)),
    is_ambiguous INTEGER NOT NULL CHECK (is_ambiguous IN (0, 1)),
    source_mask INTEGER NOT NULL CHECK (source_mask > 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE artist_dictionary_sync (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    synced_at TEXT NOT NULL,
    tag_count INTEGER NOT NULL CHECK (tag_count >= 0),
    artist_count INTEGER NOT NULL CHECK (artist_count >= 0),
    alias_count INTEGER NOT NULL CHECK (alias_count >= 0),
    name_count INTEGER NOT NULL CHECK (name_count >= 0)
) STRICT;

CREATE INDEX idx_artist_dictionary_canonical
ON artist_dictionary_names(canonical_name, match_name);
"#;
