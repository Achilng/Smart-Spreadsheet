use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::TransactionBehavior;
use serde::Serialize;

use super::Database;
use super::prompt_edit::{
    combined_artists, normalized_bare_tag_in_fragment, normalized_explicit_artist_tag_in_fragment,
    prefix_known_artist_tags_in_prompt,
};
use super::quick_edit::{QuickArtistPrefixChange, QuickEditError};

const PREVIEW_SAMPLE_LIMIT: usize = 12;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoArtistCandidate {
    pub match_name: String,
    pub display_name: String,
    pub matched_rows: u64,
    pub matched_fields: u64,
    pub sample_row_ids: Vec<i64>,
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
    match_name: String,
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
        let rows = {
            let mut statement = self.connection.prepare(
                "SELECT id, positive_prompt, character_prompt, negative_prompt, artists
                 FROM rows ORDER BY id",
            )?;
            statement
                .query_map([], read_prompt_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        let library_confirmed_names = library_confirmed_names(&rows);
        let scanned_rows = u64::try_from(rows.len())
            .map_err(|_| QuickEditError::Database(super::DatabaseError::RowCountOverflow))?;
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
                let field_matches = matching_library_artist_names(prompt, &library_confirmed_names);
                if field_matches.is_empty() {
                    continue;
                }
                prompt_fields_needing_changes += 1;
                for match_name in field_matches {
                    row_matches.insert(match_name.clone());
                    let accumulator = candidates.entry(match_name.clone()).or_insert_with(|| {
                        CandidateAccumulator {
                            match_name,
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
        let library_rows = {
            let mut statement = self.connection.prepare(
                "SELECT id, positive_prompt, character_prompt, negative_prompt, artists
                 FROM rows ORDER BY id",
            )?;
            statement
                .query_map([], read_prompt_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        let known_names = library_confirmed_names(&library_rows);
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

    /// 只处理指定行，并把资料库任意位置已经明确写成 `artist:名称` 的 Tag
    /// 作为唯一证据。用于导入完成后的自动整理，不会触碰既有旧行。
    pub fn apply_confirmed_artist_prefix_to_rows(
        &mut self,
        row_ids: &[i64],
    ) -> Result<AutoArtistPrefixApplyResult, QuickEditError> {
        let target_ids = row_ids
            .iter()
            .copied()
            .filter(|row_id| *row_id > 0)
            .collect::<HashSet<_>>();
        if target_ids.is_empty() {
            return Ok(AutoArtistPrefixApplyResult {
                scanned_rows: 0,
                matched_rows: 0,
                changed_rows: 0,
                prompt_fields_changed: 0,
                changes: Vec::new(),
            });
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let library_rows = {
            let mut statement = transaction.prepare(
                "SELECT id, positive_prompt, character_prompt, negative_prompt, artists
                 FROM rows ORDER BY id",
            )?;
            statement
                .query_map([], read_prompt_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        let known_names = library_confirmed_names(&library_rows);
        let target_rows = library_rows
            .into_iter()
            .filter(|row| target_ids.contains(&row.id))
            .collect::<Vec<_>>();
        let scanned_rows = u64::try_from(target_rows.len())
            .map_err(|_| QuickEditError::Database(super::DatabaseError::RowCountOverflow))?;
        let changes = target_rows
            .into_iter()
            .filter_map(|row| automatic_change(row, &known_names))
            .collect::<Vec<_>>();

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

fn matching_library_artist_names(
    prompt: &str,
    library_confirmed_names: &HashSet<String>,
) -> BTreeSet<String> {
    bare_tag_names(prompt)
        .into_iter()
        .filter(|name| library_confirmed_names.contains(name))
        .collect()
}

fn bare_tag_names(prompt: &str) -> BTreeSet<String> {
    prompt
        .split([',', '\n', '\r'])
        .filter_map(normalized_bare_tag_in_fragment)
        .collect()
}

fn explicit_artist_names(prompt: &str) -> BTreeSet<String> {
    prompt
        .split([',', '\n', '\r'])
        .filter_map(normalized_explicit_artist_tag_in_fragment)
        .collect()
}

fn library_confirmed_names(rows: &[PromptRow]) -> HashSet<String> {
    rows.iter()
        .flat_map(|row| {
            [
                row.positive_prompt.as_deref(),
                row.character_prompt.as_deref(),
                row.negative_prompt.as_deref(),
            ]
            .into_iter()
            .flatten()
        })
        .flat_map(explicit_artist_names)
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
    let display_name = accumulator.match_name.clone();
    AutoArtistCandidate {
        match_name: accumulator.match_name,
        display_name,
        matched_rows: accumulator.matched_rows,
        matched_fields: accumulator.matched_fields,
        sample_row_ids: accumulator.sample_row_ids,
    }
}

fn normalize_selected_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::append_rows;
    use crate::db::NewRow;

    #[test]
    fn apply_updates_all_prompt_fields_and_reuses_existing_undo() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[
                NewRow {
                    source_ordinal: 1,
                    identity: "evidence".into(),
                    positive_prompt: Some("artist:parsley_f".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 2,
                    identity: "target".into(),
                    positive_prompt: Some("parsley_f".into()),
                    character_prompt: Some("{parsley_f}".into()),
                    negative_prompt: Some("0.5::parsley_f::".into()),
                    ..NewRow::default()
                },
            ],
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
                 FROM rows WHERE id = 2",
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
            .query_row("SELECT positive_prompt FROM rows WHERE id = 2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(reverted, "parsley_f");
    }

    #[test]
    fn library_explicit_artist_confirms_and_applies_unknown_name() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[
                NewRow {
                    source_ordinal: 1,
                    identity: "explicit".into(),
                    positive_prompt: Some("artist:xy".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 2,
                    identity: "positive".into(),
                    positive_prompt: Some("xy".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 3,
                    identity: "character".into(),
                    character_prompt: Some("{XY}".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 4,
                    identity: "negative".into(),
                    negative_prompt: Some("0.5::xy::".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 5,
                    identity: "similar".into(),
                    positive_prompt: Some("xyz".into()),
                    ..NewRow::default()
                },
            ],
        );

        let preview = database.preview_auto_artist_prefix().unwrap();
        assert_eq!(preview.candidates.len(), 1);
        let candidate = &preview.candidates[0];
        assert_eq!(candidate.match_name, "xy");
        assert_eq!(candidate.matched_rows, 3);

        let applied = database.apply_auto_artist_prefix(&["xy".into()]).unwrap();
        assert_eq!(applied.changed_rows, 3);
        let mut statement = database
            .connection
            .prepare(
                "SELECT positive_prompt, character_prompt, negative_prompt
                 FROM rows WHERE id BETWEEN 2 AND 4 ORDER BY id",
            )
            .unwrap();
        let prompts = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(prompts[0].0.as_deref(), Some("artist:xy"));
        assert_eq!(prompts[1].1.as_deref(), Some("{artist:XY}"));
        assert_eq!(prompts[2].2.as_deref(), Some("0.5::artist:xy::"));
    }

    #[test]
    fn bare_tags_without_library_evidence_are_ignored() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[NewRow {
                source_ordinal: 1,
                identity: "bare".into(),
                positive_prompt: Some("watermark, rare_artist_name".into()),
                ..NewRow::default()
            }],
        );

        let preview = database.preview_auto_artist_prefix().unwrap();
        assert_eq!(preview.scanned_rows, 1);
        assert_eq!(preview.matched_rows, 0);
        assert!(preview.candidates.is_empty());
    }

    #[test]
    fn import_style_apply_only_changes_requested_new_rows() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[
                NewRow {
                    source_ordinal: 1,
                    identity: "evidence".into(),
                    positive_prompt: Some("artist:xy".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 2,
                    identity: "old-bare".into(),
                    positive_prompt: Some("xy".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 3,
                    identity: "new-bare".into(),
                    positive_prompt: Some("xy".into()),
                    ..NewRow::default()
                },
            ],
        );

        let applied = database
            .apply_confirmed_artist_prefix_to_rows(&[3])
            .unwrap();

        assert_eq!(applied.scanned_rows, 1);
        assert_eq!(applied.changed_rows, 1);
        let prompts = database
            .connection
            .prepare("SELECT positive_prompt FROM rows WHERE id IN (2, 3) ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(prompts, vec!["xy", "artist:xy"]);
    }
}
