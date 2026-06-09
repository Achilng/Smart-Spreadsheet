pub const CURRENT_SCHEMA_VERSION: u32 = 1;

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
