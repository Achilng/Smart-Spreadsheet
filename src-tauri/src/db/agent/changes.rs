//! Agent actions, simulation into per-row changes and the dry-run report.
use std::collections::{BTreeMap, HashMap, HashSet};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::model_badge::badge_label;
use super::target::Resolved;
use super::{Res, s};
use crate::automation::actions::{note_sequence_prefixes, simulate_actions};
use crate::automation::matching::build_regex;
use crate::automation::model::{PromptActionField, RuleAction, RuleRow, default_note_separator};
use crate::automation::text::{parse_prompt_tokens, prompt_token_set};
use crate::automation::validation::validate_action;
use crate::db::Database;
use crate::db::automation_rules::execution::seed_note_sequence_counters;
use crate::pipeline::prompt_text::combined_artists;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    AddTags {
        tags: Vec<String>,
    },
    RemoveTags {
        tags: Vec<String>,
    },
    SetGroup {
        #[serde(rename = "groupName")]
        group_name: String,
        #[serde(default, rename = "onlyIfUngrouped")]
        only_if_ungrouped: bool,
    },
    ClearGroup,
    AppendPrompt {
        field: PromptActionField,
        value: String,
    },
    DeletePromptTags {
        field: PromptActionField,
        value: String,
    },
    ReplacePrompt {
        field: PromptActionField,
        find: String,
        #[serde(default)]
        replace: String,
        #[serde(rename = "caseSensitive")]
        case_sensitive: bool,
    },
    RegexReplacePrompt {
        field: PromptActionField,
        pattern: String,
        #[serde(default)]
        replace: String,
        #[serde(rename = "caseSensitive")]
        case_sensitive: bool,
    },
    SetNote {
        value: String,
    },
    AppendNote {
        value: String,
        #[serde(default = "default_note_separator")]
        separator: String,
    },
    ClearNote,
    SetNoteSequence {
        prefix: String,
    },
    SetFavorite {
        favorite: bool,
    },
    PrefixArtist {
        artists: Vec<String>,
    },
}

/// Per-image values usable as `{name}` in group names, notes and TAGs.
pub(crate) const PLACEHOLDERS: &[&str] = &[
    "id", "artists", "artistAlias", "model", "modelBadge", "sampler", "steps", "scale", "seed",
    "width", "height", "fileName",
];

fn uses_placeholder(text: &str) -> bool {
    PLACEHOLDERS.iter().any(|name| text.contains(&format!("{{{name}}}")))
}

impl Action {
    /// Whether this action's text varies per image.
    fn templated(&self) -> bool {
        match self {
            Action::SetGroup { group_name, .. } => uses_placeholder(group_name),
            Action::SetNote { value } | Action::AppendNote { value, .. } => uses_placeholder(value),
            Action::AddTags { tags } => tags.iter().any(|tag| uses_placeholder(tag)),
            _ => false,
        }
    }

    /// Convert an automation rule action; `None` for actions that only make sense during import.
    pub(crate) fn from_rule(action: &RuleAction, group_names: &HashMap<i64, String>) -> Res<Option<Action>> {
        Ok(Some(match action.clone() {
            RuleAction::AddTags { tags } => Action::AddTags { tags },
            RuleAction::RemoveTags { tags } => Action::RemoveTags { tags },
            RuleAction::SetGroup { group_id, only_if_ungrouped } => Action::SetGroup {
                group_name: group_names.get(&group_id).cloned().ok_or_else(|| format!("规则里的分组 #{group_id} 已不存在"))?,
                only_if_ungrouped,
            },
            RuleAction::ClearGroup => Action::ClearGroup,
            RuleAction::AppendPrompt { field, value } => Action::AppendPrompt { field, value },
            RuleAction::DeletePromptTags { field, value } => Action::DeletePromptTags { field, value },
            RuleAction::ReplacePrompt { field, find, replace, case_sensitive } => Action::ReplacePrompt { field, find, replace, case_sensitive },
            RuleAction::RegexReplacePrompt { field, pattern, replace, case_sensitive } => Action::RegexReplacePrompt { field, pattern, replace, case_sensitive },
            RuleAction::PrefixArtist { artists } => Action::PrefixArtist { artists },
            RuleAction::SetNote { value } => Action::SetNote { value },
            RuleAction::SetNoteSequence { prefix } => Action::SetNoteSequence { prefix },
            RuleAction::AppendNote { value, separator } => Action::AppendNote { value, separator },
            RuleAction::ClearNote => Action::ClearNote,
            RuleAction::StopProcessing => return Ok(None),
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextChange {
    pub field: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupChange {
    pub before: Option<i64>,
    pub after: Option<i64>,
    pub before_name: Option<String>,
    pub after_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowChange {
    pub id: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text: Vec<TextChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite: Option<[bool; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags_added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags_removed: Vec<String>,
}

/// Groups that do not exist yet get negative placeholder IDs until apply creates them.
pub(crate) struct Plan {
    /// Fixed rule per action; `None` for favourites and for templated actions.
    pub(crate) rule_actions: Vec<Option<RuleAction>>,
    templated: Vec<bool>,
    pub(crate) new_groups: Vec<String>,
    pub(crate) group_names: HashMap<i64, String>,
    group_ids: HashMap<String, i64>,
    aliases: HashMap<String, String>,
}

impl Plan {
    fn group_id(&mut self, name: &str) -> i64 {
        if let Some(id) = self.group_ids.get(name) {
            return *id;
        }
        self.new_groups.push(name.to_owned());
        let placeholder = -(self.new_groups.len() as i64);
        self.group_names.insert(placeholder, name.to_owned());
        self.group_ids.insert(name.to_owned(), placeholder);
        placeholder
    }

    fn placeholder(&self, name: &str, row: &RuleRow, resolved: &Resolved) -> Option<String> {
        let text = |value: &Option<String>| value.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
        match name {
            "id" => Some(row.id.to_string()),
            "artists" => text(&row.artists),
            "artistAlias" => resolved
                .index
                .get(&row.id)
                .and_then(|entry| entry.artist_key.as_ref())
                .and_then(|key| self.aliases.get(key))
                .map(|alias| alias.trim().to_owned())
                .filter(|alias| !alias.is_empty())
                .or_else(|| text(&row.artists)),
            "model" => text(&row.generation_model),
            "modelBadge" => badge_label(row.generation_model.as_deref()).map(str::to_owned),
            "sampler" => text(&row.generation_sampler),
            "steps" => row.generation_steps.map(|v| v.to_string()),
            "scale" => row.generation_scale.map(|v| v.to_string()),
            "seed" => text(&row.generation_seed),
            "width" => row.image_width.map(|v| v.to_string()),
            "height" => row.image_height.map(|v| v.to_string()),
            "fileName" => row.image_path.as_deref().and_then(|path| path.rsplit(['/', '\\']).next()).map(str::to_owned),
            _ => None,
        }
    }

    /// Fill in a template for one image; `None` when a value it needs is missing.
    fn expand(&self, template: &str, row: &RuleRow, resolved: &Resolved) -> Option<String> {
        let mut text = template.to_owned();
        for name in PLACEHOLDERS {
            let token = format!("{{{name}}}");
            if text.contains(&token) {
                text = text.replace(&token, &self.placeholder(name, row, resolved)?);
            }
        }
        let text = text.trim().to_owned();
        (!text.is_empty()).then_some(text)
    }

    /// The rule a templated action becomes for one image.
    fn expanded(&self, action: &Action, row: &RuleRow, resolved: &Resolved) -> Option<RuleAction> {
        match action {
            Action::SetGroup { group_name, only_if_ungrouped } => {
                let name = self.expand(group_name, row, resolved)?;
                Some(RuleAction::SetGroup { group_id: *self.group_ids.get(&name)?, only_if_ungrouped: *only_if_ungrouped })
            }
            Action::SetNote { value } => Some(RuleAction::SetNote { value: self.expand(value, row, resolved)? }),
            Action::AppendNote { value, separator } => Some(RuleAction::AppendNote {
                value: self.expand(value, row, resolved)?,
                separator: separator.clone(),
            }),
            Action::AddTags { tags } => {
                let tags = tags.iter().filter_map(|tag| self.expand(tag, row, resolved)).collect::<Vec<_>>();
                (!tags.is_empty()).then_some(RuleAction::AddTags { tags })
            }
            _ => None,
        }
    }
}

pub(crate) fn field_name(field: PromptActionField) -> &'static str {
    match field {
        PromptActionField::Positive => "positive",
        PromptActionField::Character => "character",
        PromptActionField::Negative => "negative",
    }
}

fn field_value(row: &RuleRow, field: &str) -> Option<String> {
    match field {
        "positive" => row.positive_prompt.clone(),
        "character" => row.character_prompt.clone(),
        "negative" => row.negative_prompt.clone(),
        "note" => row.note.clone(),
        "artists" => row.artists.clone(),
        _ => None,
    }
}

impl Database {
    /// Rules for each action. Templated actions are filled in per image, so the plan sees the rows.
    pub(crate) fn agent_plan(&self, actions: &[Action], resolved: &Resolved) -> Res<Plan> {
        if actions.is_empty() {
            return Err("至少需要一个修改动作".into());
        }
        let mut plan = Plan {
            rule_actions: Vec::new(),
            templated: Vec::new(),
            new_groups: Vec::new(),
            group_names: HashMap::new(),
            group_ids: HashMap::new(),
            aliases: HashMap::new(),
        };
        for group in self.list_groups().map_err(s)? {
            plan.group_names.insert(group.id, group.name.clone());
            plan.group_ids.insert(group.name, group.id);
        }
        if actions.iter().any(Action::templated) {
            let mut statement = self
                .connection
                .prepare("SELECT key, alias FROM dedupe_aliases WHERE mode = 'artists' AND alias != ''")
                .map_err(s)?;
            plan.aliases = statement
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(s)?
                .collect::<Result<HashMap<_, _>, _>>()
                .map_err(s)?;
        }
        for action in actions {
            if action.templated() {
                if let Action::SetGroup { group_name, .. } = action {
                    for row in &resolved.rows {
                        if let Some(name) = plan.expand(group_name, row, resolved) {
                            plan.group_id(&name);
                        }
                    }
                }
                plan.rule_actions.push(None);
                plan.templated.push(true);
                continue;
            }
            let rule = match action {
                Action::AddTags { tags } => Some(RuleAction::AddTags { tags: tags.clone() }),
                Action::RemoveTags { tags } => Some(RuleAction::RemoveTags { tags: tags.clone() }),
                Action::SetGroup { group_name, only_if_ungrouped } => {
                    let name = group_name.trim();
                    if name.is_empty() {
                        return Err("分组名称不能为空".into());
                    }
                    Some(RuleAction::SetGroup { group_id: plan.group_id(name), only_if_ungrouped: *only_if_ungrouped })
                }
                Action::ClearGroup => Some(RuleAction::ClearGroup),
                Action::AppendPrompt { field, value } => {
                    Some(RuleAction::AppendPrompt { field: *field, value: value.clone() })
                }
                Action::DeletePromptTags { field, value } => {
                    Some(RuleAction::DeletePromptTags { field: *field, value: value.clone() })
                }
                Action::ReplacePrompt { field, find, replace, case_sensitive } => {
                    Some(RuleAction::ReplacePrompt {
                        field: *field,
                        find: find.clone(),
                        replace: replace.clone(),
                        case_sensitive: *case_sensitive,
                    })
                }
                Action::RegexReplacePrompt { field, pattern, replace, case_sensitive } => {
                    Some(RuleAction::RegexReplacePrompt {
                        field: *field,
                        pattern: pattern.clone(),
                        replace: replace.clone(),
                        case_sensitive: *case_sensitive,
                    })
                }
                Action::SetNote { value } => Some(RuleAction::SetNote { value: value.clone() }),
                Action::AppendNote { value, separator } => Some(RuleAction::AppendNote {
                    value: value.clone(),
                    separator: separator.clone(),
                }),
                Action::ClearNote => Some(RuleAction::ClearNote),
                Action::SetNoteSequence { prefix } => {
                    Some(RuleAction::SetNoteSequence { prefix: prefix.clone() })
                }
                Action::PrefixArtist { artists } => Some(RuleAction::PrefixArtist { artists: artists.clone() }),
                Action::SetFavorite { .. } => None,
            };
            if let Some(rule) = &rule {
                match rule {
                    // Placeholder IDs are negative; validation only rejects them for saved rules.
                    RuleAction::SetGroup { .. } => {}
                    other => validate_action(other).map_err(s)?,
                }
            }
            plan.rule_actions.push(rule);
            plan.templated.push(false);
        }
        Ok(plan)
    }

    /// Per-row changes, plus how many (row, templated action) pairs were skipped for a missing value.
    pub(crate) fn agent_simulate(
        &self,
        resolved: &Resolved,
        actions: &[Action],
        plan: &Plan,
    ) -> Res<(Vec<RowChange>, usize)> {
        let rule_list = plan.rule_actions.iter().flatten().cloned().collect::<Vec<_>>();
        let mut counters =
            seed_note_sequence_counters(&self.connection, &note_sequence_prefixes(&rule_list))
                .map_err(s)?;
        let mut changes = Vec::new();
        let mut skipped = 0;
        for row in &resolved.rows {
            let mut updated = row.clone();
            let mut favorite = resolved.favorite(row.id);
            for (index, action) in actions.iter().enumerate() {
                let expanded;
                let rule = if plan.templated[index] {
                    expanded = plan.expanded(action, row, resolved);
                    if expanded.is_none() {
                        skipped += 1;
                    }
                    expanded.as_ref()
                } else {
                    plan.rule_actions[index].as_ref()
                };
                match (action, rule) {
                    (Action::SetFavorite { favorite: value }, _) => favorite = *value,
                    (Action::DeletePromptTags { field, value }, Some(rule)) => {
                        // Rejoining a prompt with no matching tag would only reformat it.
                        let targets = parse_prompt_tokens(value).into_iter().collect::<HashSet<_>>();
                        let current = field_value(&updated, field_name(*field)).unwrap_or_default();
                        if prompt_token_set(&current).iter().any(|token| targets.contains(token)) {
                            updated = simulate_actions(&updated, std::slice::from_ref(rule), &mut counters)
                                .map_err(s)?
                                .0;
                        }
                    }
                    (_, Some(rule)) => {
                        updated = simulate_actions(&updated, std::slice::from_ref(rule), &mut counters)
                            .map_err(s)?
                            .0;
                    }
                    (_, None) => {}
                }
            }
            updated.artists = settled_artists(row, &updated, resolved.llm_artists(row.id).is_some());
            let mut change = RowChange {
                id: row.id,
                text: Vec::new(),
                group: None,
                favorite: None,
                tags_added: Vec::new(),
                tags_removed: Vec::new(),
            };
            for field in ["positive", "character", "negative", "artists", "note"] {
                let (before, after) = (field_value(row, field), field_value(&updated, field));
                if before != after {
                    change.text.push(TextChange { field: field.into(), before, after });
                }
            }
            if row.group_id != updated.group_id {
                change.group = Some(GroupChange {
                    before: row.group_id,
                    after: updated.group_id,
                    before_name: row.group_id.and_then(|id| plan.group_names.get(&id).cloned()),
                    after_name: updated.group_id.and_then(|id| plan.group_names.get(&id).cloned()),
                });
            }
            let old_favorite = resolved.favorite(row.id);
            if favorite != old_favorite {
                change.favorite = Some([old_favorite, favorite]);
            }
            change.tags_added = sorted(updated.tags.difference(&row.tags));
            change.tags_removed = sorted(row.tags.difference(&updated.tags));
            if !change.text.is_empty()
                || change.group.is_some()
                || change.favorite.is_some()
                || !change.tags_added.is_empty()
                || !change.tags_removed.is_empty()
            {
                changes.push(change);
            }
        }
        Ok((changes, skipped))
    }

    /// TAGs the changes add that the library does not have yet.
    pub(crate) fn agent_missing_tags(&self, changes: &[RowChange]) -> Res<Vec<String>> {
        let existing = self
            .list_tags()
            .map_err(s)?
            .into_iter()
            .map(|tag| tag.name)
            .collect::<HashSet<_>>();
        let mut missing = Vec::new();
        for tag in changes.iter().flat_map(|change| &change.tags_added) {
            if !existing.contains(tag) && !missing.contains(tag) {
                missing.push(tag.clone());
            }
        }
        Ok(missing)
    }
}

fn sorted<'a>(values: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut values = values.cloned().collect::<Vec<_>>();
    values.sort();
    values
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewReport {
    pub preview_id: String,
    pub title: String,
    pub target: Value,
    pub target_notes: Vec<String>,
    pub actions: Vec<Action>,
    pub scanned_rows: usize,
    pub changed_rows: usize,
    pub unchanged_rows: usize,
    pub field_counts: BTreeMap<String, usize>,
    pub new_groups: Vec<String>,
    pub new_tags: Vec<String>,
    pub match_analysis: Vec<Value>,
    pub samples: Vec<Value>,
    pub warnings: Vec<String>,
    /// Rows that change in exactly the same way, largest first.
    #[serde(default)]
    pub change_groups: Vec<ChangeGroup>,
    /// Rows left out of `change_groups` once it reaches its size limit.
    #[serde(default)]
    pub other_rows: usize,
    /// Plain-language lines for changes that are not per-row edits (renaming a group, merging TAGs…).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summary: Vec<String>,
    /// Set on library-management previews: the operation that will run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<Value>,
}

const GROUP_LIMIT: usize = 40;
const GROUP_ROW_IDS: usize = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub field: String,
    /// Edited span by characters; used for notes, which are free text.
    pub removed: String,
    pub added: String,
    /// Prompt fields compare comma-separated tags, so reformatting alone does not split groups.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_tags: Vec<String>,
    /// Only separators or spacing change.
    #[serde(default)]
    pub format_only: bool,
    /// Excerpt of the first row around the edit; empty when the edit spans too much to excerpt.
    pub before: String,
    pub after: String,
    pub after_empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeGroup {
    pub count: usize,
    /// The first rows of the group, enough for thumbnails.
    pub row_ids: Vec<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text: Vec<TextEdit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<[Option<String>; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite: Option<[bool; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags_added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags_removed: Vec<String>,
}

/// The edited span of a before/after pair, by common prefix and suffix.
fn edited_span(before: &str, after: &str) -> (String, String) {
    let a = before.chars().collect::<Vec<_>>();
    let b = after.chars().collect::<Vec<_>>();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    (a[prefix..a.len() - suffix].iter().collect(), b[prefix..b.len() - suffix].iter().collect())
}

/// Tags of a prompt as (comparison key, display text). The key ignores case and weight syntax, so a
/// `2::` / `::` / `{` that only moves to a neighbouring tag is not reported as a tag change.
fn prompt_tags(text: &str) -> Vec<(String, String)> {
    text.split([',', '，', '\n', '\r'])
        .filter_map(|fragment| {
            let core = crate::automation::text::tag_core(fragment);
            let key = crate::pipeline::prompt_text::normalize_prompt_token(core);
            (!key.is_empty()).then(|| (key, core.to_owned()))
        })
        .collect()
}

/// Tags present in `a` more often than in `b`, in the order they appear in `a`.
fn tags_missing_from(a: &[(String, String)], b: &[(String, String)]) -> Vec<String> {
    let mut left = HashMap::<&str, usize>::new();
    for (key, _) in b {
        *left.entry(key).or_default() += 1;
    }
    a.iter()
        .filter(|(key, _)| match left.get_mut(key.as_str()) {
            Some(count) if *count > 0 => {
                *count -= 1;
                false
            }
            _ => true,
        })
        .map(|(_, display)| display.clone())
        .collect()
}

const EXCERPT_SPAN: usize = 160;

/// Bucket rows by what actually changes in them, so identical edits are shown once.
/// Returns the groups (largest first, at most `GROUP_LIMIT`) and every group's full row list in the same order.
pub(crate) fn group_changes(changes: &[RowChange]) -> Vec<(ChangeGroup, Vec<i64>)> {
    let mut order = Vec::<String>::new();
    let mut groups = HashMap::<String, (ChangeGroup, Vec<i64>)>::new();
    for change in changes {
        let text = change
            .text
            .iter()
            .filter(|item| item.field != "artists")
            .map(|item| {
                let before = item.before.as_deref().unwrap_or("");
                let after = item.after.as_deref().unwrap_or("");
                let (removed, added) = edited_span(before, after);
                let (before_excerpt, after_excerpt) = if removed.chars().count() + added.chars().count() <= EXCERPT_SPAN {
                    excerpt(before, after, 40)
                } else {
                    (String::new(), String::new())
                };
                let prompt = matches!(item.field.as_str(), "positive" | "character" | "negative");
                let (removed_tags, added_tags) = if prompt {
                    let (a, b) = (prompt_tags(before), prompt_tags(after));
                    // Sorted, so rows losing the same tags in a different order share a group.
                    let sorted = |mut tags: Vec<String>| {
                        tags.sort_by_key(|tag| tag.to_lowercase());
                        tags
                    };
                    (sorted(tags_missing_from(&a, &b)), sorted(tags_missing_from(&b, &a)))
                } else {
                    (Vec::new(), Vec::new())
                };
                let format_only = prompt && removed_tags.is_empty() && added_tags.is_empty();
                TextEdit {
                    field: item.field.clone(),
                    // Prompt groups are keyed by tags; the character span would split them by context.
                    removed: if prompt { String::new() } else { removed },
                    added: if prompt { String::new() } else { added },
                    removed_tags,
                    added_tags,
                    format_only,
                    before: before_excerpt,
                    after: after_excerpt,
                    after_empty: item.after.is_none(),
                }
            })
            .collect::<Vec<_>>();
        let group = change.group.as_ref().map(|group| [group.before_name.clone(), group.after_name.clone()]);
        let key = json!([
            text.iter().map(|edit| json!([edit.field, edit.removed, edit.added, edit.removed_tags, edit.added_tags])).collect::<Vec<_>>(),
            text.iter().map(|edit| edit.after_empty).collect::<Vec<_>>(),
            group,
            change.favorite,
            change.tags_added,
            change.tags_removed,
        ])
        .to_string();
        let entry = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key);
            (
                ChangeGroup {
                    count: 0,
                    row_ids: Vec::new(),
                    text,
                    group,
                    favorite: change.favorite,
                    tags_added: change.tags_added.clone(),
                    tags_removed: change.tags_removed.clone(),
                },
                Vec::new(),
            )
        });
        entry.0.count += 1;
        if entry.0.row_ids.len() < GROUP_ROW_IDS {
            entry.0.row_ids.push(change.id);
        }
        entry.1.push(change.id);
    }
    let mut list = order.into_iter().filter_map(|key| groups.remove(&key)).collect::<Vec<_>>();
    // Stable: equal sizes keep the order rows were found in.
    list.sort_by(|a, b| b.0.count.cmp(&a.0.count));
    list
}

/// Groups for a report, capped, with the number of rows that did not fit.
pub(crate) fn report_groups(changes: &[RowChange]) -> (Vec<ChangeGroup>, usize) {
    let mut groups = group_changes(changes).into_iter().map(|(group, _)| group).collect::<Vec<_>>();
    let other = groups.iter().skip(GROUP_LIMIT).map(|group| group.count).sum();
    groups.truncate(GROUP_LIMIT);
    (groups, other)
}

/// Shorten a before/after pair around the edited span so long prompts stay readable.
pub(crate) fn excerpt(before: &str, after: &str, context: usize) -> (String, String) {
    let a = before.chars().collect::<Vec<_>>();
    let b = after.chars().collect::<Vec<_>>();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let max_suffix = a.len().min(b.len()) - prefix;
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suffix)
        .take_while(|(x, y)| x == y)
        .count();
    let cut = |chars: &[char]| {
        let start = prefix.saturating_sub(context);
        let end = (chars.len() - suffix + context).min(chars.len());
        let mut text = String::new();
        if start > 0 {
            text.push('…');
        }
        text.extend(&chars[start..end]);
        if end < chars.len() {
            text.push('…');
        }
        text
    };
    (cut(&a), cut(&b))
}

pub(crate) fn context_of(text: &str, start: usize, end: usize, context: usize) -> String {
    let before = text[..start].chars().rev().take(context).collect::<Vec<_>>();
    let after = text[end..].chars().take(context).collect::<String>();
    let mut out = String::new();
    if before.len() == context {
        out.push('…');
    }
    out.extend(before.into_iter().rev());
    out.push('【');
    out.push_str(&text[start..end]);
    out.push('】');
    out.push_str(&after);
    if text[end..].chars().count() > context {
        out.push('…');
    }
    out
}

/// Describe what a pattern actually hits, so the agent can spot edge cases before writing.
pub(crate) fn analyze_matches(
    regex: &Regex,
    rows: &[RuleRow],
    field: &str,
) -> Value {
    let mut distinct: HashMap<String, (usize, i64, String)> = HashMap::new();
    let (mut rows_hit, mut occurrences, mut multi) = (0, 0, 0);
    let (mut at_start, mut at_end, mut middle, mut multiline) = (0, 0, 0, 0);
    let mut multi_samples = Vec::new();
    for row in rows {
        let Some(text) = field_value(row, field) else { continue };
        let found = regex.find_iter(&text).filter(|m| !m.as_str().is_empty()).collect::<Vec<_>>();
        if found.is_empty() {
            continue;
        }
        rows_hit += 1;
        occurrences += found.len();
        if found.len() > 1 {
            multi += 1;
            if multi_samples.len() < 3 {
                multi_samples.push(json!({"rowId": row.id, "count": found.len()}));
            }
        }
        let trimmed_start = text.len() - text.trim_start().len();
        let trimmed_end = text.trim_end().len();
        for m in &found {
            if m.start() <= trimmed_start {
                at_start += 1;
            } else if m.end() >= trimmed_end {
                at_end += 1;
            } else {
                middle += 1;
            }
            if m.as_str().contains('\n') {
                multiline += 1;
            }
            let key = m.as_str().chars().take(80).collect::<String>();
            let entry = distinct
                .entry(key)
                .or_insert_with(|| (0, row.id, context_of(&text, m.start(), m.end(), 30)));
            entry.0 += 1;
        }
    }
    let total_distinct = distinct.len();
    let mut distinct = distinct.into_iter().collect::<Vec<_>>();
    distinct.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(&b.0)));
    json!({
        "field": field,
        "rowsMatched": rows_hit,
        "occurrences": occurrences,
        "rowsWithMultipleMatches": multi,
        "multipleMatchSamples": multi_samples,
        "positions": {"atStart": at_start, "inMiddle": middle, "atEnd": at_end, "spanningLines": multiline},
        "distinctMatchCount": total_distinct,
        "distinctMatches": distinct.into_iter().take(25).map(|(text, (count, row_id, context))| json!({
            "text": text, "count": count, "sampleRowId": row_id, "context": context
        })).collect::<Vec<_>>(),
    })
}

/// Stored artist strings drive grouping, so a prompt edit must not regroup an image unless
/// it really changes the artist part. AI-extracted strings are never re-derived by rules.
fn settled_artists(before: &RuleRow, after: &RuleRow, ai_extracted: bool) -> Option<String> {
    let derive = |row: &RuleRow| {
        combined_artists(row.positive_prompt.as_deref().unwrap_or(""), row.character_prompt.as_deref())
    };
    if ai_extracted {
        return before.artists.clone();
    }
    let derived = derive(after);
    if derive(before) == derived { before.artists.clone() } else { derived }
}

/// Rows whose positive prompt loses part of their AI-extracted artist string. The AI string is often
/// reformatted rather than copied, so each artist counts only if it was in the prompt before the edit.
pub(crate) fn stale_ai_artists(resolved: &Resolved, changes: &[RowChange]) -> usize {
    changes
        .iter()
        .filter(|change| {
            let Some(artists) = resolved.llm_artists(change.id) else { return false };
            change.text.iter().find(|item| item.field == "positive").is_some_and(|item| {
                let (before, after) = (item.before.as_deref().unwrap_or(""), item.after.as_deref().unwrap_or(""));
                artists
                    .split([',', '，', '\n'])
                    .map(str::trim)
                    .filter(|artist| !artist.is_empty())
                    .any(|artist| before.contains(artist) && !after.contains(artist))
            })
        })
        .count()
}

pub(crate) fn action_regex(action: &Action) -> Option<(Res<Regex>, PromptActionField)> {
    match action {
        Action::ReplacePrompt { field, find, case_sensitive, .. } => {
            Some((build_regex(&regex::escape(find), *case_sensitive).map_err(s), *field))
        }
        Action::RegexReplacePrompt { field, pattern, case_sensitive, .. } => {
            Some((build_regex(pattern, *case_sensitive).map_err(s), *field))
        }
        _ => None,
    }
}

pub(crate) fn sample_json(change: &RowChange) -> Value {
    let text = change
        .text
        .iter()
        .filter(|item| item.field != "artists")
        .map(|item| {
            let (before, after) = excerpt(
                item.before.as_deref().unwrap_or(""),
                item.after.as_deref().unwrap_or(""),
                40,
            );
            json!({"field": item.field, "before": before, "after": after, "afterEmpty": item.after.is_none()})
        })
        .collect::<Vec<_>>();
    let mut value = json!({"rowId": change.id, "text": text});
    if let Some(group) = &change.group {
        value["group"] = json!({"before": group.before_name, "after": group.after_name});
    }
    if let Some(favorite) = change.favorite {
        value["favorite"] = json!(favorite);
    }
    if !change.tags_added.is_empty() {
        value["tagsAdded"] = json!(change.tags_added);
    }
    if !change.tags_removed.is_empty() {
        value["tagsRemoved"] = json!(change.tags_removed);
    }
    value
}
