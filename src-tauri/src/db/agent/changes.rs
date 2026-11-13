//! Agent actions, simulation into per-row changes and the dry-run report.
use std::collections::{BTreeMap, HashMap, HashSet};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::target::Resolved;
use super::{Res, s};
use crate::automation::actions::{note_sequence_prefixes, simulate_actions};
use crate::automation::matching::build_regex;
use crate::automation::model::{PromptActionField, RuleAction, RuleRow, default_note_separator};
use crate::automation::text::{normalized_strings, parse_prompt_tokens, prompt_token_set};
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
    pub(crate) rule_actions: Vec<Option<RuleAction>>,
    pub(crate) new_groups: Vec<String>,
    pub(crate) group_names: HashMap<i64, String>,
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
    pub(crate) fn agent_plan(&self, actions: &[Action]) -> Res<Plan> {
        if actions.is_empty() {
            return Err("至少需要一个修改动作".into());
        }
        let mut group_names = HashMap::new();
        let mut name_to_id = HashMap::new();
        for group in self.list_groups().map_err(s)? {
            group_names.insert(group.id, group.name.clone());
            name_to_id.insert(group.name, group.id);
        }
        let mut new_groups: Vec<String> = Vec::new();
        let mut rule_actions = Vec::new();
        for action in actions {
            let rule = match action {
                Action::AddTags { tags } => Some(RuleAction::AddTags { tags: tags.clone() }),
                Action::RemoveTags { tags } => Some(RuleAction::RemoveTags { tags: tags.clone() }),
                Action::SetGroup { group_name, only_if_ungrouped } => {
                    let name = group_name.trim();
                    if name.is_empty() {
                        return Err("分组名称不能为空".into());
                    }
                    let id = match name_to_id.get(name) {
                        Some(id) => *id,
                        None => {
                            let position = match new_groups.iter().position(|value| value == name) {
                                Some(position) => position,
                                None => {
                                    new_groups.push(name.to_owned());
                                    new_groups.len() - 1
                                }
                            };
                            let placeholder = -1 - position as i64;
                            group_names.insert(placeholder, name.to_owned());
                            placeholder
                        }
                    };
                    Some(RuleAction::SetGroup { group_id: id, only_if_ungrouped: *only_if_ungrouped })
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
                Action::SetFavorite { .. } => None,
            };
            if let Some(rule) = &rule {
                match rule {
                    // Placeholder IDs are negative; validation only rejects them for saved rules.
                    RuleAction::SetGroup { .. } => {}
                    other => validate_action(other).map_err(s)?,
                }
            }
            rule_actions.push(rule);
        }
        Ok(Plan { rule_actions, new_groups, group_names })
    }

    pub(crate) fn agent_simulate(
        &self,
        resolved: &Resolved,
        actions: &[Action],
        plan: &Plan,
    ) -> Res<Vec<RowChange>> {
        let rule_list = plan.rule_actions.iter().flatten().cloned().collect::<Vec<_>>();
        let mut counters =
            seed_note_sequence_counters(&self.connection, &note_sequence_prefixes(&rule_list))
                .map_err(s)?;
        let mut changes = Vec::new();
        for row in &resolved.rows {
            let mut updated = row.clone();
            let mut favorite = resolved.favorite(row.id);
            for (action, rule) in actions.iter().zip(&plan.rule_actions) {
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
        Ok(changes)
    }

    pub(crate) fn agent_missing_tags(&self, actions: &[Action]) -> Res<Vec<String>> {
        let existing = self
            .list_tags()
            .map_err(s)?
            .into_iter()
            .map(|tag| tag.name)
            .collect::<HashSet<_>>();
        let mut missing = Vec::new();
        for action in actions {
            if let Action::AddTags { tags } = action {
                for tag in normalized_strings(tags) {
                    if !existing.contains(&tag) && !missing.contains(&tag) {
                        missing.push(tag);
                    }
                }
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

/// Rows whose AI-extracted artist string no longer appears in the edited positive prompt.
pub(crate) fn stale_ai_artists(resolved: &Resolved, changes: &[RowChange]) -> usize {
    changes
        .iter()
        .filter(|change| {
            let Some(artists) = resolved.llm_artists(change.id) else { return false };
            change
                .text
                .iter()
                .find(|item| item.field == "positive")
                .is_some_and(|item| !item.after.as_deref().unwrap_or("").contains(artists))
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
