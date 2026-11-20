//! Library-wide changes (groups, TAGs, artist-string aliases and representatives):
//! planned like row edits, applied in one transaction, and undone from what was recorded.
use std::collections::{BTreeSet, HashMap};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::changes::{GroupChange, RowChange};
use super::{Res, s};
use crate::db::Database;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GroupRef {
    Id(i64),
    Name(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LibraryOp {
    RenameGroup { group: GroupRef, new_name: String },
    MergeGroups { groups: Vec<GroupRef>, into: GroupRef },
    DeleteGroups { groups: Vec<GroupRef> },
    DeleteEmptyGroups,
    RenameTag { tag: String, new_name: String },
    MergeTags { tags: Vec<String>, into: String },
    DeleteTags { tags: Vec<String> },
    SetArtistAlias { artist_string: String, alias: String },
    SetArtistRepresentative {
        row_id: i64,
        #[serde(default = "enabled")]
        enabled: bool,
    },
}

fn enabled() -> bool {
    true
}

pub(crate) struct LibraryPlan {
    pub(crate) summary: Vec<String>,
    /// Effect on each image; also what undo restores for group and TAG operations.
    pub(crate) changes: Vec<RowChange>,
    pub(crate) warnings: Vec<String>,
    pub(crate) new_groups: Vec<String>,
    pub(crate) new_tags: Vec<String>,
    /// The artist string's dedupe key, for alias changes.
    pub(crate) artist_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredGroup {
    id: i64,
    name: String,
    created_at: String,
}

fn quote_list(names: &[String]) -> String {
    let shown = names.iter().take(8).map(|name| format!("「{name}」")).collect::<String>();
    if names.len() > 8 { format!("{shown} 等 {} 个", names.len()) } else { shown }
}

fn empty_change(id: i64) -> RowChange {
    RowChange { id, text: Vec::new(), group: None, favorite: None, tags_added: Vec::new(), tags_removed: Vec::new() }
}

fn ids(connection: &Connection, sql: &str, value: impl rusqlite::ToSql) -> Res<Vec<i64>> {
    let mut statement = connection.prepare(sql).map_err(s)?;
    let rows = statement.query_map([value], |row| row.get(0)).map_err(s)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(s)
}

fn group_rows(connection: &Connection, group: i64) -> Res<Vec<i64>> {
    ids(connection, "SELECT id FROM rows WHERE group_id = ?1 ORDER BY id", group)
}

fn tag_rows(connection: &Connection, tag: &str) -> Res<Vec<i64>> {
    ids(connection, "SELECT row_tags.row_id FROM row_tags JOIN tags ON tags.id = row_tags.tag_id
        WHERE tags.name = ?1 COLLATE BINARY ORDER BY row_tags.row_id", tag)
}

fn tag_materials(connection: &Connection, tag: &str) -> Res<Vec<i64>> {
    ids(connection, "SELECT material_tags.material_id FROM material_tags JOIN tags ON tags.id = material_tags.tag_id
        WHERE tags.name = ?1 COLLATE BINARY ORDER BY material_tags.material_id", tag)
}

fn tag_exists(connection: &Connection, tag: &str) -> Res<bool> {
    connection
        .query_row("SELECT EXISTS(SELECT 1 FROM tags WHERE name = ?1 COLLATE BINARY)", [tag], |row| row.get(0))
        .map_err(s)
}

fn stored_group(connection: &Connection, id: i64) -> Res<Option<StoredGroup>> {
    connection
        .query_row("SELECT id, name, created_at FROM groups WHERE id = ?1", [id], |row| {
            Ok(StoredGroup { id: row.get(0)?, name: row.get(1)?, created_at: row.get(2)? })
        })
        .optional()
        .map_err(s)
}

fn group_named(connection: &Connection, name: &str) -> Res<Option<StoredGroup>> {
    connection
        .query_row("SELECT id, name, created_at FROM groups WHERE name = ?1", [name], |row| {
            Ok(StoredGroup { id: row.get(0)?, name: row.get(1)?, created_at: row.get(2)? })
        })
        .optional()
        .map_err(s)
}

fn find_group(connection: &Connection, group: &GroupRef) -> Res<StoredGroup> {
    match group {
        GroupRef::Id(id) => stored_group(connection, *id)?.ok_or_else(|| format!("分组 #{id} 不存在")),
        GroupRef::Name(name) => group_named(connection, name.trim())?.ok_or_else(|| format!("分组「{}」不存在", name.trim())),
    }
}

/// Put a deleted group back under its old ID; fails if the ID or name has been taken since.
fn restore_group(transaction: &Transaction<'_>, group: &StoredGroup) -> Res<bool> {
    let taken: bool = transaction
        .query_row("SELECT EXISTS(SELECT 1 FROM groups WHERE id = ?1 OR name = ?2)", params![group.id, group.name], |row| row.get(0))
        .map_err(s)?;
    if taken {
        return Ok(false);
    }
    transaction
        .execute("INSERT INTO groups(id, name, created_at) VALUES (?1, ?2, ?3)", params![group.id, group.name, group.created_at])
        .map_err(s)?;
    Ok(true)
}

fn distinct_tags(tags: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    tags.iter().map(|tag| tag.trim().to_owned()).filter(|tag| !tag.is_empty() && seen.insert(tag.clone())).collect()
}

fn alias_of(connection: &Connection, key: &str) -> Res<Option<String>> {
    connection
        .query_row("SELECT alias FROM dedupe_aliases WHERE mode = 'artists' AND key = ?1", [key], |row| row.get(0))
        .optional()
        .map_err(s)
}

fn representative_of(connection: &Connection, key: &str) -> Res<Option<i64>> {
    connection
        .query_row("SELECT row_id FROM artist_representatives WHERE artist_key = ?1", [key], |row| row.get(0))
        .optional()
        .map_err(s)
}

impl Database {
    pub(crate) fn agent_library_plan(&self, op: &LibraryOp) -> Res<LibraryPlan> {
        let connection = &self.connection;
        let mut plan = LibraryPlan { summary: Vec::new(), changes: Vec::new(), warnings: Vec::new(), new_groups: Vec::new(), new_tags: Vec::new(), artist_key: None };
        match op {
            LibraryOp::RenameGroup { group, new_name } => {
                let group = find_group(connection, group)?;
                let new_name = new_name.trim();
                if new_name.is_empty() {
                    return Err("新的分组名称不能为空".into());
                }
                if new_name == group.name {
                    return Err("新名称和原来一样".into());
                }
                if let Some(other) = group_named(connection, new_name)? {
                    let count = group_rows(connection, other.id)?.len();
                    return Err(format!("已经有名为「{new_name}」的分组（{count} 张）。想把两组并在一起请用 mergeGroups"));
                }
                let count = group_rows(connection, group.id)?.len();
                plan.summary.push(format!("分组「{}」改名为「{new_name}」，组里 {count} 张图不变", group.name));
            }
            LibraryOp::MergeGroups { groups, into } => {
                let target = match into {
                    GroupRef::Name(name) if group_named(connection, name.trim())?.is_none() => {
                        let name = name.trim();
                        if name.is_empty() {
                            return Err("目标分组名称不能为空".into());
                        }
                        plan.new_groups.push(name.to_owned());
                        None
                    }
                    other => Some(find_group(connection, other)?),
                };
                let target_name = target.as_ref().map_or_else(|| plan.new_groups[0].clone(), |group| group.name.clone());
                let mut sources = Vec::new();
                for group in groups {
                    let group = find_group(connection, group)?;
                    if target.as_ref().is_none_or(|target| target.id != group.id) && !sources.iter().any(|item: &StoredGroup| item.id == group.id) {
                        sources.push(group);
                    }
                }
                if sources.is_empty() {
                    return Err("至少需要一个和目标不同的来源分组".into());
                }
                let mut parts = Vec::new();
                for source in &sources {
                    let rows = group_rows(connection, source.id)?;
                    parts.push(format!("「{}」{} 张", source.name, rows.len()));
                    for id in rows {
                        plan.changes.push(RowChange {
                            group: Some(GroupChange {
                                before: Some(source.id),
                                after: Some(target.as_ref().map_or(-1, |group| group.id)),
                                before_name: Some(source.name.clone()),
                                after_name: Some(target_name.clone()),
                            }),
                            ..empty_change(id)
                        });
                    }
                }
                plan.summary.push(format!("把分组{}合并到「{target_name}」，合并后删除原来的分组", parts.join("、")));
                if target.is_none() {
                    plan.summary.push(format!("「{target_name}」是新分组，执行时会自动创建"));
                }
            }
            LibraryOp::DeleteGroups { groups } => {
                let mut seen = Vec::new();
                for group in groups {
                    let group = find_group(connection, group)?;
                    if seen.contains(&group.id) {
                        continue;
                    }
                    seen.push(group.id);
                    let rows = group_rows(connection, group.id)?;
                    plan.summary.push(format!("删除分组「{}」：组里 {} 张图变成未分组，图片本身不会删除", group.name, rows.len()));
                    for id in rows {
                        plan.changes.push(RowChange {
                            group: Some(GroupChange { before: Some(group.id), after: None, before_name: Some(group.name.clone()), after_name: None }),
                            ..empty_change(id)
                        });
                    }
                }
                if seen.is_empty() {
                    return Err("至少要指定一个分组".into());
                }
            }
            LibraryOp::DeleteEmptyGroups => {
                let empty = self.list_groups().map_err(s)?.into_iter().filter(|group| group.member_count == 0)
                    .map(|group| group.name).collect::<Vec<_>>();
                if empty.is_empty() {
                    return Err("现在没有空分组".into());
                }
                plan.summary.push(format!("删除 {} 个没有图片的空分组：{}", empty.len(), quote_list(&empty)));
            }
            LibraryOp::RenameTag { tag, new_name } => {
                let (tag, new_name) = (tag.trim(), new_name.trim());
                if !tag_exists(connection, tag)? {
                    return Err(format!("TAG「{tag}」不存在（区分大小写）"));
                }
                if new_name.is_empty() || new_name == tag {
                    return Err("新的 TAG 名称不能为空，也不能和原来一样".into());
                }
                if tag_exists(connection, new_name)? {
                    return Err(format!("已经有 TAG「{new_name}」。想把两个 TAG 并在一起请用 mergeTags"));
                }
                let rows = tag_rows(connection, tag)?;
                let materials = tag_materials(connection, tag)?.len();
                plan.summary.push(format!("TAG「{tag}」改名为「{new_name}」：{} 张图{}跟着改", rows.len(),
                    if materials > 0 { format!("和 {materials} 个素材") } else { String::new() }));
                plan.changes = rows.into_iter().map(|id| RowChange {
                    tags_added: vec![new_name.to_owned()], tags_removed: vec![tag.to_owned()], ..empty_change(id)
                }).collect();
            }
            LibraryOp::MergeTags { tags, into } => {
                let into = into.trim();
                if into.is_empty() {
                    return Err("目标 TAG 不能为空".into());
                }
                let sources = distinct_tags(tags).into_iter().filter(|tag| tag != into).collect::<Vec<_>>();
                if sources.is_empty() {
                    return Err("至少需要一个和目标不同的来源 TAG".into());
                }
                for tag in &sources {
                    if !tag_exists(connection, tag)? {
                        return Err(format!("TAG「{tag}」不存在（区分大小写）"));
                    }
                }
                if !tag_exists(connection, into)? {
                    plan.new_tags.push(into.to_owned());
                }
                let holders = tag_rows(connection, into)?.into_iter().collect::<BTreeSet<_>>();
                let mut by_row: HashMap<i64, Vec<String>> = HashMap::new();
                let mut parts = Vec::new();
                let mut materials = BTreeSet::new();
                for tag in &sources {
                    let rows = tag_rows(connection, tag)?;
                    parts.push(format!("「{tag}」{} 张", rows.len()));
                    for id in rows {
                        by_row.entry(id).or_default().push(tag.clone());
                    }
                    materials.extend(tag_materials(connection, tag)?);
                }
                let mut rows = by_row.into_iter().collect::<Vec<_>>();
                rows.sort_by_key(|(id, _)| *id);
                plan.changes = rows.into_iter().map(|(id, removed)| RowChange {
                    tags_added: if holders.contains(&id) { Vec::new() } else { vec![into.to_owned()] },
                    tags_removed: removed,
                    ..empty_change(id)
                }).collect();
                plan.summary.push(format!("把 TAG {}合并成「{into}」，合并后删除原来的 TAG", parts.join("、")));
                if !materials.is_empty() {
                    plan.summary.push(format!("{} 个素材上的这些 TAG 也会一起换成「{into}」", materials.len()));
                }
            }
            LibraryOp::DeleteTags { tags } => {
                let tags = distinct_tags(tags);
                if tags.is_empty() {
                    return Err("至少要指定一个 TAG".into());
                }
                let mut by_row: HashMap<i64, Vec<String>> = HashMap::new();
                for tag in &tags {
                    if !tag_exists(connection, tag)? {
                        return Err(format!("TAG「{tag}」不存在（区分大小写）"));
                    }
                    let rows = tag_rows(connection, tag)?;
                    let materials = tag_materials(connection, tag)?.len();
                    plan.summary.push(format!("删除 TAG「{tag}」：从 {} 张图{}上去掉，图片本身不会删除", rows.len(),
                        if materials > 0 { format!("和 {materials} 个素材") } else { String::new() }));
                    for id in rows {
                        by_row.entry(id).or_default().push(tag.clone());
                    }
                }
                let mut rows = by_row.into_iter().collect::<Vec<_>>();
                rows.sort_by_key(|(id, _)| *id);
                plan.changes = rows.into_iter().map(|(id, removed)| RowChange { tags_removed: removed, ..empty_change(id) }).collect();
            }
            LibraryOp::SetArtistAlias { artist_string, alias } => {
                let key = self.artist_key(artist_string.trim()).map_err(s)?.ok_or_else(|| format!("无法从「{artist_string}」识别出画师串"))?;
                let count: i64 = connection
                    .query_row("SELECT COUNT(*) FROM rows WHERE artist_match_key = ?1", [&key], |row| row.get(0))
                    .map_err(s)?;
                if count == 0 {
                    plan.warnings.push("全库目前没有属于这个画师串的图片".into());
                }
                let previous = alias_of(connection, &key)?.filter(|text| !text.is_empty());
                let alias = alias.trim();
                if previous.as_deref().unwrap_or("") == alias {
                    return Err("别名和现在一样，不需要修改".into());
                }
                plan.summary.push(format!(
                    "画师串「{}」（{count} 张）的别名：{} → {}",
                    artist_string.trim(),
                    previous.map_or("无".to_owned(), |text| format!("「{text}」")),
                    if alias.is_empty() { "清除".to_owned() } else { format!("「{alias}」") },
                ));
                plan.artist_key = Some(key);
            }
            LibraryOp::SetArtistRepresentative { row_id, enabled } => {
                let key: Option<String> = connection
                    .query_row("SELECT artist_match_key FROM rows WHERE id = ?1", [row_id], |row| row.get(0))
                    .optional()
                    .map_err(s)?
                    .ok_or_else(|| format!("图片 #{row_id} 不存在"))?;
                let key = key.ok_or("这张图没有可识别的画师串，不能设为代表图")?;
                let current = representative_of(connection, &key)?;
                if *enabled {
                    if current == Some(*row_id) {
                        return Err("这张图已经是代表图了".into());
                    }
                    plan.summary.push(format!("把 #{row_id} 设为它所在画师串的代表图（{}）",
                        current.map_or("原来没有代表图".to_owned(), |id| format!("原来是 #{id}"))));
                } else {
                    if current != Some(*row_id) {
                        return Err("这张图现在不是代表图".into());
                    }
                    plan.summary.push(format!("取消 #{row_id} 的代表图身份"));
                }
                plan.changes.push(empty_change(*row_id));
            }
        }
        Ok(plan)
    }
}

/// Run the operation inside `transaction`. Returns the changes as stored (real group IDs)
/// and what undo needs.
pub(crate) fn library_apply(
    transaction: &Transaction<'_>,
    op: &LibraryOp,
    plan: &LibraryPlan,
) -> Res<(Vec<RowChange>, Value)> {
    let mut changes = plan.changes.clone();
    let undo = match op {
        LibraryOp::RenameGroup { group, new_name } => {
            let group = find_group(transaction, group)?;
            let new_name = new_name.trim();
            transaction.execute("UPDATE groups SET name = ?2 WHERE id = ?1", params![group.id, new_name]).map_err(s)?;
            json!({"groupId": group.id, "oldName": group.name, "newName": new_name})
        }
        LibraryOp::MergeGroups { groups, into } => {
            let (target, created) = match find_group(transaction, into) {
                Ok(group) => (group.id, false),
                Err(_) => {
                    let GroupRef::Name(name) = into else { return Err("目标分组不存在".into()) };
                    transaction.execute("INSERT INTO groups(name) VALUES (?1)", [name.trim()]).map_err(s)?;
                    (transaction.last_insert_rowid(), true)
                }
            };
            let mut sources = Vec::new();
            for group in groups {
                let group = find_group(transaction, group)?;
                if group.id != target && !sources.iter().any(|item: &StoredGroup| item.id == group.id) {
                    sources.push(group);
                }
            }
            for source in &sources {
                transaction.execute("UPDATE rows SET group_id = ?2 WHERE group_id = ?1", params![source.id, target]).map_err(s)?;
                transaction.execute("DELETE FROM groups WHERE id = ?1", [source.id]).map_err(s)?;
            }
            for change in &mut changes {
                if let Some(group) = &mut change.group {
                    group.after = Some(target);
                }
            }
            json!({"sources": sources, "intoId": target, "intoCreated": created})
        }
        LibraryOp::DeleteGroups { groups } => {
            let mut stored = Vec::new();
            for group in groups {
                let group = find_group(transaction, group)?;
                if stored.iter().any(|item: &StoredGroup| item.id == group.id) {
                    continue;
                }
                transaction.execute("UPDATE rows SET group_id = NULL WHERE group_id = ?1", [group.id]).map_err(s)?;
                transaction.execute("DELETE FROM groups WHERE id = ?1", [group.id]).map_err(s)?;
                stored.push(group);
            }
            json!({"groups": stored})
        }
        LibraryOp::DeleteEmptyGroups => {
            let empty = {
                let mut statement = transaction.prepare(
                    "SELECT id, name, created_at FROM groups
                     WHERE NOT EXISTS (SELECT 1 FROM rows WHERE rows.group_id = groups.id)",
                ).map_err(s)?;
                statement
                    .query_map([], |row| Ok(StoredGroup { id: row.get(0)?, name: row.get(1)?, created_at: row.get(2)? }))
                    .map_err(s)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(s)?
            };
            for group in &empty {
                transaction.execute("DELETE FROM groups WHERE id = ?1", [group.id]).map_err(s)?;
            }
            json!({"groups": empty})
        }
        LibraryOp::RenameTag { tag, new_name } => {
            transaction
                .execute("UPDATE tags SET name = ?2 WHERE name = ?1 COLLATE BINARY", params![tag.trim(), new_name.trim()])
                .map_err(s)?;
            json!({"oldName": tag.trim(), "newName": new_name.trim()})
        }
        LibraryOp::MergeTags { tags, into } => {
            let into = into.trim();
            let created = transaction.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [into]).map_err(s)? > 0;
            let into_id: i64 = transaction
                .query_row("SELECT id FROM tags WHERE name = ?1 COLLATE BINARY", [into], |row| row.get(0))
                .map_err(s)?;
            let holders = tag_materials(transaction, into)?.into_iter().collect::<BTreeSet<_>>();
            let mut sources = Vec::new();
            let mut added_materials = BTreeSet::new();
            for tag in distinct_tags(tags).into_iter().filter(|tag| tag != into) {
                let materials = tag_materials(transaction, &tag)?;
                added_materials.extend(materials.iter().copied().filter(|id| !holders.contains(id)));
                transaction.execute(
                    "INSERT OR IGNORE INTO row_tags(row_id, tag_id)
                     SELECT row_tags.row_id, ?2 FROM row_tags JOIN tags ON tags.id = row_tags.tag_id
                     WHERE tags.name = ?1 COLLATE BINARY",
                    params![tag, into_id],
                ).map_err(s)?;
                transaction.execute(
                    "INSERT OR IGNORE INTO material_tags(material_id, tag_id)
                     SELECT material_tags.material_id, ?2 FROM material_tags JOIN tags ON tags.id = material_tags.tag_id
                     WHERE tags.name = ?1 COLLATE BINARY",
                    params![tag, into_id],
                ).map_err(s)?;
                transaction.execute("DELETE FROM tags WHERE name = ?1 COLLATE BINARY", [&tag]).map_err(s)?;
                sources.push(json!({"name": tag, "materialIds": materials}));
            }
            json!({"sources": sources, "into": into, "intoCreated": created, "materialsAddedInto": added_materials})
        }
        LibraryOp::DeleteTags { tags } => {
            let mut stored = Vec::new();
            for tag in distinct_tags(tags) {
                let materials = tag_materials(transaction, &tag)?;
                transaction.execute("DELETE FROM tags WHERE name = ?1 COLLATE BINARY", [&tag]).map_err(s)?;
                stored.push(json!({"name": tag, "materialIds": materials}));
            }
            json!({"tags": stored})
        }
        LibraryOp::SetArtistAlias { alias, .. } => {
            let key = plan.artist_key.clone().ok_or("无法识别画师串")?;
            let previous = alias_of(transaction, &key)?;
            transaction.execute(
                "INSERT INTO dedupe_aliases (mode, key, alias) VALUES ('artists', ?1, ?2)
                 ON CONFLICT(mode, key) DO UPDATE SET alias = excluded.alias",
                params![key, alias.trim()],
            ).map_err(s)?;
            json!({"key": key, "previous": previous, "alias": alias.trim()})
        }
        LibraryOp::SetArtistRepresentative { row_id, enabled } => {
            let key: String = transaction
                .query_row("SELECT artist_match_key FROM rows WHERE id = ?1", [row_id], |row| row.get(0))
                .map_err(s)?;
            let previous = representative_of(transaction, &key)?;
            if *enabled {
                transaction.execute(
                    "INSERT INTO artist_representatives(artist_key, row_id) VALUES (?1, ?2)
                     ON CONFLICT(artist_key) DO UPDATE SET row_id = excluded.row_id",
                    params![key, row_id],
                ).map_err(s)?;
            } else {
                transaction.execute("DELETE FROM artist_representatives WHERE row_id = ?1", [row_id]).map_err(s)?;
            }
            json!({"key": key, "previous": previous, "now": if *enabled { Some(*row_id) } else { None }})
        }
    };
    Ok((changes, undo))
}

/// Undo one library operation. Returns (restored, skipped, message).
pub(crate) fn library_undo(
    transaction: &Transaction<'_>,
    op: &LibraryOp,
    undo: &Value,
    changes: &[RowChange],
) -> Res<(usize, usize, String)> {
    let restore_rows = super::apply::restore_changes;
    let groups = |key: &str| -> Res<Vec<StoredGroup>> { serde_json::from_value(undo[key].clone()).map_err(s) };
    Ok(match op {
        LibraryOp::RenameGroup { .. } => {
            let (id, old, new) = (undo["groupId"].as_i64().unwrap_or(0), undo["oldName"].as_str().unwrap_or(""), undo["newName"].as_str().unwrap_or(""));
            let current = stored_group(transaction, id)?;
            if current.as_ref().is_some_and(|group| group.name == new) && group_named(transaction, old)?.is_none() {
                transaction.execute("UPDATE groups SET name = ?2 WHERE id = ?1", params![id, old]).map_err(s)?;
                (1, 0, format!("分组已改回「{old}」"))
            } else {
                (0, 1, "分组之后又被改名、删除，或旧名字已被占用，没有改回".into())
            }
        }
        LibraryOp::MergeGroups { .. } | LibraryOp::DeleteGroups { .. } => {
            let stored = groups(if matches!(op, LibraryOp::MergeGroups { .. }) { "sources" } else { "groups" })?;
            let mut restored_groups = Vec::new();
            for group in &stored {
                if restore_group(transaction, group)? {
                    restored_groups.push(group.id);
                }
            }
            let (rows, blocked): (Vec<_>, Vec<_>) = changes.iter().cloned().partition(|change| {
                change.group.as_ref().and_then(|group| group.before).is_some_and(|id| restored_groups.contains(&id))
            });
            let (restored, skipped) = restore_rows(transaction, &rows)?;
            if undo["intoCreated"].as_bool() == Some(true)
                && let Some(id) = undo["intoId"].as_i64() {
                transaction.execute("DELETE FROM groups WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM rows WHERE group_id = ?1)", [id]).map_err(s)?;
            }
            let lost = stored.len() - restored_groups.len();
            let mut message = format!("已恢复 {} 个分组，{restored} 张图回到原来的分组", restored_groups.len());
            if lost > 0 {
                message.push_str(&format!("；{lost} 个分组的名字或编号已被占用，没有恢复"));
            }
            if skipped + blocked.len() > 0 {
                message.push_str(&format!("；{} 张图之后又被改过，保留现状", skipped + blocked.len()));
            }
            (restored, skipped + blocked.len(), message)
        }
        LibraryOp::DeleteEmptyGroups => {
            let stored = groups("groups")?;
            let mut restored = 0;
            for group in &stored {
                restored += usize::from(restore_group(transaction, group)?);
            }
            (restored, stored.len() - restored, format!("已恢复 {restored} 个空分组{}",
                if restored < stored.len() { format!("，{} 个因名字被占用没有恢复", stored.len() - restored) } else { String::new() }))
        }
        LibraryOp::RenameTag { .. } => {
            let (old, new) = (undo["oldName"].as_str().unwrap_or(""), undo["newName"].as_str().unwrap_or(""));
            if tag_exists(transaction, new)? && !tag_exists(transaction, old)? {
                transaction.execute("UPDATE tags SET name = ?2 WHERE name = ?1 COLLATE BINARY", params![new, old]).map_err(s)?;
                (1, 0, format!("TAG 已改回「{old}」"))
            } else {
                (0, 1, "TAG 之后又被改名或删除，或旧名字已被占用，没有改回".into())
            }
        }
        LibraryOp::MergeTags { .. } | LibraryOp::DeleteTags { .. } => {
            let merge = matches!(op, LibraryOp::MergeTags { .. });
            let stored = undo[if merge { "sources" } else { "tags" }].as_array().cloned().unwrap_or_default();
            for item in &stored {
                let name = item["name"].as_str().unwrap_or_default();
                transaction.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [name]).map_err(s)?;
                for id in item["materialIds"].as_array().into_iter().flatten().filter_map(Value::as_i64) {
                    transaction.execute(
                        "INSERT OR IGNORE INTO material_tags SELECT m.id, t.id FROM materials m, tags t WHERE m.id = ?1 AND t.name = ?2 COLLATE BINARY",
                        params![id, name],
                    ).map_err(s)?;
                }
            }
            let (restored, skipped) = restore_rows(transaction, changes)?;
            if merge {
                let into = undo["into"].as_str().unwrap_or_default();
                for id in undo["materialsAddedInto"].as_array().into_iter().flatten().filter_map(Value::as_i64) {
                    transaction.execute(
                        "DELETE FROM material_tags WHERE material_id = ?1 AND tag_id = (SELECT id FROM tags WHERE name = ?2 COLLATE BINARY)",
                        params![id, into],
                    ).map_err(s)?;
                }
                if undo["intoCreated"].as_bool() == Some(true) {
                    transaction.execute(
                        "DELETE FROM tags WHERE name = ?1 COLLATE BINARY
                         AND NOT EXISTS (SELECT 1 FROM row_tags WHERE row_tags.tag_id = tags.id)
                         AND NOT EXISTS (SELECT 1 FROM material_tags WHERE material_tags.tag_id = tags.id)",
                        [into],
                    ).map_err(s)?;
                }
            }
            let mut message = format!("已恢复 {} 个 TAG，{restored} 张图恢复原样", stored.len());
            if skipped > 0 {
                message.push_str(&format!("；{skipped} 张图之后又被改过，保留现状"));
            }
            (restored, skipped, message)
        }
        LibraryOp::SetArtistAlias { .. } => {
            let key = undo["key"].as_str().unwrap_or_default();
            let alias = undo["alias"].as_str().unwrap_or_default();
            if alias_of(transaction, key)?.as_deref().unwrap_or("") != alias {
                (0, 1, "这个画师串的别名之后又被改过，保留现状".into())
            } else {
                match undo["previous"].as_str() {
                    Some(previous) => transaction.execute(
                        "UPDATE dedupe_aliases SET alias = ?2 WHERE mode = 'artists' AND key = ?1", params![key, previous],
                    ),
                    None => transaction.execute("DELETE FROM dedupe_aliases WHERE mode = 'artists' AND key = ?1", [key]),
                }.map_err(s)?;
                (1, 0, "别名已恢复".into())
            }
        }
        LibraryOp::SetArtistRepresentative { .. } => {
            let key = undo["key"].as_str().unwrap_or_default();
            if representative_of(transaction, key)? != undo["now"].as_i64() {
                (0, 1, "代表图之后又被改过，保留现状".into())
            } else {
                transaction.execute("DELETE FROM artist_representatives WHERE artist_key = ?1", [key]).map_err(s)?;
                if let Some(previous) = undo["previous"].as_i64() {
                    transaction.execute(
                        "INSERT OR IGNORE INTO artist_representatives(artist_key, row_id)
                         SELECT ?1, id FROM rows WHERE id = ?2 AND artist_match_key = ?1",
                        params![key, previous],
                    ).map_err(s)?;
                }
                (1, 0, "代表图已恢复".into())
            }
        }
    })
}
