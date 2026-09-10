//! Connection-local, lazily populated search text. No schema migration or image reads.
use rusqlite::{Connection, params};

pub(super) const SEARCH_COLUMNS: [&str; 6] = [
    "image_path",
    "positive_prompt",
    "character_prompt",
    "negative_prompt",
    "note",
    "artists",
];

// Keep query normalization compatible with the existing separator-insensitive search.
#[cfg(test)]
pub(super) const SEARCH_SEPARATORS: &str = " \t\n\r\u{000B}\u{000C}\u{0085}\u{00A0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000},，_-";

pub(super) fn is_search_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, ',' | '，' | '_' | '-')
}

pub(super) fn normalize_search(search: &str) -> String {
    search
        .to_lowercase()
        .chars()
        .filter(|c| !is_search_separator(*c))
        .collect()
}

fn normalize_column(text: &str) -> String {
    // SQLite LOWER historically folds ASCII only. Preserve its matching semantics.
    text.chars()
        .filter(|c| !is_search_separator(*c))
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub(super) fn prepare_search_text(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS query_search_text (
            id INTEGER PRIMARY KEY,
            image_path TEXT NOT NULL, positive_prompt TEXT NOT NULL,
            character_prompt TEXT NOT NULL, negative_prompt TEXT NOT NULL,
            note TEXT NOT NULL, artists TEXT NOT NULL
         ) STRICT;
         CREATE TEMP TABLE IF NOT EXISTS query_search_version (version INTEGER NOT NULL) STRICT;
         CREATE TEMP TRIGGER IF NOT EXISTS query_search_update
         AFTER UPDATE OF id, image_path, positive_prompt, character_prompt, negative_prompt, note, artists ON main.rows
         WHEN OLD.id IS NOT NEW.id OR OLD.image_path IS NOT NEW.image_path
           OR OLD.positive_prompt IS NOT NEW.positive_prompt
           OR OLD.character_prompt IS NOT NEW.character_prompt
           OR OLD.negative_prompt IS NOT NEW.negative_prompt
           OR OLD.note IS NOT NEW.note OR OLD.artists IS NOT NEW.artists
         BEGIN DELETE FROM query_search_text WHERE id IN (OLD.id, NEW.id); END;
         CREATE TEMP TRIGGER IF NOT EXISTS query_search_delete AFTER DELETE ON main.rows
         BEGIN DELETE FROM query_search_text WHERE id = OLD.id; END;
         CREATE TEMP TRIGGER IF NOT EXISTS query_search_insert AFTER INSERT ON main.rows
         BEGIN DELETE FROM query_search_text WHERE id = NEW.id; END;",
    )?;
    // TEMP triggers see this connection's writes only. Another connection (imports,
    // toolbox edits, etc.) invalidates the cache via SQLite's data_version instead.
    let version: i64 = connection.pragma_query_value(None, "data_version", |row| row.get(0))?;
    let current: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM temp.query_search_version WHERE version = ?1)",
        [version],
        |row| row.get(0),
    )?;
    if !current {
        connection.execute_batch(
            "DELETE FROM temp.query_search_text; DELETE FROM temp.query_search_version;",
        )?;
        connection.execute(
            "INSERT INTO temp.query_search_version VALUES (?1)",
            [version],
        )?;
    }
    let mut select = connection.prepare(
        "SELECT id, image_path, positive_prompt, character_prompt, negative_prompt, note, artists
         FROM main.rows WHERE id NOT IN (SELECT id FROM temp.query_search_text)",
    )?;
    let mut insert = connection
        .prepare("INSERT INTO temp.query_search_text VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?;
    let mut rows = select.query([])?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let text = (1..=6)
            .map(|index| {
                let text = row.get_ref(index)?;
                Ok(normalize_column(text.as_str_or_null()?.unwrap_or_default()))
            })
            .collect::<rusqlite::Result<Vec<_>>>()?;
        insert.execute(params![
            id, text[0], text[1], text[2], text[3], text[4], text[5]
        ])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(connection: &Connection) {
        connection.execute_batch(
            "CREATE TABLE rows (id INTEGER PRIMARY KEY, image_path TEXT, positive_prompt TEXT,
              character_prompt TEXT, negative_prompt TEXT, note TEXT, artists TEXT, group_id INTEGER);
             INSERT INTO rows(id, note) VALUES (1, 'Alpha Beta'), (2, 'UNCHANGED');",
        ).unwrap();
    }

    fn cached_note(connection: &Connection, id: i64) -> String {
        connection
            .query_row(
                "SELECT note FROM temp.query_search_text WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn search_cache_reuses_text_and_refreshes_only_changed_rows() {
        let connection = Connection::open_in_memory().unwrap();
        seed(&connection);
        prepare_search_text(&connection).unwrap();
        let writes = connection.total_changes();
        prepare_search_text(&connection).unwrap();
        assert_eq!(
            connection.total_changes(),
            writes,
            "unchanged search must not rebuild text"
        );
        connection
            .execute("UPDATE rows SET group_id = 4, note = note WHERE id = 1", [])
            .unwrap();
        assert_eq!(cached_note(&connection, 1), "alphabeta");
        connection
            .execute("UPDATE rows SET note = 'Changed' WHERE id = 1", [])
            .unwrap();
        assert_eq!(cached_note(&connection, 2), "unchanged");
        let writes = connection.total_changes();
        prepare_search_text(&connection).unwrap();
        assert_eq!(
            connection.total_changes() - writes,
            1,
            "only edited text is rebuilt"
        );
        assert_eq!(cached_note(&connection, 1), "changed");
        connection.execute_batch("DELETE FROM rows WHERE id = 1; INSERT INTO rows(id, note) VALUES (1, 'Reused'), (3, 'New');").unwrap();
        prepare_search_text(&connection).unwrap();
        assert_eq!(cached_note(&connection, 1), "reused");
        assert_eq!(cached_note(&connection, 3), "new");
        connection
            .execute("DELETE FROM rows WHERE id = 3", [])
            .unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM temp.query_search_text", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn search_cache_invalidation_rolls_back_with_failed_edit() {
        let mut connection = Connection::open_in_memory().unwrap();
        seed(&connection);
        prepare_search_text(&connection).unwrap();
        let tx = connection.transaction().unwrap();
        tx.execute("UPDATE rows SET note = 'Temporary' WHERE id = 1", [])
            .unwrap();
        prepare_search_text(&tx).unwrap();
        assert_eq!(cached_note(&tx, 1), "temporary");
        tx.rollback().unwrap();
        assert_eq!(cached_note(&connection, 1), "alphabeta");
    }

    #[test]
    fn search_cache_observes_other_connections() {
        let uri = format!(
            "file:search-cache-external-{}?mode=memory&cache=shared",
            std::process::id()
        );
        let connection = Connection::open(&uri).unwrap();
        seed(&connection);
        prepare_search_text(&connection).unwrap();
        let other = Connection::open(&uri).unwrap();
        other
            .execute_batch(
                "UPDATE rows SET note = 'External' WHERE id = 1;
            DELETE FROM rows WHERE id = 2; INSERT INTO rows(id, note) VALUES (3, 'Imported');",
            )
            .unwrap();
        prepare_search_text(&connection).unwrap();
        assert_eq!(cached_note(&connection, 1), "external");
        assert_eq!(cached_note(&connection, 3), "imported");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM temp.query_search_text", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn column_normalization_matches_legacy_sql() {
        let connection = Connection::open_in_memory().unwrap();
        let sql = SEARCH_SEPARATORS
            .chars()
            .fold("LOWER(COALESCE(?1, ''))".to_owned(), |sql, c| {
                format!("REPLACE({sql}, CHAR({}), '')", c as u32)
            });
        for text in [
            format!("ALPHA{SEARCH_SEPARATORS}Beta"),
            "中文 ÉÖİ ABC %'\\\0".into(),
        ] {
            let legacy: String = connection
                .query_row(&format!("SELECT {sql}"), [&text], |r| r.get(0))
                .unwrap();
            assert_eq!(normalize_column(&text), legacy);
        }
    }
}
