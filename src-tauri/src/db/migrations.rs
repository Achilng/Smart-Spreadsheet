pub const CURRENT_SCHEMA_VERSION: u32 = 17;
pub const MINIMUM_UPGRADABLE_SCHEMA_VERSION: u32 = 8;

/// 新资料库直接创建当前结构，不再重放早期工作簿/XLSX 导入迁移。
pub(super) const SCHEMA_17: &str = r#"
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
    vibe_signature TEXT,
    style_signature TEXT,
    image_width INTEGER CHECK (image_width IS NULL OR image_width > 0),
    image_height INTEGER CHECK (image_height IS NULL OR image_height > 0),
    generation_model TEXT,
    generation_sampler TEXT,
    generation_steps INTEGER CHECK (generation_steps IS NULL OR generation_steps >= 0),
    generation_seed TEXT,
    generation_scale REAL,
    generation_cfg_rescale REAL,
    generation_noise_schedule TEXT,
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
    mode TEXT NOT NULL CHECK (mode IN ('artists', 'positivePrompt', 'vibes')),
    key TEXT NOT NULL,
    alias TEXT NOT NULL,
    PRIMARY KEY (mode, key)
) STRICT, WITHOUT ROWID;

CREATE TABLE automation_rules (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
    run_on_import INTEGER NOT NULL DEFAULT 1 CHECK (run_on_import IN (0, 1)),
    run_on_update INTEGER NOT NULL DEFAULT 0 CHECK (run_on_update IN (0, 1)),
    conditions_json TEXT NOT NULL,
    actions_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
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
CREATE INDEX idx_rows_vibe_signature ON rows(vibe_signature)
WHERE vibe_signature IS NOT NULL;
CREATE INDEX idx_rows_style_signature ON rows(style_signature)
WHERE style_signature IS NOT NULL;
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

/// v13 → v14：保存可复用自动规则，并缓存规则条件需要的图片尺寸与 NovelAI
/// 生成参数。历史行保持 NULL；重新更新原图后会补齐可读取的字段。
pub(super) const MIGRATION_14: &str = r#"
ALTER TABLE rows ADD COLUMN image_width INTEGER
CHECK (image_width IS NULL OR image_width > 0);
ALTER TABLE rows ADD COLUMN image_height INTEGER
CHECK (image_height IS NULL OR image_height > 0);
ALTER TABLE rows ADD COLUMN generation_model TEXT;
ALTER TABLE rows ADD COLUMN generation_sampler TEXT;
ALTER TABLE rows ADD COLUMN generation_steps INTEGER
CHECK (generation_steps IS NULL OR generation_steps >= 0);
ALTER TABLE rows ADD COLUMN generation_seed TEXT;
ALTER TABLE rows ADD COLUMN generation_scale REAL;
ALTER TABLE rows ADD COLUMN generation_cfg_rescale REAL;
ALTER TABLE rows ADD COLUMN generation_noise_schedule TEXT;

CREATE TABLE automation_rules (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
    run_on_import INTEGER NOT NULL DEFAULT 1 CHECK (run_on_import IN (0, 1)),
    run_on_update INTEGER NOT NULL DEFAULT 0 CHECK (run_on_update IN (0, 1)),
    conditions_json TEXT NOT NULL,
    actions_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
"#;

/// v14 → v15：移除已经退役的外部画师词典缓存。画师前缀补全改为只使用
/// 资料库中已有的明确 `artist:` 标注作为证据。
pub(super) const MIGRATION_15: &str = r#"
DROP TABLE IF EXISTS artist_dictionary_sync;
DROP TABLE IF EXISTS artist_dictionary_names;
"#;

/// v15 → v16：缓存 VIBE 引用组合的稳定签名，供重复项视图“按 VIBE”聚合。
/// 历史行打开数据目录时从图片文件回填；别名表放开 'vibes' 聚合模式。
pub(super) const MIGRATION_16: &str = r#"
ALTER TABLE rows ADD COLUMN vibe_signature TEXT;
CREATE INDEX idx_rows_vibe_signature ON rows(vibe_signature)
WHERE vibe_signature IS NOT NULL;

CREATE TABLE dedupe_aliases_v16 (
    mode TEXT NOT NULL CHECK (mode IN ('artists', 'positivePrompt', 'vibes')),
    key TEXT NOT NULL,
    alias TEXT NOT NULL,
    PRIMARY KEY (mode, key)
) STRICT, WITHOUT ROWID;
INSERT INTO dedupe_aliases_v16 SELECT mode, key, alias FROM dedupe_aliases;
DROP TABLE dedupe_aliases;
ALTER TABLE dedupe_aliases_v16 RENAME TO dedupe_aliases;
"#;

/// v16 → v17：缓存正向提示词的画风签名（剥离官方质量词后的归一化哈希），
/// 供图片对比窗口的“相同画风”分区使用。计算只依赖库内 positive_prompt
/// 文本，历史行由启动回填补齐，不需要读图片文件。
pub(super) const MIGRATION_17: &str = r#"
ALTER TABLE rows ADD COLUMN style_signature TEXT;
CREATE INDEX idx_rows_style_signature ON rows(style_signature)
WHERE style_signature IS NOT NULL;
"#;
