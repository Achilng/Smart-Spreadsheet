use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::Serialize;

use super::{Database, DatabaseError};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistRepresentativeResult {
    pub conflict: bool,
    pub representative_id: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library_filters::LibraryFilter;
    use crate::db::test_support::database_with_rows;
    use crate::db::{DedupeMode, RowQuery, RowSelection, SortMode, TagMatchMode};

    fn fixture() -> Database {
        let database = database_with_rows(5);
        database
            .connection
            .execute_batch(
                "UPDATE rows SET artists = CASE id WHEN 1 THEN 'artist A' WHEN 2 THEN ' artist A '
             WHEN 3 THEN 'artist a' WHEN 4 THEN '' ELSE NULL END,
             positive_prompt = 'same prompt', vibe_signature = 'same vibe';",
            )
            .unwrap();
        database
    }

    fn query() -> RowQuery {
        RowQuery {
            offset: 0,
            limit: 100,
            tags: vec![],
            tag_mode: TagMatchMode::And,
            dedupe: DedupeMode::Artists,
            single_artist_only: false,
            artist_filter: String::new(),
            has_vibe: false,
            untagged_only: false,
            filters: vec![],
            group_view: false,
            hide_grouped: false,
            search: String::new(),
        }
    }

    fn ids(database: &mut Database, query: &RowQuery) -> Vec<i64> {
        database
            .query_rows(query)
            .unwrap()
            .rows
            .iter()
            .map(|row| row.id)
            .collect()
    }

    #[test]
    fn representative_overrides_oldest_and_invalidates_cached_dedupe() {
        let mut database = fixture();
        let mut query = query();
        assert_eq!(ids(&mut database, &query), vec![1, 3, 4, 5]);
        let result = database
            .set_artist_representative(2, " artist A ", true, None)
            .unwrap();
        assert!(!result.conflict);
        assert_eq!(result.representative_id, Some(2));
        assert_eq!(ids(&mut database, &query), vec![2, 3, 4, 5]);
        assert!(database.get_rows_by_ids(&[2]).unwrap()[0].artist_representative);
        for sort in [SortMode::TimeDesc, SortMode::RecentlyUpdated] {
            let rows = database.query_rows_sorted(&query, sort).unwrap().rows;
            assert!(rows.iter().any(|row| row.id == 2));
            assert!(!rows.iter().any(|row| row.id == 1));
        }
        query.limit = 1;
        assert_eq!(ids(&mut database, &query), vec![2]);
        query.offset = 1;
        assert_eq!(ids(&mut database, &query), vec![3]);
        let selection = RowSelection::Filtered {
            tags: vec![],
            tag_mode: TagMatchMode::And,
            dedupe: DedupeMode::Artists,
            single_artist_only: false,
            artist_filter: String::new(),
            has_vibe: false,
            untagged_only: false,
            filters: vec![],
            search: String::new(),
            excluded_row_ids: vec![],
        };
        assert_eq!(
            database.selected_row_ids(&selection).unwrap(),
            vec![2, 3, 4, 5]
        );
    }

    #[test]
    fn representative_respects_filters_and_other_dedupe_modes() {
        let mut database = fixture();
        database
            .set_artist_representative(2, "artist A", true, None)
            .unwrap();
        let mut query = query();
        database.set_favorite(1, true).unwrap();
        query.filters = vec![LibraryFilter::Favorite];
        assert_eq!(ids(&mut database, &query), vec![1]);
        database.set_favorite(2, true).unwrap();
        assert_eq!(ids(&mut database, &query), vec![2]);
        query.filters.clear();
        database.create_tag("only-first").unwrap();
        database
            .set_tags_for_row(1, &["only-first".into()])
            .unwrap();
        query.tags = vec!["only-first".into()];
        assert_eq!(ids(&mut database, &query), vec![1]);
        query.tags.clear();
        database
            .connection
            .execute(
                "UPDATE rows SET image_path = 'unique-first.png' WHERE id = 1",
                [],
            )
            .unwrap();
        database.bump_data_version();
        query.search = "unique-first".into();
        assert_eq!(ids(&mut database, &query), vec![1]);
        query.search.clear();
        query.dedupe = DedupeMode::None;
        assert_eq!(ids(&mut database, &query), vec![1, 2, 3, 4, 5]);
        for mode in [DedupeMode::PositivePrompt, DedupeMode::Vibes] {
            query.dedupe = mode;
            assert_eq!(ids(&mut database, &query), vec![1]);
        }
    }

    #[test]
    fn conflict_requires_explicit_replacement_and_stale_confirmation_is_safe() {
        let mut database = fixture();
        database
            .set_artist_representative(2, "artist A", true, None)
            .unwrap();
        let conflict = database
            .set_artist_representative(1, "artist A", true, None)
            .unwrap();
        assert!(conflict.conflict);
        assert_eq!(conflict.representative_id, Some(2));
        assert_eq!(ids(&mut database, &query()), vec![2, 3, 4, 5]);
        assert!(
            !database
                .set_artist_representative(1, "artist A", true, Some(2))
                .unwrap()
                .conflict
        );
        let rows = database.get_rows_by_ids(&[1, 2]).unwrap();
        assert!(rows[0].artist_representative);
        assert!(!rows[1].artist_representative);
        let stale = database
            .set_artist_representative(2, "artist A", true, Some(2))
            .unwrap();
        assert!(stale.conflict);
        assert_eq!(stale.representative_id, Some(1));
        // Cancelling a stale, unmarked row cannot remove the current choice.
        database
            .set_artist_representative(2, "artist A", false, None)
            .unwrap();
        assert!(database.get_rows_by_ids(&[1]).unwrap()[0].artist_representative);
        database
            .set_artist_representative(1, "artist A", false, None)
            .unwrap();
        assert!(!database.get_rows_by_ids(&[1]).unwrap()[0].artist_representative);
        assert_eq!(ids(&mut database, &query()), vec![1, 3, 4, 5]);
    }

    #[test]
    fn empty_missing_and_changed_artists_cannot_be_marked() {
        let mut database = fixture();
        for id in [4, 5] {
            assert!(matches!(
                database.set_artist_representative(id, "", true, None),
                Err(DatabaseError::EmptyArtists)
            ));
        }
        assert!(matches!(
            database.set_artist_representative(999, "artist A", true, None),
            Err(DatabaseError::RowNotFound(999))
        ));
        assert!(matches!(
            database.set_artist_representative(1, "old artist", true, None),
            Err(DatabaseError::ArtistStringChanged)
        ));
        database
            .set_artist_representative(1, "artist A", true, None)
            .unwrap();
        // Case and order remain significant, matching the existing dedupe key.
        assert!(
            !database
                .set_artist_representative(3, "artist a", true, None)
                .unwrap()
                .conflict
        );
    }

    #[test]
    fn changing_artist_or_deleting_image_releases_the_choice() {
        let mut database = fixture();
        database
            .set_artist_representative(2, "artist A", true, None)
            .unwrap();
        database
            .set_artist_representative(3, "artist a", true, None)
            .unwrap();
        database
            .connection
            .execute("UPDATE rows SET artists = 'artist A ' WHERE id = 2", [])
            .unwrap();
        assert!(database.get_rows_by_ids(&[2]).unwrap()[0].artist_representative);
        database
            .connection
            .execute("UPDATE rows SET artists = 'artist a' WHERE id = 2", [])
            .unwrap();
        assert!(!database.get_rows_by_ids(&[2]).unwrap()[0].artist_representative);
        assert!(database.get_rows_by_ids(&[3]).unwrap()[0].artist_representative);
        database
            .connection
            .execute("DELETE FROM rows WHERE id = 3", [])
            .unwrap();
        assert!(
            !database
                .set_artist_representative(2, "artist a", true, None)
                .unwrap()
                .conflict
        );
        database
            .connection
            .execute("UPDATE rows SET artists = NULL WHERE id = 2", [])
            .unwrap();
        assert!(!database.get_rows_by_ids(&[2]).unwrap()[0].artist_representative);
    }

    #[test]
    fn protected_llm_artist_string_preserves_its_representative() {
        let mut database = fixture();
        database
            .connection
            .execute(
                "UPDATE rows SET artist_llm = '{\"artistString\":\"artist A\"}' WHERE id = 1",
                [],
            )
            .unwrap();
        database
            .set_artist_representative(1, "artist A", true, None)
            .unwrap();
        database
            .connection
            .execute(
                "UPDATE rows SET artists = 'ignored fallback' WHERE id = 1",
                [],
            )
            .unwrap();
        let row = database.get_rows_by_ids(&[1]).unwrap().remove(0);
        assert_eq!(row.artists.as_deref(), Some("artist A"));
        assert!(row.artist_representative);
        database.connection.execute(
            "UPDATE rows SET artists = 'new artist', artist_llm = '{\"artistString\":\"new artist\"}' WHERE id = 1", [],
        ).unwrap();
        assert!(!database.get_rows_by_ids(&[1]).unwrap()[0].artist_representative);
    }
}

impl Database {
    /// Compare-and-set: a replacement must name the representative the user saw.
    pub fn set_artist_representative(
        &mut self,
        row_id: i64,
        artists: &str,
        enabled: bool,
        expected_representative_id: Option<i64>,
    ) -> Result<ArtistRepresentativeResult, DatabaseError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let key: Option<String> = transaction
            .query_row(
                "SELECT NULLIF(TRIM(COALESCE(artists, '')), '') FROM rows WHERE id = ?1",
                [row_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(DatabaseError::RowNotFound(row_id))?;
        let key = key.ok_or(DatabaseError::EmptyArtists)?;
        // Use the same ASCII-space normalization as SQLite TRIM and dedupe.
        if key != artists.trim_matches(' ') {
            return Err(DatabaseError::ArtistStringChanged);
        }
        let current: Option<i64> = transaction
            .query_row(
                "SELECT row_id FROM artist_representatives WHERE artist_key = ?1",
                [&key],
                |row| row.get(0),
            )
            .optional()?;
        if enabled && current != Some(row_id) && current != expected_representative_id {
            return Ok(ArtistRepresentativeResult {
                conflict: true,
                representative_id: current,
            });
        }
        let representative_id = if enabled {
            transaction.execute(
                "INSERT INTO artist_representatives(artist_key, row_id) VALUES (?1, ?2)
                 ON CONFLICT(artist_key) DO UPDATE SET row_id = excluded.row_id",
                params![key, row_id],
            )?;
            Some(row_id)
        } else {
            transaction.execute(
                "DELETE FROM artist_representatives WHERE row_id = ?1",
                [row_id],
            )?;
            current.filter(|id| *id != row_id)
        };
        transaction.commit()?;
        self.bump_data_version();
        Ok(ArtistRepresentativeResult {
            conflict: false,
            representative_id,
        })
    }
}
