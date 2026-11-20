//! Resolve an agent's target description into concrete rows.
use std::collections::{HashMap, HashSet};

use rusqlite::params_from_iter;
use serde::{Deserialize, Serialize};

use super::{Res, s};
use super::model_badge::{badge_key, badge_label};
use crate::automation::matching::PreparedConditionSet;
use crate::automation::model::{
    RuleCondition, RuleConditionGroup, RuleConditionSet, RuleMatchMode, RuleRow,
};
use crate::automation::validation::validate_condition;
use crate::db::automation_rules::execution::load_rule_rows;
use crate::db::{Database, RowSelection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    Selection,
    FilteredView,
    All,
    RowIds,
}

/// Identities shared by the main window's dedupe views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SameKind {
    Artists,
    PositivePrompt,
    Vibes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    pub scope: Scope,
    #[serde(default)]
    pub row_ids: Vec<i64>,
    #[serde(default)]
    pub artist_strings: Vec<String>,
    #[serde(default)]
    pub expand_to_same_artist_string: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expand_to_same: Vec<SameKind>,
    #[serde(default)]
    pub favorite: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_badge: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub batch_ids: Vec<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_before: Option<String>,
    /// Keep only rows an automation rule's conditions would match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<i64>,
    #[serde(default)]
    pub conditions: Vec<RuleCondition>,
    #[serde(default)]
    pub condition_match: Option<RuleMatchMode>,
}

impl Target {
    pub fn row_ids(ids: Vec<i64>) -> Self {
        Self {
            scope: Scope::RowIds,
            row_ids: ids,
            artist_strings: Vec::new(),
            expand_to_same_artist_string: false,
            expand_to_same: Vec::new(),
            favorite: None,
            model_badge: None,
            batch_ids: Vec::new(),
            imported_after: None,
            imported_before: None,
            updated_after: None,
            updated_before: None,
            rule_id: None,
            conditions: Vec::new(),
            condition_match: None,
        }
    }

    pub fn all() -> Self {
        Self { scope: Scope::All, ..Self::row_ids(Vec::new()) }
    }

    /// Whether the base scope covers the library before applying rule conditions.
    pub(crate) fn whole_library_scope(&self) -> bool {
        self.scope == Scope::All && self.artist_strings.is_empty()
            && !self.expand_to_same_artist_string && self.expand_to_same.is_empty()
            && self.model_badge.is_none() && self.favorite.is_none() && !self.has_time_filter()
    }

    fn has_time_filter(&self) -> bool {
        !self.batch_ids.is_empty() || self.imported_after.is_some() || self.imported_before.is_some()
            || self.updated_after.is_some() || self.updated_before.is_some()
    }

    /// Conditions that need full rows to evaluate.
    pub(crate) fn has_row_filters(&self) -> bool {
        !self.conditions.is_empty() || self.rule_id.is_some()
    }
}

/// What the main window last reported; the agent never guesses a selection.
#[derive(Debug, Clone, Default)]
pub struct MainView {
    pub selection: Option<RowSelection>,
    pub filtered: Option<RowSelection>,
}

pub(crate) struct RowIndexEntry {
    pub(crate) artist_key: Option<String>,
    pub(crate) favorite: bool,
    /// Artist string confirmed by LLM style extraction; it outranks the rule-based derivation.
    pub(crate) llm_artists: Option<String>,
}

pub(crate) struct Resolved {
    pub(crate) rows: Vec<RuleRow>,
    pub(crate) index: HashMap<i64, RowIndexEntry>,
    pub(crate) notes: Vec<String>,
}

impl Resolved {
    pub(crate) fn favorite(&self, id: i64) -> bool {
        self.index.get(&id).is_some_and(|entry| entry.favorite)
    }

    pub(crate) fn llm_artists(&self, id: i64) -> Option<&str> {
        self.index.get(&id)?.llm_artists.as_deref()
    }
}

/// Local time as typed by the user, normalised to the stored UTC text for comparison.
pub(crate) fn parse_local_time(text: &str) -> Res<String> {
    use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
    let text = text.trim();
    let utc = if let Ok(time) = DateTime::parse_from_rfc3339(text) {
        time.with_timezone(&Utc)
    } else {
        let naive = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M"]
            .iter()
            .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
            .or_else(|| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok().and_then(|day| day.and_hms_opt(0, 0, 0)))
            .ok_or_else(|| format!("看不懂时间「{text}」，请写成 2026-11-20 或 2026-11-20 14:30（本地时间）"))?;
        Local
            .from_local_datetime(&naive)
            .earliest()
            .ok_or_else(|| format!("本地时间「{text}」不存在"))?
            .with_timezone(&Utc)
    };
    Ok(utc.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}

/// Stored UTC text shown as local time.
pub(crate) fn local_time(text: Option<&str>) -> Option<String> {
    use chrono::{DateTime, Local};
    let text = text?;
    Some(match DateTime::parse_from_rfc3339(text) {
        Ok(time) => time.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S").to_string(),
        Err(_) => text.to_owned(),
    })
}

/// Condition sets a target adds on top of its row IDs; all must match.
pub(crate) fn prepare_sets(sets: &[RuleConditionSet]) -> Res<Vec<PreparedConditionSet<'_>>> {
    sets.iter().map(|set| PreparedConditionSet::new(set).map_err(s)).collect()
}

pub(crate) fn matches_all(prepared: &[PreparedConditionSet<'_>], row: &RuleRow) -> bool {
    prepared.iter().all(|set| set.matches(row))
}

impl Database {
    pub(crate) fn agent_row_index(&self) -> Res<HashMap<i64, RowIndexEntry>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, artist_match_key, favorite, artist_llm FROM rows")
            .map_err(s)?;
        let entries = statement
            .query_map([], |row| {
                let source: Option<String> = row.get(3)?;
                Ok((
                    row.get::<_, i64>(0)?,
                    RowIndexEntry {
                        artist_key: row.get(1)?,
                        favorite: row.get(2)?,
                        llm_artists: source
                            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                            .and_then(|value| value["artistString"].as_str().map(str::to_owned))
                            .filter(|text| !text.is_empty()),
                    },
                ))
            })
            .map_err(s)?
            .collect::<Result<HashMap<_, _>, _>>()
            .map_err(s)?;
        Ok(entries)
    }

    /// The target's own conditions and, when it names one, an automation rule's conditions.
    pub(crate) fn agent_filter_sets(&self, target: &Target) -> Res<Vec<RuleConditionSet>> {
        let mut sets = Vec::new();
        if let Some(set) = target_conditions(target)? {
            sets.push(set);
        }
        if let Some(id) = target.rule_id {
            let rule = self
                .list_automation_rules()
                .map_err(s)?
                .into_iter()
                .find(|rule| rule.id == id)
                .ok_or_else(|| format!("自动规则 #{id} 不存在"))?;
            sets.push(rule.conditions);
        }
        Ok(sets)
    }

    pub(crate) fn agent_resolve(&mut self, target: &Target, view: &MainView) -> Res<Resolved> {
        let sets = self.agent_filter_sets(target)?;
        let prepared = prepare_sets(&sets)?;
        let index = self.agent_row_index()?;
        let (ids, mut notes) = self.agent_target_ids(target, view, &index)?;
        let mut ids = ids.into_iter().collect::<Vec<_>>();
        ids.sort_unstable();
        let mut rows = if ids.is_empty() {
            Vec::new()
        } else {
            load_rule_rows(&self.connection, Some(&ids)).map_err(s)?
        };
        if !prepared.is_empty() {
            rows.retain(|row| matches_all(&prepared, row));
        }
        if let Some(id) = target.rule_id {
            notes.push(format!("按自动规则 #{id} 的条件筛选后剩 {} 张", rows.len()));
        }
        Ok(Resolved { rows, index, notes })
    }

    /// Resolve the initial target, including artist/favorite filters, before rule conditions.
    pub(crate) fn agent_target_ids(
        &mut self,
        target: &Target,
        view: &MainView,
        index: &HashMap<i64, RowIndexEntry>,
    ) -> Res<(HashSet<i64>, Vec<String>)> {
        let mut notes = Vec::new();
        let mut ids: HashSet<i64> = match target.scope {
            Scope::Selection => {
                let selection = view
                    .selection
                    .as_ref()
                    .ok_or("主窗口当前没有选中任何图片")?;
                self.selected_row_ids(selection).map_err(s)?.into_iter().collect()
            }
            Scope::FilteredView => {
                let filtered = view.filtered.as_ref().ok_or("还没有收到主窗口的筛选状态，请先打开主窗口")?;
                self.selected_row_ids(filtered).map_err(s)?.into_iter().collect()
            }
            Scope::All => index.keys().copied().collect(),
            Scope::RowIds => {
                if target.row_ids.is_empty() {
                    return Err("scope 为 rowIds 时必须提供 rowIds".into());
                }
                let missing = target
                    .row_ids
                    .iter()
                    .filter(|id| !index.contains_key(id))
                    .take(10)
                    .collect::<Vec<_>>();
                if !missing.is_empty() {
                    return Err(format!("这些图片 ID 不存在：{missing:?}"));
                }
                target.row_ids.iter().copied().collect()
            }
        };
        if ids.is_empty() && target.scope == Scope::Selection {
            return Err("主窗口当前没有选中任何图片".into());
        }
        let expand_artists = target.expand_to_same_artist_string || target.expand_to_same.contains(&SameKind::Artists);
        if expand_artists {
            let keys = ids
                .iter()
                .filter_map(|id| index.get(id)?.artist_key.clone())
                .collect::<HashSet<_>>();
            let without = ids
                .iter()
                .filter(|id| index.get(id).is_none_or(|entry| entry.artist_key.is_none()))
                .count();
            let before = ids.len();
            ids.extend(
                index
                    .iter()
                    .filter(|(_, entry)| entry.artist_key.as_ref().is_some_and(|key| keys.contains(key)))
                    .map(|(id, _)| *id),
            );
            notes.push(format!(
                "已扩展到同画师串：涉及 {} 个画师串分组，图片从 {before} 张扩展到 {} 张",
                keys.len(),
                ids.len()
            ));
            if without > 0 {
                notes.push(format!("其中 {without} 张没有可识别的画师串，只保留本身"));
            }
        }
        for kind in &target.expand_to_same {
            let (expression, label) = match kind {
                SameKind::Artists => continue,
                SameKind::PositivePrompt => ("NULLIF(TRIM(COALESCE(positive_prompt, '')), '')", "正向提示词"),
                SameKind::Vibes => ("NULLIF(TRIM(COALESCE(vibe_signature, '')), '')", "VIBE 组合"),
            };
            let before = ids.len();
            let seed = serde_json::to_string(&ids.iter().collect::<Vec<_>>()).map_err(s)?;
            let sql = format!(
                "SELECT id FROM rows WHERE {expression} IN
                 (SELECT {expression} FROM rows WHERE id IN (SELECT value FROM json_each(?1)) AND {expression} IS NOT NULL)"
            );
            let mut statement = self.connection.prepare(&sql).map_err(s)?;
            let found = statement
                .query_map([seed], |row| row.get::<_, i64>(0))
                .map_err(s)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(s)?;
            ids.extend(found);
            notes.push(format!("已扩展到{label}完全相同的图：图片从 {before} 张扩展到 {} 张", ids.len()));
        }
        if !target.artist_strings.is_empty() {
            let mut keys = HashSet::new();
            for text in &target.artist_strings {
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                let key = self
                    .artist_key(text)
                    .map_err(s)?
                    .ok_or_else(|| format!("无法从「{text}」识别出画师串"))?;
                let count = index
                    .values()
                    .filter(|entry| entry.artist_key.as_deref() == Some(key.as_str()))
                    .count();
                notes.push(format!("画师串「{text}」在全库共有 {count} 张图"));
                keys.insert(key);
            }
            ids.retain(|id| {
                index
                    .get(id)
                    .and_then(|entry| entry.artist_key.as_ref())
                    .is_some_and(|key| keys.contains(key))
            });
        }
        if let Some(favorite) = target.favorite {
            ids.retain(|id| index.get(id).is_some_and(|entry| entry.favorite == favorite));
        }
        if let Some(label) = &target.model_badge {
            let key = badge_key(label);
            if key.is_empty() {
                return Err("模型徽章不能为空".into());
            }
            let mut statement = self.connection.prepare(
                "SELECT id, generation_model FROM rows WHERE generation_model IS NOT NULL"
            ).map_err(s)?;
            let mut rows = statement.query([]).map_err(s)?;
            let mut matching = HashSet::new();
            let mut models = HashMap::new();
            while let Some(row) = rows.next().map_err(s)? {
                let id: i64 = row.get(0).map_err(s)?;
                if !ids.contains(&id) { continue; }
                let model: String = row.get(1).map_err(s)?;
                let matched = models.entry(model).or_insert_with_key(|model|
                    badge_label(Some(model)).is_some_and(|label| badge_key(label) == key));
                if *matched { matching.insert(id); }
            }
            ids = matching;
        }
        if target.has_time_filter() {
            let mut clauses = Vec::new();
            let mut values: Vec<String> = Vec::new();
            if !target.batch_ids.is_empty() {
                clauses.push("rows.batch_id IN (SELECT value FROM json_each(?))".to_owned());
                values.push(serde_json::to_string(&target.batch_ids).map_err(s)?);
            }
            let stamp = |column: &str| format!("strftime('%Y-%m-%dT%H:%M:%fZ', {column})");
            for (value, column, operator) in [
                (&target.imported_after, "import_batches.imported_at", ">="),
                (&target.imported_before, "import_batches.imported_at", "<"),
                (&target.updated_after, "rows.updated_at", ">="),
                (&target.updated_before, "rows.updated_at", "<"),
            ] {
                if let Some(text) = value {
                    clauses.push(format!("{} {operator} ?", stamp(column)));
                    values.push(parse_local_time(text)?);
                }
            }
            let sql = format!(
                "SELECT rows.id FROM rows JOIN import_batches ON import_batches.id = rows.batch_id WHERE {}",
                clauses.join(" AND ")
            );
            let mut statement = self.connection.prepare(&sql).map_err(s)?;
            let matching = statement
                .query_map(params_from_iter(values.iter()), |row| row.get::<_, i64>(0))
                .map_err(s)?
                .collect::<Result<HashSet<_>, _>>()
                .map_err(s)?;
            let before = ids.len();
            ids.retain(|id| matching.contains(id));
            notes.push(format!("按导入批次 / 时间筛选：{before} 张中剩 {} 张", ids.len()));
        }
        Ok((ids, notes))
    }
}

pub(super) fn target_conditions(target: &Target) -> Res<Option<RuleConditionSet>> {
    if target.conditions.is_empty() {
        return Ok(None);
    }
    for condition in &target.conditions {
        validate_condition(condition).map_err(s)?;
    }
    let set = RuleConditionSet {
        mode: RuleMatchMode::All,
        negate: false,
        groups: vec![RuleConditionGroup {
            mode: target.condition_match.unwrap_or(RuleMatchMode::All),
            conditions: target.conditions.iter().map(with_spelling_variants).collect(),
        }],
    };
    Ok(Some(set))
}

/// NovelAI reads `_` and a space as the same character, so the agent matches artists
/// under both spellings; saved automation rules keep their exact-match behaviour.
fn with_spelling_variants(condition: &RuleCondition) -> RuleCondition {
    let RuleCondition::Artist { operator, artists } = condition else {
        return condition.clone();
    };
    let mut expanded = Vec::new();
    for artist in artists {
        for variant in [artist.clone(), artist.replace('_', " "), artist.replace(' ', "_")] {
            if !expanded.contains(&variant) {
                expanded.push(variant);
            }
        }
    }
    RuleCondition::Artist { operator: *operator, artists: expanded }
}
