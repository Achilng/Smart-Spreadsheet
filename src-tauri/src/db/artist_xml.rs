use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::DatabaseError;
use crate::pipeline::{artist_string, extract_artist_blocks};

const KEY: &str = "artist_xml_extraction_version";

/// Repair only XML-tagged historical prompts, including incorrect nonempty values.
/// Preserve prompts and unrelated artist strings; commit the version with the repair.
pub(super) fn repair_xml_artist_strings(connection: &mut Connection) -> Result<(), DatabaseError> {
    let version: Option<String> = connection
        .query_row("SELECT value FROM settings WHERE key = ?1", [KEY], |row| {
            row.get(0)
        })
        .optional()?;
    if version.as_deref() == Some("1") {
        return Ok(());
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    {
        let mut select = transaction.prepare(
            "SELECT id, positive_prompt, character_prompt FROM rows
             WHERE id > ?1 AND (positive_prompt LIKE '%<artist%' OR character_prompt LIKE '%<artist%')
             ORDER BY id LIMIT 256",
        )?;
        let mut update = transaction.prepare("UPDATE rows SET artists = ?2 WHERE id = ?1")?;
        let mut last_id = 0;
        loop {
            let batch = select
                .query_map([last_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            if batch.is_empty() {
                break;
            }
            for (id, positive, character) in batch {
                last_id = id;
                let positive = positive.as_deref().unwrap_or_default();
                if extract_artist_blocks(positive).is_some()
                    || extract_artist_blocks(character.as_deref().unwrap_or_default()).is_some()
                {
                    update.execute(params![id, artist_string(positive, character.as_deref())])?;
                }
            }
        }
    }
    transaction.execute(
        "INSERT INTO settings(key, value) VALUES (?1, '1')
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [KEY],
    )?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, NewRow, test_support::append_rows};

    #[test]
    fn repairs_nonempty_xml_artists_across_batches_without_changing_prompts_or_legacy() {
        let mut db = Database::open_in_memory().unwrap();
        let prompt = "<artist>0.8::a, b::, a</artist> <style>year_2025</style> girl";
        let rows: Vec<_> = (0..260)
            .map(|i| NewRow {
                identity: format!("xml-{i}"),
                source_ordinal: i + 1,
                positive_prompt: Some(prompt.into()),
                artists: Some("incorrect old value".into()),
                ..NewRow::default()
            })
            .chain([NewRow {
                identity: "legacy".into(),
                source_ordinal: 261,
                positive_prompt: Some("artist:legacy".into()),
                artists: Some("keep custom value".into()),
                ..NewRow::default()
            }])
            .collect();
        append_rows(&mut db, &rows);
        db.connection
            .execute("DELETE FROM settings WHERE key = ?1", [KEY])
            .unwrap();
        // Include character-only and explicitly empty blocks in the same migration.
        db.connection.execute_batch("UPDATE rows SET positive_prompt = 'girl', character_prompt = '<ARTIST>c</ARTIST>' WHERE id = 2;
            UPDATE rows SET positive_prompt = '<artist> </artist> girl' WHERE id = 3;").unwrap();
        repair_xml_artist_strings(&mut db.connection).unwrap();
        let count: i64 = db.connection.query_row(
            "SELECT COUNT(*) FROM rows WHERE artists = '0.8::a, b::, a' AND positive_prompt = ?1", [prompt], |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 258);
        let artist = |id| {
            db.connection
                .query_row("SELECT artists FROM rows WHERE id = ?1", [id], |row| {
                    row.get::<_, Option<String>>(0)
                })
                .unwrap()
        };
        assert_eq!(artist(2).as_deref(), Some("c"));
        assert_eq!(artist(3), None);
        assert_eq!(artist(261).as_deref(), Some("keep custom value"));
        let changes = db.connection.total_changes();
        repair_xml_artist_strings(&mut db.connection).unwrap();
        assert_eq!(db.connection.total_changes(), changes);
    }
}
