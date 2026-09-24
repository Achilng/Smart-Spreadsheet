use rusqlite::TransactionBehavior;
use serde::Serialize;

use super::Database;
use super::tags::{RowSelection, TagMutationError, create_selection_rows, drop_selection_tables};
use crate::pipeline::prompt_text::{
    combined_artists, normalize_artist_name, prefix_artist_tag_in_prompt,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptEditResult {
    pub affected_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SinglePromptEditResult {
    pub affected_rows: u64,
    pub new_artists: Option<String>,
    pub artist_llm: Option<String>,
}

impl Database {
    pub fn update_positive_prompt(
        &mut self,
        row_id: i64,
        new_prompt: &str,
    ) -> Result<SinglePromptEditResult, TagMutationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let character_prompt: Option<String> = transaction.query_row(
            "SELECT character_prompt FROM rows WHERE id = ?1",
            [row_id],
            |row| row.get(0),
        )?;
        let artists_str = combined_artists(new_prompt, character_prompt.as_deref());
        let updated = transaction.execute(
            "UPDATE rows SET positive_prompt = ?2, artists = ?3, style_signature = ?4 WHERE id = ?1",
            rusqlite::params![
                row_id,
                new_prompt,
                &artists_str,
                crate::pipeline::style_signature_of(Some(new_prompt))
            ],
        )?;
        let (artists_str, artist_llm) = transaction.query_row(
            "SELECT artists, artist_llm FROM rows WHERE id = ?1",
            [row_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        transaction.commit()?;
        Ok(SinglePromptEditResult {
            affected_rows: updated as u64,
            new_artists: artists_str,
            artist_llm,
        })
    }

    pub fn update_character_prompt(
        &mut self,
        row_id: i64,
        new_prompt: &str,
    ) -> Result<SinglePromptEditResult, TagMutationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let positive_prompt: Option<String> = transaction.query_row(
            "SELECT positive_prompt FROM rows WHERE id = ?1",
            [row_id],
            |row| row.get(0),
        )?;
        let artists_str =
            combined_artists(positive_prompt.as_deref().unwrap_or(""), Some(new_prompt));
        let updated = transaction.execute(
            "UPDATE rows SET character_prompt = ?2, artists = ?3 WHERE id = ?1",
            rusqlite::params![row_id, new_prompt, &artists_str],
        )?;
        let (artists_str, artist_llm) = transaction.query_row(
            "SELECT artists, artist_llm FROM rows WHERE id = ?1",
            [row_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        transaction.commit()?;
        Ok(SinglePromptEditResult {
            affected_rows: updated as u64,
            new_artists: artists_str,
            artist_llm,
        })
    }

    pub fn update_negative_prompt(
        &mut self,
        row_id: i64,
        new_prompt: &str,
    ) -> Result<u64, TagMutationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated = transaction.execute(
            "UPDATE rows SET negative_prompt = ?2 WHERE id = ?1",
            rusqlite::params![row_id, new_prompt],
        )?;
        transaction.commit()?;
        Ok(updated as u64)
    }

    pub fn find_replace_prompt(
        &mut self,
        selection: &RowSelection,
        find: &str,
        replace: &str,
    ) -> Result<PromptEditResult, TagMutationError> {
        if find.is_empty() {
            return Ok(PromptEditResult { affected_rows: 0 });
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        create_selection_rows(&transaction, selection)?;

        let target = super::tags::TARGET_ROWS_TABLE;
        let mut stmt = transaction.prepare(&format!(
            "SELECT r.id, r.positive_prompt, r.character_prompt FROM rows r
             INNER JOIN {target} t ON t.id = r.id
             WHERE r.positive_prompt IS NOT NULL AND INSTR(r.positive_prompt, ?1) > 0"
        ))?;
        let rows_to_update: Vec<(i64, String, Option<String>)> = stmt
            .query_map([find], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut update = transaction
            .prepare("UPDATE rows SET positive_prompt = ?2, artists = ?3, style_signature = ?4 WHERE id = ?1")?;
        let mut count = 0u64;
        for (id, prompt, character_prompt) in &rows_to_update {
            let new_prompt = prompt.replace(find, replace);
            let artists_str = combined_artists(&new_prompt, character_prompt.as_deref());
            update.execute(rusqlite::params![
                id,
                new_prompt,
                artists_str,
                crate::pipeline::style_signature_of(Some(&new_prompt))
            ])?;
            count += 1;
        }
        drop(update);

        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        Ok(PromptEditResult {
            affected_rows: count,
        })
    }

    pub fn prepend_artist(
        &mut self,
        selection: &RowSelection,
        artist_name: &str,
    ) -> Result<PromptEditResult, TagMutationError> {
        let artist_name = normalize_artist_name(artist_name);
        if artist_name.is_empty() {
            return Ok(PromptEditResult { affected_rows: 0 });
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        create_selection_rows(&transaction, selection)?;

        let target = super::tags::TARGET_ROWS_TABLE;
        let mut stmt = transaction.prepare(&format!(
            "SELECT r.id, r.positive_prompt, r.character_prompt FROM rows r
             INNER JOIN {target} t ON t.id = r.id"
        ))?;
        let rows_to_update: Vec<(i64, Option<String>, Option<String>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut update = transaction
            .prepare("UPDATE rows SET positive_prompt = ?2, artists = ?3, style_signature = ?4 WHERE id = ?1")?;
        let mut count = 0u64;
        for (id, prompt, character_prompt) in &rows_to_update {
            let Some(old) = prompt.as_deref() else {
                continue;
            };
            let Some(new_prompt) = prefix_artist_tag_in_prompt(old, artist_name) else {
                continue;
            };
            let artists_str = combined_artists(&new_prompt, character_prompt.as_deref());
            update.execute(rusqlite::params![
                id,
                new_prompt,
                artists_str,
                crate::pipeline::style_signature_of(Some(&new_prompt))
            ])?;
            count += 1;
        }
        drop(update);

        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        Ok(PromptEditResult {
            affected_rows: count,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::super::tags::RowSelection;
    use super::super::test_support::database_with_rows;
    use crate::pipeline::prompt_text::{
        normalized_bare_tag_in_fragment, normalized_explicit_artist_tag_in_fragment,
        prefix_known_artist_tags_in_prompt,
    };

    #[test]
    fn xml_prompt_edits_refresh_artist_comparison_and_keep_legacy_fallback() {
        let mut db = database_with_rows(2);
        let body = "0.8::a, b::, a";
        db.update_positive_prompt(
            1,
            &format!("<artist>{body}</artist><style>year_2025</style> girl"),
        )
        .unwrap();
        db.update_positive_prompt(
            2,
            &format!("<artist> {body} </artist><style>year_2026</style> boy"),
        )
        .unwrap();
        let page = db.query_compare_same_artists(1, 0, 24).unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.rows[0].id, 2);
        assert_eq!(db.row_ids_with_artists(body).unwrap(), vec![1, 2]);
        let changed = db
            .update_character_prompt(1, "girl <artist>0.5::c::</artist>")
            .unwrap();
        assert_eq!(
            changed.new_artists.as_deref(),
            Some("0.8::a, b::, a\n0.5::c::")
        );
        assert_eq!(
            db.query_compare_same_artists(1, 0, 24).unwrap().total_count,
            0
        );
        db.update_character_prompt(1, "girl").unwrap();
        db.update_positive_prompt(1, "artist:legacy, quality")
            .unwrap();
        assert_eq!(db.row_ids_with_artists("artist:legacy").unwrap(), vec![1]);
    }

    #[test]
    fn update_single_row_prompt_and_reextracts_artists() {
        let mut db = database_with_rows(3);
        let result = db
            .update_positive_prompt(1, "artist:alice, best quality, artist:bob")
            .unwrap();
        assert_eq!(result.affected_rows, 1);

        let (prompt, artists): (String, String) = db
            .connection
            .query_row(
                "SELECT positive_prompt, artists FROM rows WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(prompt, "artist:alice, best quality, artist:bob");
        assert_eq!(artists, "artist:alice\nartist:bob");
    }

    #[test]
    fn editing_either_prompt_reextracts_artists_from_both_fields() {
        let mut db = database_with_rows(1);
        let character = db
            .update_character_prompt(1, "1girl, artist:character")
            .unwrap();
        assert_eq!(character.new_artists.as_deref(), Some("artist:character"));

        let positive = db
            .update_positive_prompt(1, "best quality, artist:base")
            .unwrap();
        assert_eq!(
            positive.new_artists.as_deref(),
            Some("artist:base\nartist:character")
        );

        let row: (String, String, String) = db
            .connection
            .query_row(
                "SELECT positive_prompt, character_prompt, artists FROM rows WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "best quality, artist:base".into(),
                "1girl, artist:character".into(),
                "artist:base\nartist:character".into()
            )
        );
    }

    #[test]
    fn update_prompt_falls_back_when_last_explicit_artist_is_removed() {
        let mut db = database_with_rows(1);
        db.update_positive_prompt(1, "artist:x").unwrap();
        db.update_positive_prompt(1, "best quality, 1girl").unwrap();

        let artists: Option<String> = db
            .connection
            .query_row("SELECT artists FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(artists.as_deref(), Some("best quality, 1girl"));
        let empty = db.update_positive_prompt(1, "  ").unwrap();
        assert_eq!(empty.new_artists, None);
    }

    #[test]
    fn find_replace_modifies_matching_rows_only() {
        let mut db = database_with_rows(3);
        db.update_positive_prompt(1, "best quality, artist:alice, masterpiece")
            .unwrap();
        db.update_positive_prompt(2, "best quality, 1girl").unwrap();
        db.update_positive_prompt(3, "best quality, artist:alice, 1boy")
            .unwrap();

        let result = db
            .find_replace_prompt(
                &RowSelection::Explicit {
                    row_ids: vec![1, 2, 3],
                },
                "best quality",
                "amazing quality",
            )
            .unwrap();
        assert_eq!(result.affected_rows, 3);

        let p1: String = db
            .connection
            .query_row("SELECT positive_prompt FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(p1.starts_with("amazing quality"));
    }

    #[test]
    fn find_replace_reextracts_artists() {
        let mut db = database_with_rows(1);
        db.update_positive_prompt(1, "artist:old_name, best quality")
            .unwrap();

        db.find_replace_prompt(
            &RowSelection::Explicit { row_ids: vec![1] },
            "artist:old_name",
            "artist:new_name",
        )
        .unwrap();

        let artists: String = db
            .connection
            .query_row("SELECT artists FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(artists, "artist:new_name");
    }

    #[test]
    fn find_replace_empty_find_is_noop() {
        let mut db = database_with_rows(1);
        let result = db
            .find_replace_prompt(
                &RowSelection::Explicit { row_ids: vec![1] },
                "",
                "something",
            )
            .unwrap();
        assert_eq!(result.affected_rows, 0);
    }

    #[test]
    fn prepend_artist_prefixes_matching_bare_tag_only() {
        let mut db = database_with_rows(3);
        db.update_positive_prompt(1, "best quality, parsley_f, masterpiece")
            .unwrap();
        db.update_positive_prompt(2, "best quality, parsley_fx, masterpiece")
            .unwrap();
        db.update_positive_prompt(3, "best quality, artist:parsley_f, masterpiece")
            .unwrap();

        let result = db
            .prepend_artist(
                &RowSelection::Explicit {
                    row_ids: vec![1, 2, 3],
                },
                "parsley_f",
            )
            .unwrap();
        assert_eq!(result.affected_rows, 1);

        let p1: String = db
            .connection
            .query_row("SELECT positive_prompt FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(p1, "best quality, artist:parsley_f, masterpiece");

        let p2: String = db
            .connection
            .query_row("SELECT positive_prompt FROM rows WHERE id = 2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(p2, "best quality, parsley_fx, masterpiece");

        let p3: String = db
            .connection
            .query_row("SELECT positive_prompt FROM rows WHERE id = 3", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(p3, "best quality, artist:parsley_f, masterpiece");

        let a1: String = db
            .connection
            .query_row("SELECT artists FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(a1, "artist:parsley_f");
    }

    #[test]
    fn prepend_artist_preserves_novelai_weights_and_wrappers() {
        let mut db = database_with_rows(1);
        db.update_positive_prompt(
            1,
            "0.7::parsley_f, (parsley_f:1.2), {parsley_f}, [parsley_f], 0.5::parsley_f::, 0.6::artist:parsley_f",
        )
        .unwrap();

        let result = db
            .prepend_artist(&RowSelection::Explicit { row_ids: vec![1] }, "parsley_f")
            .unwrap();
        assert_eq!(result.affected_rows, 1);

        let prompt: String = db
            .connection
            .query_row("SELECT positive_prompt FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            prompt,
            "0.7::artist:parsley_f, (artist:parsley_f:1.2), {artist:parsley_f}, [artist:parsley_f], 0.5::artist:parsley_f::, 0.6::artist:parsley_f"
        );
    }

    #[test]
    fn prepend_artist_handles_null_prompt_as_noop() {
        let mut db = database_with_rows(1);
        db.connection
            .execute("UPDATE rows SET positive_prompt = NULL WHERE id = 1", [])
            .unwrap();

        let result = db
            .prepend_artist(&RowSelection::Explicit { row_ids: vec![1] }, "bob")
            .unwrap();
        assert_eq!(result.affected_rows, 0);

        let prompt: Option<String> = db
            .connection
            .query_row("SELECT positive_prompt FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(prompt, None);
    }

    #[test]
    fn prepend_artist_empty_name_is_noop() {
        let mut db = database_with_rows(1);
        let result = db
            .prepend_artist(&RowSelection::Explicit { row_ids: vec![1] }, "  ")
            .unwrap();
        assert_eq!(result.affected_rows, 0);
    }

    #[test]
    fn automatic_artist_matching_unwraps_weights_and_skips_existing_prefixes() {
        assert_eq!(
            normalized_bare_tag_in_fragment(" 0.7::(Parsley_F:1.2):: ").as_deref(),
            Some("parsley_f")
        );
        assert_eq!(
            normalized_bare_tag_in_fragment("rourow ::").as_deref(),
            Some("rourow")
        );
        assert_eq!(normalized_bare_tag_in_fragment("artist:parsley_f"), None);
        assert_eq!(
            normalized_explicit_artist_tag_in_fragment(" 0.7::{Artist:Parsley_F:1.2}:: ")
                .as_deref(),
            Some("parsley_f")
        );
        assert_eq!(
            normalized_explicit_artist_tag_in_fragment("artist:rourow ::").as_deref(),
            Some("rourow")
        );
        assert_eq!(
            normalized_explicit_artist_tag_in_fragment("parsley_f"),
            None
        );
    }

    #[test]
    fn automatic_artist_prefixing_handles_multiple_names_in_one_pass() {
        let selected = HashSet::from(["parsley_f".to_owned(), "other_artist".to_owned()]);
        let (rewritten, matched) = prefix_known_artist_tags_in_prompt(
            "best quality, 0.7::Parsley_F::, (other_artist:1.2), artist:parsley_f, parsley_fx",
            &selected,
        )
        .unwrap();

        assert_eq!(
            rewritten,
            "best quality, 0.7::artist:Parsley_F::, (artist:other_artist:1.2), artist:parsley_f, parsley_fx"
        );
        assert_eq!(matched, vec!["other_artist", "parsley_f"]);
    }

    #[test]
    fn artist_prefixing_preserves_cross_comma_numerical_weight_closer() {
        let prompt = "1::artist:huangdanlan, rourow ::,";
        assert_eq!(
            super::prefix_artist_tag_in_prompt(prompt, "rourow").as_deref(),
            Some("1::artist:huangdanlan, artist:rourow ::,")
        );

        let selected = HashSet::from(["rourow".to_owned()]);
        let (rewritten, matched) = prefix_known_artist_tags_in_prompt(prompt, &selected).unwrap();
        assert_eq!(rewritten, "1::artist:huangdanlan, artist:rourow ::,");
        assert_eq!(matched, vec!["rourow"]);
    }
}
