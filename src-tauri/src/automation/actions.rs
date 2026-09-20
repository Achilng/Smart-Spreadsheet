use crate::automation::error::AutomationRuleError;
use crate::automation::model::{PromptActionField, RuleAction, RuleRow};
use crate::automation::text::{
    append_prompt, delete_prompt_tags, nonempty, normalized_strings, replace_text,
};
use crate::pipeline::prompt_text::{combined_artists, prefix_artist_tag_in_prompt};
use std::collections::{HashMap, HashSet};

pub(crate) fn simulate_actions(
    original: &RuleRow,
    actions: &[RuleAction],
    sequence_counters: &mut HashMap<String, u64>,
) -> Result<(RuleRow, u64, bool), AutomationRuleError> {
    let mut row = original.clone();
    let mut changed_actions = 0_u64;
    let mut stop = false;
    for action in actions {
        let changed = match action {
            RuleAction::AddTags { tags } => normalized_strings(tags)
                .into_iter()
                .fold(false, |changed, tag| row.tags.insert(tag) || changed),
            RuleAction::RemoveTags { tags } => normalized_strings(tags)
                .into_iter()
                .fold(false, |changed, tag| row.tags.remove(&tag) || changed),
            RuleAction::SetGroup {
                group_id,
                only_if_ungrouped,
            } => {
                if *only_if_ungrouped && row.group_id.is_some() {
                    false
                } else if row.group_id != Some(*group_id) {
                    row.group_id = Some(*group_id);
                    true
                } else {
                    false
                }
            }
            RuleAction::ClearGroup => row.group_id.take().is_some(),
            RuleAction::AppendPrompt { field, value } => {
                let prompt = prompt_mut(&mut row, *field);
                let updated = append_prompt(prompt.as_deref(), value);
                if *prompt != updated {
                    *prompt = updated;
                    refresh_artists(&mut row);
                    true
                } else {
                    false
                }
            }
            RuleAction::DeletePromptTags { field, value } => {
                let prompt = prompt_mut(&mut row, *field);
                let updated = delete_prompt_tags(prompt.as_deref(), value);
                if *prompt != updated {
                    *prompt = updated;
                    refresh_artists(&mut row);
                    true
                } else {
                    false
                }
            }
            RuleAction::ReplacePrompt {
                field,
                find,
                replace,
                case_sensitive,
            } => {
                let prompt = prompt_mut(&mut row, *field);
                let updated = prompt
                    .as_deref()
                    .map(|value| replace_text(value, find, replace, *case_sensitive));
                if *prompt != updated {
                    *prompt = updated.and_then(nonempty);
                    refresh_artists(&mut row);
                    true
                } else {
                    false
                }
            }
            RuleAction::PrefixArtist { artists } => {
                let mut changed = false;
                for artist in normalized_strings(artists) {
                    for field in [
                        PromptActionField::Positive,
                        PromptActionField::Character,
                        PromptActionField::Negative,
                    ] {
                        let prompt = prompt_mut(&mut row, field);
                        if let Some(updated) = prompt
                            .as_deref()
                            .and_then(|prompt| prefix_artist_tag_in_prompt(prompt, &artist))
                        {
                            *prompt = Some(updated);
                            changed = true;
                        }
                    }
                }
                if changed {
                    refresh_artists(&mut row);
                }
                changed
            }
            RuleAction::SetNote { value } => {
                let updated = nonempty(value.clone());
                if row.note != updated {
                    row.note = updated;
                    true
                } else {
                    false
                }
            }
            RuleAction::SetNoteSequence { prefix } => {
                apply_note_sequence(&mut row, prefix, sequence_counters)
            }
            RuleAction::AppendNote { value, separator } => {
                let updated = match row.note.as_deref().filter(|note| !note.is_empty()) {
                    Some(note) => format!("{note}{separator}{value}"),
                    None => value.clone(),
                };
                if row.note.as_deref() != Some(updated.as_str()) {
                    row.note = nonempty(updated);
                    true
                } else {
                    false
                }
            }
            RuleAction::ClearNote => row.note.take().is_some(),
            RuleAction::StopProcessing => {
                stop = true;
                false
            }
        };
        if changed {
            changed_actions += 1;
        }
    }
    Ok((row, changed_actions, stop))
}

pub(crate) fn prompt_mut(row: &mut RuleRow, field: PromptActionField) -> &mut Option<String> {
    match field {
        PromptActionField::Positive => &mut row.positive_prompt,
        PromptActionField::Character => &mut row.character_prompt,
        PromptActionField::Negative => &mut row.negative_prompt,
    }
}

pub(crate) fn refresh_artists(row: &mut RuleRow) {
    row.artists = combined_artists(
        row.positive_prompt.as_deref().unwrap_or(""),
        row.character_prompt.as_deref(),
    );
}

pub(crate) fn note_sequence_prefixes(actions: &[RuleAction]) -> Vec<String> {
    let mut prefixes = Vec::new();
    let mut seen = HashSet::new();
    for action in actions {
        if let RuleAction::SetNoteSequence { prefix } = action {
            let prefix = prefix.trim().to_owned();
            if !prefix.is_empty() && seen.insert(prefix.clone()) {
                prefixes.push(prefix);
            }
        }
    }
    prefixes
}

pub(crate) fn apply_note_sequence(
    row: &mut RuleRow,
    prefix: &str,
    sequence_counters: &mut HashMap<String, u64>,
) -> bool {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return false;
    }
    if parse_note_sequence_number(row.note.as_deref().unwrap_or(""), prefix).is_some() {
        return false;
    }
    let next = sequence_counters
        .get(prefix)
        .copied()
        .unwrap_or(0)
        .saturating_add(1);
    sequence_counters.insert(prefix.to_owned(), next);
    row.note = Some(format!("{prefix}{next}"));
    true
}

pub(crate) fn parse_note_sequence_number(note: &str, prefix: &str) -> Option<u64> {
    let trimmed = note.trim();
    let remainder = trimmed.strip_prefix(prefix)?;
    if remainder.is_empty() || !remainder.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    remainder.parse().ok()
}
