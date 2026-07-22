use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::TransactionBehavior;
use serde::Serialize;

use super::Database;
use super::artist_dictionary::ArtistDictionaryEntry;
use super::prompt_edit::{
    combined_artists, normalized_bare_tag_in_fragment, prefix_known_artist_tags_in_prompt,
};
use super::quick_edit::{QuickArtistPrefixChange, QuickEditError};

const PREVIEW_SAMPLE_LIMIT: usize = 12;
const LOW_USAGE_POST_COUNT: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoArtistCandidate {
    pub match_name: String,
    pub display_name: String,
    pub canonical_name: String,
    pub post_count: u64,
    pub matched_rows: u64,
    pub matched_fields: u64,
    pub sample_row_ids: Vec<i64>,
    pub is_banned: bool,
    pub is_deprecated: bool,
    pub is_ambiguous: bool,
    pub is_low_usage: bool,
    pub is_short_name: bool,
    pub is_common_word: bool,
    pub needs_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoArtistPrefixPreview {
    pub scanned_rows: u64,
    pub matched_rows: u64,
    pub prompt_fields_needing_changes: u64,
    pub candidates: Vec<AutoArtistCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoArtistPrefixApplyResult {
    pub scanned_rows: u64,
    pub matched_rows: u64,
    pub changed_rows: u64,
    pub prompt_fields_changed: u64,
    pub changes: Vec<QuickArtistPrefixChange>,
}

#[derive(Debug)]
struct CandidateAccumulator {
    entry: ArtistDictionaryEntry,
    matched_rows: u64,
    matched_fields: u64,
    last_row_id: Option<i64>,
    sample_row_ids: Vec<i64>,
}

#[derive(Debug)]
struct PromptRow {
    id: i64,
    positive_prompt: Option<String>,
    character_prompt: Option<String>,
    negative_prompt: Option<String>,
    artists: Option<String>,
}

impl Database {
    pub fn preview_auto_artist_prefix(&self) -> Result<AutoArtistPrefixPreview, QuickEditError> {
        if self.artist_dictionary_status()?.is_none() {
            return Err(QuickEditError::ArtistDictionaryUnavailable);
        }
        let rows = {
            let mut statement = self.connection.prepare(
                "SELECT id, positive_prompt, character_prompt, negative_prompt, artists
                 FROM rows ORDER BY id",
            )?;
            statement
                .query_map([], read_prompt_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        let scanned_rows = u64::try_from(rows.len())
            .map_err(|_| QuickEditError::Database(super::DatabaseError::RowCountOverflow))?;
        let prompt_names = rows
            .iter()
            .flat_map(|row| {
                [
                    row.positive_prompt.as_deref(),
                    row.character_prompt.as_deref(),
                    row.negative_prompt.as_deref(),
                ]
                .into_iter()
                .flatten()
            })
            .flat_map(bare_tag_names)
            .collect::<BTreeSet<_>>();
        let dictionary = self
            .artist_dictionary_entries_by_names(prompt_names.iter().map(String::as_str))?
            .into_iter()
            .map(|entry| (entry.match_name.clone(), entry))
            .collect::<HashMap<_, _>>();
        let mut candidates = HashMap::<String, CandidateAccumulator>::new();
        let mut matched_rows = 0_u64;
        let mut prompt_fields_needing_changes = 0_u64;

        for row in rows {
            let mut row_matches = HashSet::new();
            for prompt in [
                row.positive_prompt.as_deref(),
                row.character_prompt.as_deref(),
                row.negative_prompt.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                let field_matches = matching_dictionary_names(prompt, &dictionary);
                if field_matches.is_empty() {
                    continue;
                }
                prompt_fields_needing_changes += 1;
                for match_name in field_matches {
                    row_matches.insert(match_name.clone());
                    let accumulator = candidates.entry(match_name.clone()).or_insert_with(|| {
                        CandidateAccumulator {
                            entry: dictionary
                                .get(&match_name)
                                .expect("match came from dictionary")
                                .clone(),
                            matched_rows: 0,
                            matched_fields: 0,
                            last_row_id: None,
                            sample_row_ids: Vec::new(),
                        }
                    });
                    accumulator.matched_fields += 1;
                    if accumulator.last_row_id != Some(row.id) {
                        accumulator.matched_rows += 1;
                        accumulator.last_row_id = Some(row.id);
                        if accumulator.sample_row_ids.len() < PREVIEW_SAMPLE_LIMIT {
                            accumulator.sample_row_ids.push(row.id);
                        }
                    }
                }
            }
            if !row_matches.is_empty() {
                matched_rows += 1;
            }
        }

        let mut candidates = candidates
            .into_values()
            .map(candidate_from_accumulator)
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .matched_rows
                .cmp(&left.matched_rows)
                .then_with(|| left.match_name.cmp(&right.match_name))
        });
        Ok(AutoArtistPrefixPreview {
            scanned_rows,
            matched_rows,
            prompt_fields_needing_changes,
            candidates,
        })
    }

    pub fn apply_auto_artist_prefix(
        &mut self,
        selected_names: &[String],
    ) -> Result<AutoArtistPrefixApplyResult, QuickEditError> {
        let selected_names = selected_names
            .iter()
            .filter_map(|name| normalize_selected_name(name))
            .collect::<HashSet<_>>();
        if selected_names.is_empty() {
            return Err(QuickEditError::EmptyArtistSelection);
        }
        let known_names = self
            .artist_dictionary_entries_by_names(selected_names.iter().map(String::as_str))?
            .into_iter()
            .map(|entry| entry.match_name)
            .collect::<HashSet<_>>();
        let mut unknown_names = selected_names
            .difference(&known_names)
            .cloned()
            .collect::<Vec<_>>();
        if !unknown_names.is_empty() {
            unknown_names.sort();
            return Err(QuickEditError::UnknownArtistNames(unknown_names));
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rows = {
            let mut statement = transaction.prepare(
                "SELECT id, positive_prompt, character_prompt, negative_prompt, artists
                 FROM rows ORDER BY id",
            )?;
            statement
                .query_map([], read_prompt_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        let scanned_rows = u64::try_from(rows.len())
            .map_err(|_| QuickEditError::Database(super::DatabaseError::RowCountOverflow))?;
        let mut changes = Vec::new();
        for row in rows {
            if let Some(change) = automatic_change(row, &selected_names) {
                changes.push(change);
            }
        }
        {
            let mut update = transaction.prepare(
                "UPDATE rows
                 SET positive_prompt = ?2,
                     character_prompt = ?3,
                     negative_prompt = ?4,
                     artists = ?5
                 WHERE id = ?1",
            )?;
            for change in &changes {
                update.execute(rusqlite::params![
                    change.row_id,
                    change.new_positive_prompt,
                    change.new_character_prompt,
                    change.new_negative_prompt,
                    change.new_artists,
                ])?;
            }
        }
        transaction.commit()?;

        let prompt_fields_changed = changes.iter().map(changed_prompt_field_count).sum::<u64>();
        let changed_rows = u64::try_from(changes.len())
            .map_err(|_| QuickEditError::Database(super::DatabaseError::RowCountOverflow))?;
        Ok(AutoArtistPrefixApplyResult {
            scanned_rows,
            matched_rows: changed_rows,
            changed_rows,
            prompt_fields_changed,
            changes,
        })
    }
}

fn read_prompt_row(row: &rusqlite::Row<'_>) -> Result<PromptRow, rusqlite::Error> {
    Ok(PromptRow {
        id: row.get(0)?,
        positive_prompt: row.get(1)?,
        character_prompt: row.get(2)?,
        negative_prompt: row.get(3)?,
        artists: row.get(4)?,
    })
}

fn matching_dictionary_names(
    prompt: &str,
    dictionary: &HashMap<String, ArtistDictionaryEntry>,
) -> BTreeSet<String> {
    bare_tag_names(prompt)
        .into_iter()
        .filter(|name| dictionary.contains_key(name))
        .collect()
}

fn bare_tag_names(prompt: &str) -> BTreeSet<String> {
    prompt
        .split([',', '\n', '\r'])
        .filter_map(normalized_bare_tag_in_fragment)
        .collect()
}

fn automatic_change(
    row: PromptRow,
    selected_names: &HashSet<String>,
) -> Option<QuickArtistPrefixChange> {
    let positive_rewrite = row
        .positive_prompt
        .as_deref()
        .and_then(|prompt| prefix_known_artist_tags_in_prompt(prompt, selected_names))
        .map(|(prompt, _)| prompt);
    let character_rewrite = row
        .character_prompt
        .as_deref()
        .and_then(|prompt| prefix_known_artist_tags_in_prompt(prompt, selected_names))
        .map(|(prompt, _)| prompt);
    let negative_rewrite = row
        .negative_prompt
        .as_deref()
        .and_then(|prompt| prefix_known_artist_tags_in_prompt(prompt, selected_names))
        .map(|(prompt, _)| prompt);
    let artist_source_changed = positive_rewrite.is_some() || character_rewrite.is_some();
    if !artist_source_changed && negative_rewrite.is_none() {
        return None;
    }

    let new_positive_prompt = positive_rewrite.or_else(|| row.positive_prompt.clone());
    let new_character_prompt = character_rewrite.or_else(|| row.character_prompt.clone());
    let new_negative_prompt = negative_rewrite.or_else(|| row.negative_prompt.clone());
    let new_artists = if artist_source_changed {
        combined_artists(
            new_positive_prompt.as_deref().unwrap_or(""),
            new_character_prompt.as_deref(),
        )
    } else {
        row.artists.clone()
    };

    Some(QuickArtistPrefixChange {
        row_id: row.id,
        previous_positive_prompt: row.positive_prompt,
        new_positive_prompt,
        previous_character_prompt: row.character_prompt,
        new_character_prompt,
        previous_negative_prompt: row.negative_prompt,
        new_negative_prompt,
        previous_artists: row.artists,
        new_artists,
    })
}

fn changed_prompt_field_count(change: &QuickArtistPrefixChange) -> u64 {
    u64::from(change.previous_positive_prompt != change.new_positive_prompt)
        + u64::from(change.previous_character_prompt != change.new_character_prompt)
        + u64::from(change.previous_negative_prompt != change.new_negative_prompt)
}

fn candidate_from_accumulator(accumulator: CandidateAccumulator) -> AutoArtistCandidate {
    let is_low_usage = accumulator.entry.post_count < LOW_USAGE_POST_COUNT;
    let is_short_name = significant_character_count(&accumulator.entry.match_name) <= 3;
    let is_common_word = is_common_prompt_word(&accumulator.entry.match_name);
    let needs_confirmation = accumulator.entry.is_ambiguous || is_short_name || is_common_word;
    AutoArtistCandidate {
        match_name: accumulator.entry.match_name,
        display_name: accumulator.entry.display_name,
        canonical_name: accumulator.entry.canonical_name,
        post_count: accumulator.entry.post_count,
        matched_rows: accumulator.matched_rows,
        matched_fields: accumulator.matched_fields,
        sample_row_ids: accumulator.sample_row_ids,
        is_banned: accumulator.entry.is_banned,
        is_deprecated: accumulator.entry.is_deprecated,
        is_ambiguous: accumulator.entry.is_ambiguous,
        is_low_usage,
        is_short_name,
        is_common_word,
        needs_confirmation,
    }
}

fn normalize_selected_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_lowercase())
}

fn significant_character_count(name: &str) -> usize {
    name.chars()
        .filter(|character| character.is_alphanumeric())
        .count()
}

fn is_common_prompt_word(name: &str) -> bool {
    matches!(
        name.replace('_', " ").as_str(),
        "art"
            | "black"
            | "blue"
            | "boy"
            | "cloud"
            | "dark"
            | "fire"
            | "flower"
            | "girl"
            | "green"
            | "light"
            | "line"
            | "moon"
            | "red"
            | "sky"
            | "snow"
            | "star"
            | "style"
            | "water"
            | "white"
            | "wind"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::append_rows;
    use crate::db::{ArtistDictionaryInput, DanbooruArtistRecord, DanbooruArtistTag, NewRow};

    fn artist_tag(id: i64, name: &str, post_count: u64) -> DanbooruArtistTag {
        DanbooruArtistTag {
            id,
            name: name.into(),
            post_count,
            category: 1,
            is_deprecated: false,
        }
    }

    #[test]
    fn preview_groups_historical_name_with_current_takedown_artist() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .replace_artist_dictionary(
                &ArtistDictionaryInput {
                    tags: vec![
                        artist_tag(1, "parsley-f", 891),
                        artist_tag(2, "parsley_f", 0),
                        artist_tag(3, "red", 1),
                    ],
                    artists: vec![DanbooruArtistRecord {
                        id: 27_785,
                        name: "parsley-f".into(),
                        other_names: vec!["parsley_f".into()],
                        is_deleted: false,
                        is_banned: true,
                    }],
                    aliases: Vec::new(),
                },
                "2026-07-22T12:00:00Z",
            )
            .unwrap();
        append_rows(
            &mut database,
            &[
                NewRow {
                    source_ordinal: 1,
                    identity: "first".into(),
                    positive_prompt: Some("best quality, 0.7::parsley_f::, red".into()),
                    character_prompt: Some("(parsley_f:1.2)".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 2,
                    identity: "second".into(),
                    positive_prompt: Some("artist:parsley_f, parsley_fx".into()),
                    ..NewRow::default()
                },
            ],
        );

        let preview = database.preview_auto_artist_prefix().unwrap();
        assert_eq!(preview.scanned_rows, 2);
        assert_eq!(preview.matched_rows, 1);
        assert_eq!(preview.prompt_fields_needing_changes, 2);
        let parsley = preview
            .candidates
            .iter()
            .find(|candidate| candidate.match_name == "parsley_f")
            .unwrap();
        assert_eq!(parsley.canonical_name, "parsley-f");
        assert_eq!(parsley.post_count, 891);
        assert!(parsley.is_banned);
        assert!(!parsley.is_low_usage);
        assert!(!parsley.needs_confirmation);
        let red = preview
            .candidates
            .iter()
            .find(|candidate| candidate.match_name == "red")
            .unwrap();
        assert!(red.is_low_usage);
        assert!(red.is_short_name);
        assert!(red.is_common_word);
        assert!(red.needs_confirmation);
    }

    #[test]
    fn apply_updates_all_prompt_fields_and_reuses_existing_undo() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .replace_artist_dictionary(
                &ArtistDictionaryInput {
                    tags: vec![artist_tag(1, "parsley_f", 8)],
                    ..ArtistDictionaryInput::default()
                },
                "2026-07-22T12:00:00Z",
            )
            .unwrap();
        append_rows(
            &mut database,
            &[NewRow {
                source_ordinal: 1,
                identity: "first".into(),
                positive_prompt: Some("parsley_f".into()),
                character_prompt: Some("{parsley_f}".into()),
                negative_prompt: Some("0.5::parsley_f::".into()),
                ..NewRow::default()
            }],
        );

        let applied = database
            .apply_auto_artist_prefix(&["parsley_f".into()])
            .unwrap();
        assert_eq!(applied.changed_rows, 1);
        assert_eq!(applied.prompt_fields_changed, 3);
        let row: (String, String, String, String) = database
            .connection
            .query_row(
                "SELECT positive_prompt, character_prompt, negative_prompt, artists
                 FROM rows WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "artist:parsley_f".into(),
                "{artist:parsley_f}".into(),
                "0.5::artist:parsley_f::".into(),
                "artist:parsley_f\n{artist:parsley_f}".into(),
            )
        );

        database
            .revert_quick_artist_prefix_changes(&applied.changes)
            .unwrap();
        let reverted: String = database
            .connection
            .query_row("SELECT positive_prompt FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(reverted, "parsley_f");
    }
}
