use std::collections::HashMap;

use rusqlite::{TransactionBehavior, params};

use super::{Database, DatabaseError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingImageTarget {
    pub row_id: i64,
    pub identity: String,
    pub stored_image_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingImageUpdate {
    pub row_id: i64,
    pub identity: String,
    pub image_path: String,
    pub source_size: Option<i64>,
    pub source_mtime: Option<i64>,
    pub positive_prompt: Option<String>,
    pub character_prompt: Option<String>,
    pub negative_prompt: Option<String>,
    pub artists: Option<String>,
    pub content_hash: Option<String>,
    pub perceptual_hash: Option<String>,
    pub metadata_fingerprint: Option<String>,
    pub stored_image_path: Option<String>,
    pub stored_image_is_original: bool,
}

impl Database {
    pub fn existing_image_targets(
        &mut self,
        identities: &[String],
    ) -> Result<HashMap<String, ExistingImageTarget>, DatabaseError> {
        const CANDIDATES_TABLE: &str = "temp.image_update_candidates";
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(&format!(
            "DROP TABLE IF EXISTS {CANDIDATES_TABLE};
             CREATE TEMP TABLE {CANDIDATES_TABLE} (
                 identity TEXT PRIMARY KEY
             ) STRICT, WITHOUT ROWID;"
        ))?;
        {
            let mut insert = transaction.prepare(&format!(
                "INSERT OR IGNORE INTO {CANDIDATES_TABLE}(identity) VALUES (?1)"
            ))?;
            for identity in identities {
                insert.execute([identity])?;
            }
        }
        let targets = {
            let mut statement = transaction.prepare(&format!(
                "SELECT rows.id, rows.identity, rows.stored_image_path
                 FROM rows
                 JOIN {CANDIDATES_TABLE} AS candidates
                   ON candidates.identity = rows.identity"
            ))?;
            statement
                .query_map([], |row| {
                    let target = ExistingImageTarget {
                        row_id: row.get(0)?,
                        identity: row.get(1)?,
                        stored_image_path: row.get(2)?,
                    };
                    Ok((target.identity.clone(), target))
                })?
                .collect::<Result<HashMap<_, _>, _>>()?
        };
        transaction.execute_batch(&format!("DROP TABLE {CANDIDATES_TABLE};"))?;
        transaction.commit()?;
        Ok(targets)
    }

    pub fn existing_image_targets_by_content_hash(
        &mut self,
        hashes: &[String],
    ) -> Result<HashMap<String, Vec<ExistingImageTarget>>, DatabaseError> {
        self.existing_image_targets_by_value(
            hashes,
            "temp.image_update_hash_candidates",
            "content_hash",
        )
    }

    pub fn existing_image_targets_by_metadata_fingerprint(
        &mut self,
        fingerprints: &[String],
    ) -> Result<HashMap<String, Vec<ExistingImageTarget>>, DatabaseError> {
        self.existing_image_targets_by_value(
            fingerprints,
            "temp.image_update_metadata_candidates",
            "metadata_fingerprint",
        )
    }

    fn existing_image_targets_by_value(
        &mut self,
        values: &[String],
        candidates_table: &str,
        column: &str,
    ) -> Result<HashMap<String, Vec<ExistingImageTarget>>, DatabaseError> {
        debug_assert!(matches!(column, "content_hash" | "metadata_fingerprint"));
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(&format!(
            "DROP TABLE IF EXISTS {candidates_table};
             CREATE TEMP TABLE {candidates_table} (
                 value TEXT PRIMARY KEY
             ) STRICT, WITHOUT ROWID;"
        ))?;
        {
            let mut insert = transaction.prepare(&format!(
                "INSERT OR IGNORE INTO {candidates_table}(value) VALUES (?1)"
            ))?;
            for value in values {
                insert.execute([value])?;
            }
        }
        let pairs = {
            let mut statement = transaction.prepare(&format!(
                "SELECT rows.{column}, rows.id, rows.identity, rows.stored_image_path
                 FROM rows
                 JOIN {candidates_table} AS candidates
                   ON candidates.value = rows.{column}
                 ORDER BY rows.id"
            ))?;
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        ExistingImageTarget {
                            row_id: row.get(1)?,
                            identity: row.get(2)?,
                            stored_image_path: row.get(3)?,
                        },
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        transaction.execute_batch(&format!("DROP TABLE {candidates_table};"))?;
        transaction.commit()?;

        let mut targets: HashMap<String, Vec<ExistingImageTarget>> = HashMap::new();
        for (value, target) in pairs {
            targets.entry(value).or_default().push(target);
        }
        Ok(targets)
    }

    pub fn update_existing_images(
        &mut self,
        updates: &[ExistingImageUpdate],
    ) -> Result<u64, DatabaseError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut updated = 0_u64;
        {
            let mut statement = transaction.prepare(
                "UPDATE rows SET
                    identity = ?2,
                    image_path = ?3,
                    source_size = ?4,
                    source_mtime = ?5,
                    positive_prompt = ?6,
                    character_prompt = ?7,
                    negative_prompt = ?8,
                    artists = ?9,
                    metadata_failed = 0,
                    content_hash = ?10,
                    perceptual_hash = ?11,
                    metadata_fingerprint = ?12,
                    stored_image_path = ?13,
                    stored_image_is_original = ?14
                 WHERE id = ?1",
            )?;
            for update in updates {
                updated += statement.execute(params![
                    update.row_id,
                    update.identity,
                    update.image_path,
                    update.source_size,
                    update.source_mtime,
                    update.positive_prompt,
                    update.character_prompt,
                    update.negative_prompt,
                    update.artists,
                    update.content_hash,
                    update.perceptual_hash,
                    update.metadata_fingerprint,
                    update.stored_image_path,
                    update.stored_image_is_original,
                ])? as u64;
            }
        }
        transaction.commit()?;
        self.bump_data_version();
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{NewRow, RowSelection, SourceType};

    #[test]
    fn updates_metadata_in_place_and_preserves_organization() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .append_batch(
                SourceType::Folder,
                r"D:\images",
                &[NewRow {
                    source_ordinal: 1,
                    identity: r"file:d:\images\one.png".into(),
                    positive_prompt: Some("old prompt".into()),
                    ..NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();
        database.create_tag("保留标签").unwrap();
        database.set_tags_for_row(1, &["保留标签".into()]).unwrap();
        database.update_note(1, "保留备注").unwrap();
        let group = database.create_group("保留分组").unwrap();
        database
            .assign_rows_to_group(&RowSelection::Explicit { row_ids: vec![1] }, group.id)
            .unwrap();

        let updated = database
            .update_existing_images(&[ExistingImageUpdate {
                row_id: 1,
                identity: r"file:d:\moved\one.png".into(),
                image_path: r"D:\moved\one.png".into(),
                source_size: Some(100),
                source_mtime: Some(200),
                positive_prompt: Some("new prompt".into()),
                character_prompt: Some("new character".into()),
                negative_prompt: Some("new negative".into()),
                artists: Some("artist:new".into()),
                content_hash: Some("content".into()),
                perceptual_hash: Some("perceptual".into()),
                metadata_fingerprint: Some("metadata".into()),
                stored_image_path: None,
                stored_image_is_original: true,
            }])
            .unwrap();

        assert_eq!(updated, 1);
        let rows = database.get_rows_by_ids(&[1]).unwrap();
        assert_eq!(rows[0].positive_prompt.as_deref(), Some("new prompt"));
        assert_eq!(rows[0].character_prompt.as_deref(), Some("new character"));
        assert_eq!(rows[0].negative_prompt.as_deref(), Some("new negative"));
        assert_eq!(rows[0].artists.as_deref(), Some("artist:new"));
        assert_eq!(rows[0].note.as_deref(), Some("保留备注"));
        assert_eq!(rows[0].tags, vec!["保留标签"]);
        assert_eq!(rows[0].group_id, Some(group.id));
        assert_eq!(rows[0].group_name.as_deref(), Some("保留分组"));
    }
}
