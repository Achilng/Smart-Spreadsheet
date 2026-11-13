//! Store dry-run previews, apply them, and undo applied operations row by row.
use std::collections::HashMap;

use rusqlite::types::Value as SqlValue;
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params, params_from_iter};
use serde::Serialize;
use serde_json::{Value, json};

use super::changes::{Action, PreviewReport, RowChange, action_regex, analyze_matches, field_name, sample_json};
use super::target::{MainView, Scope, Target};
use super::{Res, s};
use crate::automation::model::AutomationRule;
use crate::db::Database;

const OPERATION_LIMIT: i64 = 100;
const PREVIEW_LIMIT: i64 = 60;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    pub id: i64,
    pub conversation_id: Option<String>,
    pub created_at: String,
    pub kind: String,
    pub title: String,
    pub report: Value,
    pub changed_rows: usize,
    pub state: String,
    pub undo_message: Option<String>,
    pub undone_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoOutcome {
    pub operation_id: i64,
    pub restored_rows: usize,
    pub skipped_rows: usize,
    pub message: String,
}

fn column(field: &str) -> Option<&'static str> {
    match field {
        "positive" => Some("positive_prompt"),
        "character" => Some("character_prompt"),
        "negative" => Some("negative_prompt"),
        "artists" => Some("artists"),
        "note" => Some("note"),
        _ => None,
    }
}

fn new_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    format!("p{:x}", nanos)
}

/// Write one side (`before` or `after`) of a change.
fn write_side(transaction: &Transaction<'_>, change: &RowChange, after: bool, group_map: &HashMap<i64, i64>) -> Res<()> {
    let mut sets = Vec::new();
    let mut values: Vec<SqlValue> = Vec::new();
    for item in &change.text {
        let Some(column) = column(&item.field) else { continue };
        let value = if after { &item.after } else { &item.before };
        sets.push(format!("{column} = ?"));
        values.push(value.clone().map_or(SqlValue::Null, SqlValue::Text));
        if item.field == "positive" {
            sets.push("style_signature = ?".into());
            values.push(
                crate::pipeline::style_signature_of(value.as_deref())
                    .map_or(SqlValue::Null, SqlValue::Text),
            );
        }
    }
    if let Some(group) = &change.group {
        let id = if after { group.after } else { group.before };
        let id = id.map(|id| group_map.get(&id).copied().unwrap_or(id));
        sets.push("group_id = ?".into());
        values.push(id.map_or(SqlValue::Null, SqlValue::Integer));
    }
    if let Some([before, next]) = change.favorite {
        sets.push("favorite = ?".into());
        values.push(SqlValue::Integer(i64::from(if after { next } else { before })));
    }
    if !sets.is_empty() {
        values.push(SqlValue::Integer(change.id));
        transaction
            .execute(&format!("UPDATE rows SET {} WHERE id = ?", sets.join(", ")), params_from_iter(values))
            .map_err(s)?;
    }
    let (add, remove) = if after {
        (&change.tags_added, &change.tags_removed)
    } else {
        (&change.tags_removed, &change.tags_added)
    };
    for tag in add {
        transaction.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [tag]).map_err(s)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO row_tags(row_id, tag_id) SELECT ?1, id FROM tags WHERE name = ?2",
                params![change.id, tag],
            )
            .map_err(s)?;
    }
    for tag in remove {
        transaction
            .execute(
                "DELETE FROM row_tags WHERE row_id = ?1 AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
                params![change.id, tag],
            )
            .map_err(s)?;
    }
    Ok(())
}

/// A row may be restored only if every field the agent changed still holds the agent's value.
fn still_as_applied(transaction: &Transaction<'_>, change: &RowChange) -> Res<bool> {
    let current = transaction
        .query_row(
            "SELECT positive_prompt, character_prompt, negative_prompt, artists, note, group_id, favorite
             FROM rows WHERE id = ?1",
            [change.id],
            |row| {
                Ok((
                    [row.get::<_, Option<String>>(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?],
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, bool>(6)?,
                ))
            },
        )
        .optional()
        .map_err(s)?;
    let Some((texts, group_id, favorite)) = current else { return Ok(false) };
    for item in &change.text {
        let index = ["positive", "character", "negative", "artists", "note"]
            .iter()
            .position(|field| *field == item.field);
        if index.is_some_and(|index| texts[index] != item.after) {
            return Ok(false);
        }
    }
    if let Some(group) = &change.group {
        if group_id != group.after {
            return Ok(false);
        }
        if let Some(before) = group.before {
            let exists: bool = transaction
                .query_row("SELECT EXISTS(SELECT 1 FROM groups WHERE id = ?1)", [before], |row| row.get(0))
                .map_err(s)?;
            if !exists {
                return Ok(false);
            }
        }
    }
    if change.favorite.is_some_and(|[_, after]| after != favorite) {
        return Ok(false);
    }
    let has_tag = |tag: &String| -> Res<bool> {
        transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM row_tags JOIN tags ON tags.id = row_tags.tag_id
                 WHERE row_tags.row_id = ?1 AND tags.name = ?2)",
                params![change.id, tag],
                |row| row.get(0),
            )
            .map_err(s)
    };
    for tag in &change.tags_added {
        if !has_tag(tag)? {
            return Ok(false);
        }
    }
    for tag in &change.tags_removed {
        if has_tag(tag)? {
            return Ok(false);
        }
    }
    Ok(true)
}

impl Database {
    pub fn agent_preview(
        &mut self,
        conversation_id: &str,
        title: &str,
        target: &Target,
        actions: &[Action],
        view: &MainView,
    ) -> Res<PreviewReport> {
        let resolved = self.agent_resolve(target, view)?;
        let plan = self.agent_plan(actions)?;
        let changes = self.agent_simulate(&resolved, actions, &plan)?;
        let mut field_counts = std::collections::BTreeMap::new();
        for change in &changes {
            for item in &change.text {
                *field_counts.entry(item.field.clone()).or_insert(0) += 1;
            }
            for (key, present) in [
                ("group", change.group.is_some()),
                ("favorite", change.favorite.is_some()),
                ("tagsAdded", !change.tags_added.is_empty()),
                ("tagsRemoved", !change.tags_removed.is_empty()),
            ] {
                if present {
                    *field_counts.entry(key.into()).or_insert(0) += 1;
                }
            }
        }
        let mut warnings = Vec::new();
        let mut match_analysis = Vec::new();
        for (index, action) in actions.iter().enumerate() {
            let Some((regex, field)) = action_regex(action) else { continue };
            let regex = regex?;
            let mut analysis = analyze_matches(&regex, &resolved.rows, field_name(field));
            analysis["actionIndex"] = json!(index);
            let others = ["positive", "character", "negative"]
                .into_iter()
                .filter(|other| *other != field_name(field))
                .map(|other| {
                    let hit = analyze_matches(&regex, &resolved.rows, other)["rowsMatched"].clone();
                    (other.to_owned(), hit)
                })
                .collect::<serde_json::Map<_, _>>();
            if others.values().any(|value| value.as_u64().unwrap_or(0) > 0) {
                warnings.push(format!(
                    "第 {} 个动作只改 {}，但同样的内容也出现在其它提示词字段里：{}",
                    index + 1,
                    field_name(field),
                    Value::Object(others.clone())
                ));
            }
            analysis["sameMatchInOtherFields"] = Value::Object(others);
            match_analysis.push(analysis);
        }
        for change in &changes {
            if change.text.iter().any(|item| item.field != "artists" && item.field != "note" && item.after.is_none()) {
                warnings.push("有图片的提示词字段在修改后会变为空，请检查样例".into());
                break;
            }
        }
        if changes.iter().any(|change| change.text.iter().any(|item| item.field == "artists")) {
            warnings.push("部分图片的画师串会随提示词改变，画师串分组可能变化".into());
        }
        if changes.is_empty() {
            warnings.push("没有任何图片需要修改".into());
        }
        let samples = pick_samples(&changes, &match_analysis);
        let new_tags = self.agent_missing_tags(actions)?;
        let row_ids = resolved.rows.iter().map(|row| row.id).collect::<Vec<_>>();
        let preview_id = new_id();
        let report = PreviewReport {
            preview_id: preview_id.clone(),
            title: title.trim().to_owned(),
            target: serde_json::to_value(target).map_err(s)?,
            target_notes: resolved.notes,
            actions: actions.to_vec(),
            scanned_rows: resolved.rows.len(),
            changed_rows: changes.len(),
            unchanged_rows: resolved.rows.len() - changes.len(),
            field_counts,
            new_groups: plan.new_groups.clone(),
            new_tags,
            match_analysis,
            samples,
            warnings,
        };
        let plan_json = json!({"rowIds": row_ids, "actions": actions, "title": report.title});
        self.connection
            .execute(
                "INSERT INTO agent_previews(id, conversation_id, plan, changes, report) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    preview_id,
                    conversation_id,
                    plan_json.to_string(),
                    serde_json::to_string(&changes).map_err(s)?,
                    serde_json::to_string(&report).map_err(s)?
                ],
            )
            .map_err(s)?;
        self.connection
            .execute(
                "DELETE FROM agent_previews WHERE id NOT IN
                 (SELECT id FROM agent_previews ORDER BY created_at DESC, rowid DESC LIMIT ?1)",
                [PREVIEW_LIMIT],
            )
            .map_err(s)?;
        Ok(report)
    }

    pub fn agent_apply(&mut self, conversation_id: &str, preview_id: &str) -> Res<OperationRecord> {
        let stored = self
            .connection
            .query_row(
                "SELECT conversation_id, plan, changes, report, used FROM agent_previews WHERE id = ?1",
                [preview_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, bool>(4)?)),
            )
            .optional()
            .map_err(s)?
            .ok_or("预览不存在或已过期，请重新预览")?;
        let (owner, plan_json, changes_json, report_json, used) = stored;
        if owner != conversation_id {
            return Err("这个预览不属于当前对话".into());
        }
        if used {
            return Err("这个预览已经执行过了；如需再次修改请重新预览".into());
        }
        let plan: Value = serde_json::from_str(&plan_json).map_err(s)?;
        let actions: Vec<Action> = serde_json::from_value(plan["actions"].clone()).map_err(s)?;
        let row_ids: Vec<i64> = serde_json::from_value(plan["rowIds"].clone()).map_err(s)?;
        let changes: Vec<RowChange> = serde_json::from_str(&changes_json).map_err(s)?;
        let mut report: Value = serde_json::from_str(&report_json).map_err(s)?;
        if changes.is_empty() {
            return Err("这个预览没有任何需要修改的图片".into());
        }
        // Recompute on current data: any drift since the preview means the user saw stale numbers.
        let current = if row_ids.is_empty() {
            Vec::new()
        } else {
            let target = Target {
                scope: Scope::RowIds,
                row_ids: row_ids.clone(),
                artist_strings: vec![],
                expand_to_same_artist_string: false,
                favorite: None,
                conditions: vec![],
                condition_match: None,
            };
            let resolved = self
                .agent_resolve(&target, &MainView::default())
                .map_err(|_| "预览之后有图片被删除，请重新预览".to_owned())?;
            let plan = self.agent_plan(&actions)?;
            self.agent_simulate(&resolved, &actions, &plan)?
        };
        if current != changes {
            return Err("预览之后数据已经变化，结果可能不同。请重新调用 preview_changes 并把新结果告诉用户".into());
        }
        let plan = self.agent_plan(&actions)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(s)?;
        let mut group_map = HashMap::new();
        let mut created_groups = Vec::new();
        for (index, name) in plan.new_groups.iter().enumerate() {
            transaction.execute("INSERT INTO groups(name) VALUES (?1)", [name]).map_err(s)?;
            let id = transaction.last_insert_rowid();
            group_map.insert(-1 - index as i64, id);
            created_groups.push(id);
        }
        let mut created_tags = Vec::new();
        for change in &changes {
            for tag in &change.tags_added {
                if transaction.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [tag]).map_err(s)? > 0 {
                    created_tags.push(tag.clone());
                }
            }
        }
        let stored_changes = changes
            .into_iter()
            .map(|mut change| {
                if let Some(group) = &mut change.group {
                    group.after = group.after.map(|id| group_map.get(&id).copied().unwrap_or(id));
                }
                change
            })
            .collect::<Vec<_>>();
        for change in &stored_changes {
            write_side(&transaction, change, true, &HashMap::new())?;
        }
        report["samples"] = Value::Array(report["samples"].as_array().cloned().unwrap_or_default());
        let title = report["title"].as_str().unwrap_or("修改").to_owned();
        transaction
            .execute(
                "INSERT INTO agent_operations(conversation_id, kind, title, report, changes, created_tags, created_groups)
                 VALUES (?1, 'rows', ?2, ?3, ?4, ?5, ?6)",
                params![
                    conversation_id,
                    title,
                    report.to_string(),
                    serde_json::to_string(&stored_changes).map_err(s)?,
                    json!(created_tags).to_string(),
                    json!(created_groups).to_string()
                ],
            )
            .map_err(s)?;
        let id = transaction.last_insert_rowid();
        transaction.execute("UPDATE agent_previews SET used = 1 WHERE id = ?1", [preview_id]).map_err(s)?;
        prune_operations(&transaction)?;
        transaction.commit().map_err(s)?;
        self.bump_data_version();
        self.agent_operation(id)?.ok_or_else(|| "读取操作记录失败".into())
    }

    pub fn agent_record_rule(&mut self, conversation_id: &str, rule: &AutomationRule) -> Res<OperationRecord> {
        let report = json!({"ruleId": rule.id, "rule": rule});
        self.connection
            .execute(
                "INSERT INTO agent_operations(conversation_id, kind, title, report, changes)
                 VALUES (?1, 'rule', ?2, ?3, '[]')",
                params![conversation_id, format!("创建自动规则「{}」", rule.name), report.to_string()],
            )
            .map_err(s)?;
        let id = self.connection.last_insert_rowid();
        let transaction = self.connection.transaction().map_err(s)?;
        prune_operations(&transaction)?;
        transaction.commit().map_err(s)?;
        self.agent_operation(id)?.ok_or_else(|| "读取操作记录失败".into())
    }

    pub fn agent_undo(&mut self, operation_id: i64) -> Res<UndoOutcome> {
        let operation = self.agent_operation(operation_id)?.ok_or("操作记录不存在（可能已超出保留的 100 条）")?;
        if operation.state == "undone" {
            return Err("这次修改已经撤销过了".into());
        }
        if operation.kind == "rule" {
            let rule_id = operation.report["ruleId"].as_i64().ok_or("操作记录缺少规则 ID")?;
            let deleted = self.delete_automation_rule(rule_id).map_err(s)?;
            let message = if deleted { "已删除这条自动规则" } else { "这条自动规则已经不存在" }.to_owned();
            self.mark_undone(operation_id, &message)?;
            return Ok(UndoOutcome { operation_id, restored_rows: usize::from(deleted), skipped_rows: 0, message });
        }
        let (changes_json, tags_json, groups_json): (String, String, String) = self
            .connection
            .query_row(
                "SELECT changes, created_tags, created_groups FROM agent_operations WHERE id = ?1",
                [operation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(s)?;
        let changes: Vec<RowChange> = serde_json::from_str(&changes_json).map_err(s)?;
        let created_tags: Vec<String> = serde_json::from_str(&tags_json).map_err(s)?;
        let created_groups: Vec<i64> = serde_json::from_str(&groups_json).map_err(s)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(s)?;
        let (mut restored, mut skipped) = (0, 0);
        for change in &changes {
            if still_as_applied(&transaction, change)? {
                write_side(&transaction, change, false, &HashMap::new())?;
                restored += 1;
            } else {
                skipped += 1;
            }
        }
        for group in created_groups {
            transaction
                .execute(
                    "DELETE FROM groups WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM rows WHERE group_id = ?1)",
                    [group],
                )
                .map_err(s)?;
        }
        for tag in created_tags {
            transaction
                .execute(
                    "DELETE FROM tags WHERE name = ?1
                     AND NOT EXISTS (SELECT 1 FROM row_tags WHERE row_tags.tag_id = tags.id)",
                    [tag],
                )
                .map_err(s)?;
        }
        let message = if skipped == 0 {
            format!("已撤销，{restored} 张图恢复原样")
        } else {
            format!("已撤销 {restored} 张；另有 {skipped} 张在这之后又被改过（或已删除），保留现状")
        };
        transaction
            .execute(
                "UPDATE agent_operations SET state = 'undone', undo_message = ?2,
                 undone_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
                params![operation_id, message],
            )
            .map_err(s)?;
        transaction.commit().map_err(s)?;
        self.bump_data_version();
        Ok(UndoOutcome { operation_id, restored_rows: restored, skipped_rows: skipped, message })
    }

    fn mark_undone(&self, operation_id: i64, message: &str) -> Res<()> {
        self.connection
            .execute(
                "UPDATE agent_operations SET state = 'undone', undo_message = ?2,
                 undone_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
                params![operation_id, message],
            )
            .map_err(s)?;
        Ok(())
    }

    pub fn agent_operations(&self, limit: usize) -> Res<Vec<OperationRecord>> {
        self.query_operations("ORDER BY id DESC LIMIT ?1", params![limit as i64])
    }

    pub fn agent_operation(&self, id: i64) -> Res<Option<OperationRecord>> {
        Ok(self.query_operations("WHERE id = ?1", params![id])?.into_iter().next())
    }

    fn query_operations(&self, tail: &str, values: impl rusqlite::Params) -> Res<Vec<OperationRecord>> {
        let mut statement = self
            .connection
            .prepare(&format!(
                "SELECT id, conversation_id, created_at, kind, title, report, json_array_length(changes),
                        state, undo_message, undone_at FROM agent_operations {tail}"
            ))
            .map_err(s)?;
        let rows = statement
            .query_map(values, |row| {
                let report: String = row.get(5)?;
                Ok(OperationRecord {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    created_at: row.get(2)?,
                    kind: row.get(3)?,
                    title: row.get(4)?,
                    report: serde_json::from_str(&report).unwrap_or(Value::Null),
                    changed_rows: row.get::<_, i64>(6)? as usize,
                    state: row.get(7)?,
                    undo_message: row.get(8)?,
                    undone_at: row.get(9)?,
                })
            })
            .map_err(s)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(s)?;
        Ok(rows)
    }

    /// Page through the full change list of a preview or an applied operation.
    pub fn agent_change_page(&self, source: &str, id: &str, offset: usize, limit: usize) -> Res<(usize, Vec<RowChange>)> {
        let json: Option<String> = match source {
            "preview" => self
                .connection
                .query_row("SELECT changes FROM agent_previews WHERE id = ?1", [id], |row| row.get(0))
                .optional()
                .map_err(s)?,
            "operation" => self
                .connection
                .query_row(
                    "SELECT changes FROM agent_operations WHERE id = ?1",
                    [id.parse::<i64>().map_err(s)?],
                    |row| row.get(0),
                )
                .optional()
                .map_err(s)?,
            _ => return Err("未知的来源".into()),
        };
        let changes: Vec<RowChange> = serde_json::from_str(&json.ok_or("记录不存在或已过期")?).map_err(s)?;
        let total = changes.len();
        Ok((total, changes.into_iter().skip(offset).take(limit.min(200)).collect()))
    }
}

fn prune_operations(transaction: &Transaction<'_>) -> Res<()> {
    transaction
        .execute(
            "DELETE FROM agent_operations WHERE id NOT IN
             (SELECT id FROM agent_operations ORDER BY id DESC LIMIT ?1)",
            [OPERATION_LIMIT],
        )
        .map_err(s)?;
    Ok(())
}

/// Prefer rows that show different matched texts and multi-match rows, then fill in order.
fn pick_samples(changes: &[RowChange], analysis: &[Value]) -> Vec<Value> {
    let mut ids = Vec::new();
    for item in analysis {
        for distinct in item["distinctMatches"].as_array().into_iter().flatten().take(5) {
            if let Some(id) = distinct["sampleRowId"].as_i64() {
                ids.push(id);
            }
        }
        for multi in item["multipleMatchSamples"].as_array().into_iter().flatten().take(2) {
            if let Some(id) = multi["rowId"].as_i64() {
                ids.push(id);
            }
        }
    }
    let by_id = changes.iter().map(|change| (change.id, change)).collect::<HashMap<_, _>>();
    let mut picked = Vec::new();
    for id in ids.into_iter().chain(changes.iter().map(|change| change.id)) {
        if picked.len() >= 8 {
            break;
        }
        if !picked.contains(&id) && by_id.contains_key(&id) {
            picked.push(id);
        }
    }
    picked.into_iter().map(|id| sample_json(by_id[&id])).collect()
}
