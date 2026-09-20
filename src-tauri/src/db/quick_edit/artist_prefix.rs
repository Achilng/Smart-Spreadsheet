use super::model::{
    QuickArtistPrefixApplyResult, QuickArtistPrefixChange, QuickArtistPrefixPreview, QuickEditError,
};
use super::{PREVIEW_SAMPLE_LIMIT, row_count};
use crate::db::{Database, DatabaseError};
use crate::pipeline::prompt_text::{
    combined_artists, normalize_artist_name, prefix_artist_tag_in_prompt,
};
use rusqlite::{Connection, TransactionBehavior, params};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArtistPromptRow {
    id: i64,
    positive_prompt: Option<String>,
    character_prompt: Option<String>,
    negative_prompt: Option<String>,
    artists: Option<String>,
}

impl Database {
    pub fn preview_quick_artist_prefix(
        &self,
        artist_name: &str,
    ) -> Result<QuickArtistPrefixPreview, QuickEditError> {
        let artist_name = validated_artist_name(artist_name)?;
        let scanned_rows = row_count(&self.connection)?;
        let changes = artist_prefix_changes(&self.connection, &artist_name)?;
        let changed_rows =
            u64::try_from(changes.len()).map_err(|_| DatabaseError::CountOverflow)?;
        let prompt_fields_needing_changes =
            changes.iter().map(changed_prompt_field_count).sum::<u64>();

        Ok(QuickArtistPrefixPreview {
            scanned_rows,
            matched_rows: changed_rows,
            rows_needing_changes: changed_rows,
            prompt_fields_needing_changes,
            sample_row_ids: changes
                .iter()
                .map(|change| change.row_id)
                .take(PREVIEW_SAMPLE_LIMIT)
                .collect(),
            artist_name,
        })
    }

    pub fn apply_quick_artist_prefix(
        &mut self,
        artist_name: &str,
    ) -> Result<QuickArtistPrefixApplyResult, QuickEditError> {
        let artist_name = validated_artist_name(artist_name)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let scanned_rows = row_count(&transaction)?;
        let changes = artist_prefix_changes(&transaction, &artist_name)?;
        let prompt_fields_changed = changes.iter().map(changed_prompt_field_count).sum::<u64>();

        let mut update = transaction.prepare(
            "UPDATE rows
             SET positive_prompt = ?2,
                 character_prompt = ?3,
                 negative_prompt = ?4,
                 artists = ?5,
                 style_signature = ?6
             WHERE id = ?1",
        )?;
        for change in &changes {
            update.execute(params![
                change.row_id,
                &change.new_positive_prompt,
                &change.new_character_prompt,
                &change.new_negative_prompt,
                &change.new_artists,
                crate::pipeline::style_signature_of(change.new_positive_prompt.as_deref()),
            ])?;
        }
        drop(update);
        transaction.commit()?;
        self.bump_data_version();

        let changed_rows =
            u64::try_from(changes.len()).map_err(|_| DatabaseError::CountOverflow)?;
        Ok(QuickArtistPrefixApplyResult {
            scanned_rows,
            matched_rows: changed_rows,
            changed_rows,
            prompt_fields_changed,
            changes,
        })
    }

    pub fn revert_quick_artist_prefix_changes(
        &mut self,
        changes: &[QuickArtistPrefixChange],
    ) -> Result<u64, QuickEditError> {
        self.mutate_quick_artist_prefix_changes(changes, false)
    }

    pub fn reapply_quick_artist_prefix_changes(
        &mut self,
        changes: &[QuickArtistPrefixChange],
    ) -> Result<u64, QuickEditError> {
        self.mutate_quick_artist_prefix_changes(changes, true)
    }

    fn mutate_quick_artist_prefix_changes(
        &mut self,
        changes: &[QuickArtistPrefixChange],
        reapply: bool,
    ) -> Result<u64, QuickEditError> {
        let changes = normalize_artist_prefix_changes(changes)?;
        if changes.is_empty() {
            return Ok(0);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut update = transaction.prepare(
            "UPDATE rows
             SET positive_prompt = ?2,
                 character_prompt = ?3,
                 negative_prompt = ?4,
                 artists = ?5,
                 style_signature = ?6
             WHERE id = ?1",
        )?;
        let mut changed = 0_u64;
        for change in &changes {
            let (positive, character, negative, artists) = if reapply {
                (
                    &change.new_positive_prompt,
                    &change.new_character_prompt,
                    &change.new_negative_prompt,
                    &change.new_artists,
                )
            } else {
                (
                    &change.previous_positive_prompt,
                    &change.previous_character_prompt,
                    &change.previous_negative_prompt,
                    &change.previous_artists,
                )
            };
            let updated = update.execute(params![
                change.row_id,
                positive,
                character,
                negative,
                artists,
                crate::pipeline::style_signature_of(positive.as_deref()),
            ])?;
            if updated == 0 {
                return Err(QuickEditError::UnknownRow(change.row_id));
            }
            changed += u64::try_from(updated).map_err(|_| DatabaseError::CountOverflow)?;
        }
        drop(update);
        transaction.commit()?;
        self.bump_data_version();
        Ok(changed)
    }
}

fn validated_artist_name(artist_name: &str) -> Result<String, QuickEditError> {
    let artist_name = normalize_artist_name(artist_name);
    if artist_name.is_empty() {
        return Err(QuickEditError::EmptyArtistName);
    }
    if artist_name.contains([',', '\n', '\r']) {
        return Err(QuickEditError::InvalidArtistName);
    }
    Ok(artist_name.to_owned())
}

fn artist_prefix_changes(
    connection: &Connection,
    artist_name: &str,
) -> Result<Vec<QuickArtistPrefixChange>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT id, positive_prompt, character_prompt, negative_prompt, artists
         FROM rows
         ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(ArtistPromptRow {
            id: row.get(0)?,
            positive_prompt: row.get(1)?,
            character_prompt: row.get(2)?,
            negative_prompt: row.get(3)?,
            artists: row.get(4)?,
        })
    })?;
    let mut changes = Vec::new();
    for row in rows {
        let row = row?;
        if let Some(change) = artist_prefix_change(row, artist_name) {
            changes.push(change);
        }
    }
    Ok(changes)
}

fn artist_prefix_change(
    row: ArtistPromptRow,
    artist_name: &str,
) -> Option<QuickArtistPrefixChange> {
    let positive_rewrite = row
        .positive_prompt
        .as_deref()
        .and_then(|prompt| prefix_artist_tag_in_prompt(prompt, artist_name));
    let character_rewrite = row
        .character_prompt
        .as_deref()
        .and_then(|prompt| prefix_artist_tag_in_prompt(prompt, artist_name));
    let negative_rewrite = row
        .negative_prompt
        .as_deref()
        .and_then(|prompt| prefix_artist_tag_in_prompt(prompt, artist_name));
    let artist_source_changed = positive_rewrite.is_some() || character_rewrite.is_some();

    if positive_rewrite.is_none() && character_rewrite.is_none() && negative_rewrite.is_none() {
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
fn normalize_artist_prefix_changes(
    changes: &[QuickArtistPrefixChange],
) -> Result<Vec<QuickArtistPrefixChange>, QuickEditError> {
    let mut seen = HashSet::with_capacity(changes.len());
    let mut normalized = Vec::with_capacity(changes.len());
    for change in changes {
        if change.row_id <= 0 {
            return Err(QuickEditError::InvalidRowId(change.row_id));
        }
        if seen.insert(change.row_id) {
            normalized.push(change.clone());
        }
    }
    Ok(normalized)
}
