//! Material covers and text live in SQLite together, so backup/migration is atomic.
use super::{Database, DatabaseError, TagSummary};
use rusqlite::{OptionalExtension, params, types::Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Material {
    pub id: i64,
    pub title: String,
    pub text: String,
    pub tags: Vec<String>,
    pub updated_at: String,
    pub versions: Vec<MaterialVersion>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialVersion {
    pub id: i64,
    pub name: String,
    pub text: String,
    pub has_image: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialVersionDraft {
    pub id: Option<i64>,
    pub name: String,
    pub text: String,
    pub image_path: Option<String>,
    pub image_source_id: Option<i64>,
}

pub type VersionImages = Vec<Option<(Vec<u8>, Vec<u8>)>>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialDraft {
    pub id: Option<i64>,
    pub title: String,
    pub text: String,
    pub tags: Vec<String>,
    pub image_path: Option<String>,
    pub versions: Option<Vec<MaterialVersionDraft>>,
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
                    versions: vec![],
                })
            },
        )?;
        item.tags = self.connection.prepare(
            "SELECT t.name FROM tags t JOIN material_tags mt ON mt.tag_id=t.id WHERE mt.material_id=?1 ORDER BY t.name"
        )?.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
        item.versions = self.connection.prepare("SELECT id,name,text,cover IS NOT NULL FROM material_versions WHERE material_id=?1 ORDER BY position,id")?
            .query_map([id], |r| Ok(MaterialVersion { id:r.get(0)?, name:r.get(1)?, text:r.get(2)?, has_image:r.get(3)? }))?
            .collect::<Result<_,_>>()?;
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
            "(instr(lower(m.title), lower(?1)) > 0 OR EXISTS (SELECT 1 FROM material_versions v WHERE v.material_id=m.id AND (instr(lower(v.name), lower(?1)) > 0 OR instr(lower(v.text), lower(?1)) > 0)))"
                .to_owned(),
        ];
        let mut values = vec![Value::Text(search.trim().into())];
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
        self.save_material_versions(draft, images, &[])
    }

    pub fn save_material_versions(
        &mut self,
        draft: &MaterialDraft,
        images: Option<(&[u8], &[u8])>,
        version_images: &[Option<(Vec<u8>, Vec<u8>)>],
    ) -> Result<Material, DatabaseError> {
        if let Some(versions) = &draft.versions {
            if versions.is_empty()
                || versions.len() > 128
                || (!version_images.is_empty() && version_images.len() != versions.len())
            {
                return Err(DatabaseError::Sqlite(rusqlite::Error::InvalidQuery));
            }
        }
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
        if let Some(versions) = &draft.versions {
            let mut kept = std::collections::HashSet::new();
            // Resolve all image sources before updating or deleting any version.
            let mut resolved = Vec::with_capacity(versions.len());
            for (index, version) in versions.iter().enumerate() {
                if let Some(version_id) = version.id {
                    let owner: Option<i64> = tx
                        .query_row(
                            "SELECT material_id FROM material_versions WHERE id=?1",
                            [version_id],
                            |r| r.get(0),
                        )
                        .optional()?;
                    if owner != Some(id) || !kept.insert(version_id) {
                        return Err(DatabaseError::Sqlite(rusqlite::Error::InvalidQuery));
                    }
                }
                let image = if let Some(Some(image)) = version_images.get(index) {
                    Some(image.clone())
                } else if let Some(source_id) = version.image_source_id {
                    tx.query_row("SELECT cover,thumbnail FROM material_versions WHERE id=?1 AND material_id=?2", params![source_id,id], |r| {
                        let cover: Option<Vec<u8>> = r.get(0)?;
                        let thumb: Option<Vec<u8>> = r.get(1)?;
                        Ok(cover.zip(thumb))
                    })?
                } else {
                    None
                };
                resolved.push(image);
            }
            for (position, (version, image)) in versions.iter().zip(resolved).enumerate() {
                let name = if version.name.trim().is_empty() {
                    format!("版本 {}", position + 1)
                } else {
                    version.name.trim().to_owned()
                };
                let (cover, thumb) = image.map_or((None, None), |(a, b)| (Some(a), Some(b)));
                let position = position as i64;
                if let Some(version_id) = version.id {
                    tx.execute("UPDATE material_versions SET name=?1,text=?2,position=?3,cover=?4,thumbnail=?5 WHERE id=?6", params![name,version.text,position,cover,thumb,version_id])?;
                } else {
                    tx.execute("INSERT INTO material_versions(material_id,name,text,position,cover,thumbnail) VALUES (?1,?2,?3,?4,?5,?6)", params![id,name,version.text,position,cover,thumb])?;
                    kept.insert(tx.last_insert_rowid());
                }
            }
            let existing = tx
                .prepare("SELECT id FROM material_versions WHERE material_id=?1")?
                .query_map([id], |r| r.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for version_id in existing {
                if !kept.contains(&version_id) {
                    tx.execute("DELETE FROM material_versions WHERE id=?1", [version_id])?;
                }
            }
            tx.execute(
                "UPDATE materials SET text=?1 WHERE id=?2",
                params![versions[0].text, id],
            )?;
        } else {
            // Older callers edit the default text without destroying other versions.
            let changed = tx.execute("UPDATE material_versions SET text=?1 WHERE id=(SELECT id FROM material_versions WHERE material_id=?2 ORDER BY position,id LIMIT 1)", params![draft.text,id])?;
            if changed == 0 {
                tx.execute("INSERT INTO material_versions(material_id,name,text,position) VALUES (?1,'默认版本',?2,0)", params![id,draft.text])?;
            }
        }
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

    pub fn material_version_image(&self, id: i64) -> Result<Vec<u8>, DatabaseError> {
        Ok(self.connection.query_row("SELECT coalesce(v.cover,m.cover) FROM material_versions v JOIN materials m ON m.id=v.material_id WHERE v.id=?1", [id], |r| r.get(0))?)
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

    fn draft() -> MaterialDraft {
        MaterialDraft { id: None, title: "花绘".into(), text: "legacy".into(), tags: vec!["OC".into()], image_path: None,
            versions: Some(vec![version("无服设", "base"), version("原服设", "dress prompt")]) }
    }

    fn version(name: &str, text: &str) -> MaterialVersionDraft {
        MaterialVersionDraft { id: None, name: name.into(), text: text.into(), image_path: None, image_source_id: None }
    }

    #[test]
    fn versions_reorder_search_images_and_backup_stay_together() {
        let mut db = Database::open_in_memory().unwrap();
        let mut draft = draft();
        let saved = db.save_material_versions(&draft, Some((b"fixed", b"thumb")), &[None, Some((b"dress".to_vec(), b"small".to_vec()))]).unwrap();
        assert_eq!(saved.text, "base");
        assert_eq!(saved.versions.len(), 2);
        assert_eq!(saved.tags, ["OC"]);
        assert!(!saved.versions[0].has_image);
        assert!(saved.versions[1].has_image);
        assert_eq!(db.material_version_image(saved.versions[0].id).unwrap(), b"fixed");
        assert_eq!(db.material_version_image(saved.versions[1].id).unwrap(), b"dress");
        assert_eq!(db.list_materials("原服设", &["OC".into()], false, 0).unwrap().total, 1);
        assert_eq!(db.list_materials("DRESS", &[], false, 0).unwrap().total, 1);
        draft.id = Some(saved.id);
        let versions = draft.versions.as_mut().unwrap();
        for (item, saved) in versions.iter_mut().zip(&saved.versions) {
            item.id = Some(saved.id);
            item.image_source_id = saved.has_image.then_some(saved.id);
        }
        versions.swap(0, 1);
        let reordered = db.save_material(&draft, None).unwrap();
        assert_eq!(reordered.text, "dress prompt");
        assert_eq!(reordered.versions[0].id, saved.versions[1].id);
        assert_eq!(db.material_image(saved.id, false).unwrap(), b"fixed");
        // Duplicate an image while deleting its original source in the same transaction.
        let mut duplicate = draft.versions.as_ref().unwrap()[0].clone();
        duplicate.id = None;
        duplicate.name = "JK 制服".into();
        draft.versions = Some(vec![duplicate, draft.versions.as_ref().unwrap()[1].clone()]);
        let duplicated = db.save_material(&draft, None).unwrap();
        assert!(db.material_version_image(saved.versions[1].id).is_err());
        assert_eq!(db.material_version_image(duplicated.versions[0].id).unwrap(), b"dress");
        let mut destination = rusqlite::Connection::open_in_memory().unwrap();
        {
            let backup = rusqlite::backup::Backup::new(&db.connection, &mut destination).unwrap();
            backup.run_to_completion(10, std::time::Duration::from_millis(1), None).unwrap();
        }
        let mut reopened = Database::initialize(destination).unwrap();
        let item = reopened.material(saved.id).unwrap();
        assert_eq!(item.versions.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["JK 制服", "无服设"]);
        assert_eq!(item.text, "dress prompt");
        assert_eq!(reopened.material_version_image(item.versions[0].id).unwrap(), b"dress");
        assert_eq!(reopened.material_image(saved.id, false).unwrap(), b"fixed");
        assert_eq!(item.tags, ["OC"]);
        reopened.delete_material(saved.id).unwrap();
        assert!(reopened.material_version_image(item.versions[0].id).is_err());
        assert_eq!(reopened.connection.query_row("SELECT count(*) FROM material_versions", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
    }

    #[test]
    fn invalid_versions_roll_back_parent_images_text_and_tags() {
        let mut db = Database::open_in_memory().unwrap();
        let mut draft = draft();
        let first = db.save_material(&draft, Some((b"cover", b"thumb"))).unwrap();
        let other = db.save_material(&draft, Some((b"other", b"thumb"))).unwrap();
        draft.id = Some(first.id); draft.title = "changed".into(); draft.text = "changed".into(); draft.tags = vec!["new".into()];
        let mut foreign = version("foreign", "bad"); foreign.id = Some(other.versions[0].id);
        let mut foreign_image = version("image", "bad"); foreign_image.image_source_id = Some(other.versions[0].id);
        let mut repeated = version("repeat", "bad"); repeated.id = Some(first.versions[0].id);
        for invalid in [vec![], vec![foreign], vec![foreign_image], vec![repeated.clone(), repeated], vec![version("many", ""); 129]] {
            draft.versions = Some(invalid);
            assert!(db.save_material(&draft, Some((b"changed", b"changed"))).is_err());
            let retained = db.material(first.id).unwrap();
            assert_eq!(retained.title, "花绘"); assert_eq!(retained.text, "base"); assert_eq!(retained.tags, ["OC"]);
            assert_eq!(retained.versions.len(), 2);
            assert_eq!(db.material_image(first.id, false).unwrap(), b"cover");
        }
    }

    #[test]
    fn upgrading_v19_preserves_existing_material_as_default_version() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.execute_batch(super::super::migrations::SCHEMA_17).unwrap();
        connection.execute_batch(super::super::migrations::MIGRATION_18).unwrap();
        connection.execute_batch(super::super::migrations::MIGRATION_19).unwrap();
        connection.execute("INSERT INTO materials(id,title,text,cover,thumbnail) VALUES (7,'旧素材',?1,?2,?3)", params!["  old\ntext  ", b"cover", b"thumb"]).unwrap();
        connection.execute_batch("INSERT INTO tags(id,name) VALUES (1,'OC'); INSERT INTO material_tags VALUES (7,1); PRAGMA user_version=19;").unwrap();
        let db = Database::initialize(connection).unwrap();
        let item = db.material(7).unwrap();
        assert_eq!(db.schema_version().unwrap(), super::super::CURRENT_SCHEMA_VERSION);
        assert_eq!(item.versions.len(), 1);
        assert_eq!(item.versions[0].name, "默认版本");
        assert_eq!(item.versions[0].text, "  old\ntext  ");
        assert_eq!(item.tags, ["OC"]);
        assert_eq!(db.material_version_image(item.versions[0].id).unwrap(), b"cover");
        let reopened = Database::initialize(db.connection).unwrap();
        assert_eq!(reopened.material(7).unwrap().versions.len(), 1);
    }
    #[test]
    fn material_lifecycle_filters_tags_and_cover_are_independent_of_rows() {
        let mut db = Database::open_in_memory().unwrap();
        let mut draft = MaterialDraft {
            id: None,
            title: " 礼服 ".into(),
            text: "gold, black\n裙子".into(),
            tags: vec!["服设".into(), " 服设 ".into()],
            image_path: None,
            versions: None,
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
        assert_eq!(db.schema_version().unwrap(), super::super::CURRENT_SCHEMA_VERSION);
        let draft = MaterialDraft {
            id: None,
            title: "素材".into(),
            text: " exact text\n".into(),
            tags: vec!["服设".into()],
            image_path: None,
            versions: None,
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
