//! Apply already-computed changes inside the caller's transaction.
use super::quick_edit::{QuickArtistPrefixChange, QuickEditError};
use rusqlite::{Connection, params};

pub(super) struct ArtistPrefixWriter<'a>(rusqlite::Statement<'a>);

impl<'a> ArtistPrefixWriter<'a> {
    pub(super) fn new(connection: &'a Connection) -> rusqlite::Result<Self> {
        Ok(Self(connection.prepare(
            "UPDATE rows SET positive_prompt = ?2, character_prompt = ?3,
             negative_prompt = ?4, artists = ?5, style_signature = ?6, artist_llm = ?7
             WHERE id = ?1",
        )?))
    }

    pub(super) fn write_change(
        &mut self,
        change: &QuickArtistPrefixChange,
        forward: bool,
    ) -> rusqlite::Result<usize> {
        let (positive, character, negative, artists, llm) = if forward {
            (
                &change.new_positive_prompt,
                &change.new_character_prompt,
                &change.new_negative_prompt,
                &change.new_artists,
                &change.new_artist_llm,
            )
        } else {
            (
                &change.previous_positive_prompt,
                &change.previous_character_prompt,
                &change.previous_negative_prompt,
                &change.previous_artists,
                &change.previous_artist_llm,
            )
        };
        self.0.execute(params![
            change.row_id,
            positive,
            character,
            negative,
            artists,
            crate::pipeline::style_signature_of(positive.as_deref()),
            llm
        ])
    }
}

pub(super) fn write_changes(
    connection: &Connection,
    changes: &[QuickArtistPrefixChange],
    forward: bool,
) -> Result<u64, QuickEditError> {
    let mut writer = ArtistPrefixWriter::new(connection)?;
    let mut count = 0;
    for change in changes {
        let updated = writer.write_change(change, forward)?;
        if updated == 0 {
            return Err(QuickEditError::UnknownRow(change.row_id));
        }
        count += updated as u64;
    }
    Ok(count)
}
