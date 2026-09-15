//! Material covers and text live in SQLite together, so backup/migration is atomic.
use super::{Database, DatabaseError, TagSummary};
use rusqlite::{params, types::Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Material {
    pub id: i64,
    pub title: String,
    pub text: String,
    pub tags: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialDraft {
    pub id: Option<i64>,
    pub title: String,
    pub text: String,
    pub tags: Vec<String>,
    pub image_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MaterialPage {
    pub items: Vec<Material>,
    pub total: u64,
}

impl Database {
    pub fn material_ids_for_tag(&self, name: &str) -> Result<Vec<i64>, DatabaseError> {
        Ok(self.connection.prepare("SELECT mt.material_id FROM material_tags mt JOIN tags t ON t.id=mt.tag_id WHERE t.name=?1 ORDER BY mt.material_id")?
            .query_map([name], |r| r.get(0))?.collect::<Result<_, _>>()?)
    }

    /// Restore a deleted shared Tag without resurrecting deleted materials or replacing other Tags.
    pub fn restore_material_tag(&mut self, name: &str, ids: &[i64]) -> Result<(), DatabaseError> {
        let tx = self.connection.transaction()?;
        for id in ids {
            tx.execute("INSERT OR IGNORE INTO material_tags SELECT m.id,t.id FROM materials m,tags t WHERE m.id=?1 AND t.name=?2", params![id, name])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn material(&self, id: i64) -> Result<Material, DatabaseError> {
        let mut item = self.connection.query_row(
            "SELECT id, title, text, updated_at FROM materials WHERE id = ?1",
            [id],
            |r| {
                Ok(Material {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    text: r.get(2)?,
                    updated_at: r.get(3)?,
                    tags: vec![],
                })
            },
        )?;
        item.tags = self.connection.prepare(
            "SELECT t.name FROM tags t JOIN material_tags mt ON mt.tag_id=t.id WHERE mt.material_id=?1 ORDER BY t.name"
        )?.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
        Ok(item)
    }

    pub fn list_materials(
        &self,
        search: &str,
        tags: &[String],
        untagged: bool,
        offset: u32,
    ) -> Result<MaterialPage, DatabaseError> {
        let mut predicates = vec![
            "(instr(lower(m.title), lower(?)) > 0 OR instr(lower(m.text), lower(?)) > 0)"
                .to_owned(),
        ];
        let mut values = vec![
            Value::Text(search.trim().into()),
            Value::Text(search.trim().into()),
        ];
        for tag in super::tags::normalize_tags(tags) {
            predicates.push("EXISTS (SELECT 1 FROM material_tags mt JOIN tags t ON t.id=mt.tag_id WHERE mt.material_id=m.id AND t.name=?)".into());
            values.push(Value::Text(tag));
        }
        if untagged {
            predicates.push(
                "NOT EXISTS (SELECT 1 FROM material_tags mt WHERE mt.material_id=m.id)".into(),
            );
        }
        let predicate = predicates.join(" AND ");
        let total: i64 = self.connection.query_row(
            &format!("SELECT COUNT(*) FROM materials m WHERE {predicate}"),
            rusqlite::params_from_iter(&values),
            |r| r.get(0),
        )?;
        let total = u64::try_from(total).map_err(|_| DatabaseError::CountOverflow)?;
        values.push(Value::Integer(offset.into()));
        let ids = self.connection.prepare(&format!("SELECT m.id FROM materials m WHERE {predicate} ORDER BY m.updated_at DESC, m.id DESC LIMIT 48 OFFSET ?"))?
            .query_map(rusqlite::params_from_iter(&values), |r| r.get::<_, i64>(0))?.collect::<Result<Vec<_>, _>>()?;
        let items = ids
            .into_iter()
            .map(|id| self.material(id))
            .collect::<Result<_, _>>()?;
        Ok(MaterialPage { items, total })
    }

    pub fn material_tag_counts(&self) -> Result<Vec<TagSummary>, DatabaseError> {
        Ok(self.connection.prepare("SELECT t.name, COUNT(mt.material_id) FROM tags t LEFT JOIN material_tags mt ON mt.tag_id=t.id GROUP BY t.id ORDER BY t.name")?
            .query_map([], |r| Ok(TagSummary { name: r.get(0)?, row_count: r.get::<_, u32>(1)?.into() }))?.collect::<Result<_, _>>()?)
    }

    pub fn save_material(
        &mut self,
        draft: &MaterialDraft,
        images: Option<(&[u8], &[u8])>,
    ) -> Result<Material, DatabaseError> {
        let tx = self.connection.transaction()?;
        let title = draft.title.trim();
        let title = if title.is_empty() {
            "未命名素材"
        } else {
            title
        };
        let id = if let Some(id) = draft.id {
            let changed = tx.execute("UPDATE materials SET title=?1, text=?2, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id=?3", params![title, draft.text, id])?;
            if changed == 0 {
                return Err(DatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
            }
            if let Some((cover, thumbnail)) = images {
                tx.execute(
                    "UPDATE materials SET cover=?1, thumbnail=?2 WHERE id=?3",
                    params![cover, thumbnail, id],
                )?;
            }
            id
        } else {
            let (cover, thumbnail) =
                images.ok_or(DatabaseError::Sqlite(rusqlite::Error::InvalidQuery))?;
            tx.execute(
                "INSERT INTO materials(title,text,cover,thumbnail) VALUES (?1,?2,?3,?4)",
                params![title, draft.text, cover, thumbnail],
            )?;
            tx.last_insert_rowid()
        };
        tx.execute("DELETE FROM material_tags WHERE material_id=?1", [id])?;
        for name in super::tags::normalize_tags(&draft.tags) {
            tx.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [&name])?;
            tx.execute(
                "INSERT INTO material_tags SELECT ?1, id FROM tags WHERE name=?2",
                params![id, name],
            )?;
        }
        tx.commit()?;
        self.material(id)
    }

    pub fn delete_material(&mut self, id: i64) -> Result<(), DatabaseError> {
        self.connection
            .execute("DELETE FROM materials WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn material_image(&self, id: i64, thumbnail: bool) -> Result<Vec<u8>, DatabaseError> {
        let sql = if thumbnail {
            "SELECT thumbnail FROM materials WHERE id=?1"
        } else {
            "SELECT cover FROM materials WHERE id=?1"
        };
        Ok(self.connection.query_row(sql, [id], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn material_lifecycle_filters_tags_and_cover_are_independent_of_rows() {
        let mut db = Database::open_in_memory().unwrap();
        let mut draft = MaterialDraft {
            id: None,
            title: " 礼服 ".into(),
            text: "gold, black\n裙子".into(),
            tags: vec!["服设".into(), " 服设 ".into()],
            image_path: None,
        };
        let item = db
            .save_material(&draft, Some((b"cover", b"thumb")))
            .unwrap();
        assert_eq!(item.tags, ["服设"]);
        assert_eq!(
            db.list_materials("裙子", &["服设".into()], false, 0)
                .unwrap()
                .total,
            1
        );
        assert_eq!(db.list_materials("%", &[], false, 0).unwrap().total, 0);
        assert_eq!(db.list_materials("", &[], true, 0).unwrap().total, 0);
        assert_eq!(
            db.list_materials("", &[], false, 48).unwrap().items.len(),
            0
        );
        draft.id = Some(item.id);
        draft.text = "edited".into();
        db.save_material(&draft, None).unwrap();
        assert_eq!(db.material_image(item.id, false).unwrap(), b"cover");
        db.rename_tag("服设", "衣服").unwrap();
        assert_eq!(db.material(item.id).unwrap().tags, ["衣服"]);
        db.delete_tag("衣服").unwrap();
        assert_eq!(db.list_materials("edited", &[], true, 0).unwrap().total, 1);
        db.create_tag("衣服").unwrap();
        db.restore_material_tag("衣服", &[item.id, 99999]).unwrap();
        assert_eq!(db.material_ids_for_tag("衣服").unwrap(), [item.id]);
        db.delete_material(item.id).unwrap();
        assert_eq!(db.list_materials("", &[], false, 0).unwrap().total, 0);
        assert!(db.save_material(&draft, None).is_err());
    }

    #[test]
    fn materials_upgrade_from_v18_and_backup_preserves_cover_text_and_tags() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(super::super::migrations::SCHEMA_17)
            .unwrap();
        connection
            .execute_batch(super::super::migrations::MIGRATION_18)
            .unwrap();
        connection.pragma_update(None, "user_version", 18).unwrap();
        let mut db = Database::initialize(connection).unwrap();
        assert_eq!(db.schema_version().unwrap(), 19);
        let draft = MaterialDraft {
            id: None,
            title: "素材".into(),
            text: " exact text\n".into(),
            tags: vec!["服设".into()],
            image_path: None,
        };
        let saved = db
            .save_material(&draft, Some((b"cover", b"thumbnail")))
            .unwrap();
        let mut destination = rusqlite::Connection::open_in_memory().unwrap();
        {
            let backup = rusqlite::backup::Backup::new(&db.connection, &mut destination).unwrap();
            backup
                .run_to_completion(10, std::time::Duration::from_millis(1), None)
                .unwrap();
        }
        drop(db);
        let reopened = Database::initialize(destination).unwrap();
        assert_eq!(reopened.material(saved.id).unwrap().text, " exact text\n");
        assert_eq!(reopened.material(saved.id).unwrap().tags, ["服设"]);
        assert_eq!(reopened.material_image(saved.id, false).unwrap(), b"cover");
        assert_eq!(
            reopened.material_image(saved.id, true).unwrap(),
            b"thumbnail"
        );
    }
}
