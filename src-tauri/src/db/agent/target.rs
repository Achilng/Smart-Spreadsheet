//! Resolve an agent's target description into concrete rows.
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{Res, s};
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub scope: Scope,
    #[serde(default)]
    pub row_ids: Vec<i64>,
    #[serde(default)]
    pub artist_strings: Vec<String>,
    #[serde(default)]
    pub expand_to_same_artist_string: bool,
    #[serde(default)]
    pub favorite: Option<bool>,
    #[serde(default)]
    pub conditions: Vec<RuleCondition>,
    #[serde(default)]
    pub condition_match: Option<RuleMatchMode>,
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
}

impl Database {
    pub(crate) fn agent_row_index(&self) -> Res<HashMap<i64, RowIndexEntry>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, artist_match_key, favorite FROM rows")
            .map_err(s)?;
        let entries = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    RowIndexEntry {
                        artist_key: row.get(1)?,
                        favorite: row.get(2)?,
                    },
                ))
            })
            .map_err(s)?
            .collect::<Result<HashMap<_, _>, _>>()
            .map_err(s)?;
        Ok(entries)
    }

    /// Resolve only the row IDs (no conditions), used by selection tools as well.
    pub(crate) fn agent_resolve(&mut self, target: &Target, view: &MainView) -> Res<Resolved> {
        for condition in &target.conditions {
            validate_condition(condition).map_err(s)?;
        }
        let index = self.agent_row_index()?;
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
        if target.expand_to_same_artist_string {
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
        let mut ids = ids.into_iter().collect::<Vec<_>>();
        ids.sort_unstable();
        let mut rows = if ids.is_empty() {
            Vec::new()
        } else {
            load_rule_rows(&self.connection, Some(&ids)).map_err(s)?
        };
        if !target.conditions.is_empty() {
            let set = RuleConditionSet {
                mode: RuleMatchMode::All,
                negate: false,
                groups: vec![RuleConditionGroup {
                    mode: target.condition_match.unwrap_or(RuleMatchMode::All),
                    conditions: target.conditions.clone(),
                }],
            };
            let prepared = PreparedConditionSet::new(&set).map_err(s)?;
            rows.retain(|row| prepared.matches(row));
        }
        Ok(Resolved { rows, index, notes })
    }
}
