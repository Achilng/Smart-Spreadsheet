use std::collections::HashSet;

use rusqlite::TransactionBehavior;
use serde::Serialize;

use super::Database;
use super::tags::{RowSelection, TagMutationError, create_selection_rows, drop_selection_tables};
use crate::pipeline::extract_artist_tags;

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
}

pub(super) fn normalize_artist_name(artist_name: &str) -> &str {
    artist_name
        .trim()
        .strip_prefix("artist:")
        .unwrap_or(artist_name.trim())
        .trim()
}

pub(super) fn prefix_artist_tag_in_prompt(prompt: &str, artist_name: &str) -> Option<String> {
    let mut changed = false;
    let mut result = String::with_capacity(prompt.len() + "artist:".len());
    let mut start = 0;

    for (index, ch) in prompt.char_indices() {
        if matches!(ch, ',' | '\n' | '\r') {
            append_prefixed_fragment(
                &mut result,
                &prompt[start..index],
                artist_name,
                &mut changed,
            );
            result.push(ch);
            start = index + ch.len_utf8();
        }
    }
    append_prefixed_fragment(&mut result, &prompt[start..], artist_name, &mut changed);

    changed.then_some(result)
}

pub(super) fn normalized_bare_tag_in_fragment(fragment: &str) -> Option<String> {
    let token = fragment.trim();
    let bare = bare_tag_in_token(token)?;
    let normalized = bare.to_lowercase();
    (!normalized.starts_with("artist:")).then_some(normalized)
}

pub(super) fn normalized_explicit_artist_tag_in_fragment(fragment: &str) -> Option<String> {
    let token = fragment.trim();
    let name = explicit_artist_name_in_token(token)?.trim();
    (!name.is_empty()).then(|| name.to_lowercase())
}

pub(super) fn prefix_known_artist_tags_in_prompt(
    prompt: &str,
    selected_names: &HashSet<String>,
) -> Option<(String, Vec<String>)> {
    let mut changed = false;
    let mut matched_names = HashSet::new();
    let mut result = String::with_capacity(prompt.len() + "artist:".len());
    let mut start = 0;

    for (index, ch) in prompt.char_indices() {
        if matches!(ch, ',' | '\n' | '\r') {
            append_known_artist_fragment(
                &mut result,
                &prompt[start..index],
                selected_names,
                &mut matched_names,
                &mut changed,
            );
            result.push(ch);
            start = index + ch.len_utf8();
        }
    }
    append_known_artist_fragment(
        &mut result,
        &prompt[start..],
        selected_names,
        &mut matched_names,
        &mut changed,
    );

    changed.then(|| {
        let mut matched_names = matched_names.into_iter().collect::<Vec<_>>();
        matched_names.sort();
        (result, matched_names)
    })
}

fn append_known_artist_fragment(
    result: &mut String,
    fragment: &str,
    selected_names: &HashSet<String>,
    matched_names: &mut HashSet<String>,
    changed: &mut bool,
) {
    let start = fragment
        .find(|ch: char| !ch.is_whitespace())
        .unwrap_or(fragment.len());
    let end = fragment
        .rfind(|ch: char| !ch.is_whitespace())
        .map(|index| index + fragment[index..].chars().next().unwrap().len_utf8())
        .unwrap_or(start);
    let token = &fragment[start..end];
    if let Some((rewritten, matched_name)) = prefix_known_artist_tag_in_token(token, selected_names)
    {
        result.push_str(&fragment[..start]);
        result.push_str(&rewritten);
        result.push_str(&fragment[end..]);
        matched_names.insert(matched_name);
        *changed = true;
    } else {
        result.push_str(fragment);
    }
}

fn prefix_known_artist_tag_in_token(
    token: &str,
    selected_names: &HashSet<String>,
) -> Option<(String, String)> {
    if token.is_empty() {
        return None;
    }
    if let Some((inner, suffix)) = split_novelai_closer(token)
        && let Some((rewritten, matched_name)) =
            prefix_known_artist_tag_in_token(inner, selected_names)
    {
        return Some((format!("{rewritten}{suffix}"), matched_name));
    }
    if let Some((prefix, inner, suffix)) = split_novelai_weight(token)
        && let Some((rewritten, matched_name)) =
            prefix_known_artist_tag_in_token(inner, selected_names)
    {
        return Some((format!("{prefix}{rewritten}{suffix}"), matched_name));
    }
    if let Some((open, inner, close)) = split_outer_wrapper(token)
        && let Some((rewritten, matched_name)) =
            prefix_known_artist_tag_in_token(inner, selected_names)
    {
        return Some((format!("{open}{rewritten}{close}"), matched_name));
    }
    if let Some((name, weight)) = split_colon_weight(token)
        && let Some((rewritten, matched_name)) =
            prefix_known_artist_tag_in_token(name, selected_names)
    {
        return Some((format!("{rewritten}{weight}"), matched_name));
    }

    let matched_name = token.to_lowercase();
    if matched_name.starts_with("artist:") || !selected_names.contains(&matched_name) {
        return None;
    }
    Some((format!("artist:{token}"), matched_name))
}

fn bare_tag_in_token(token: &str) -> Option<&str> {
    if token.is_empty() {
        return None;
    }
    if let Some((inner, _)) = split_novelai_closer(token) {
        return bare_tag_in_token(inner);
    }
    if let Some((_, inner, _)) = split_novelai_weight(token) {
        return bare_tag_in_token(inner);
    }
    if let Some((_, inner, _)) = split_outer_wrapper(token) {
        return bare_tag_in_token(inner);
    }
    if let Some((name, _)) = split_colon_weight(token) {
        return bare_tag_in_token(name);
    }
    Some(token)
}

fn explicit_artist_name_in_token(token: &str) -> Option<&str> {
    if token.is_empty() {
        return None;
    }
    if let Some((inner, _)) = split_novelai_closer(token) {
        return explicit_artist_name_in_token(inner);
    }
    if let Some((_, inner, _)) = split_novelai_weight(token) {
        return explicit_artist_name_in_token(inner);
    }
    if let Some((_, inner, _)) = split_outer_wrapper(token) {
        return explicit_artist_name_in_token(inner);
    }
    if let Some((name, _)) = split_colon_weight(token) {
        return explicit_artist_name_in_token(name);
    }
    token
        .get(.."artist:".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("artist:"))
        .then(|| &token["artist:".len()..])
}

fn append_prefixed_fragment(
    result: &mut String,
    fragment: &str,
    artist_name: &str,
    changed: &mut bool,
) {
    if let Some(rewritten) = prefix_artist_tag_in_fragment(fragment, artist_name) {
        result.push_str(&rewritten);
        *changed = true;
    } else {
        result.push_str(fragment);
    }
}

fn prefix_artist_tag_in_fragment(fragment: &str, artist_name: &str) -> Option<String> {
    let start = fragment
        .find(|ch: char| !ch.is_whitespace())
        .unwrap_or(fragment.len());
    let end = fragment
        .rfind(|ch: char| !ch.is_whitespace())
        .map(|index| index + fragment[index..].chars().next().unwrap().len_utf8())
        .unwrap_or(start);
    let token = &fragment[start..end];
    let rewritten = prefix_artist_tag_in_token(token, artist_name)?;

    Some(format!(
        "{}{}{}",
        &fragment[..start],
        rewritten,
        &fragment[end..]
    ))
}

fn prefix_artist_tag_in_token(token: &str, artist_name: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }

    if let Some((inner, suffix)) = split_novelai_closer(token)
        && let Some(rewritten) = prefix_artist_tag_in_token(inner, artist_name)
    {
        return Some(format!("{rewritten}{suffix}"));
    }

    if let Some((prefix, inner, suffix)) = split_novelai_weight(token)
        && let Some(rewritten) = prefix_artist_tag_in_token(inner, artist_name)
    {
        return Some(format!("{prefix}{rewritten}{suffix}"));
    }

    if let Some((open, inner, close)) = split_outer_wrapper(token)
        && let Some(rewritten) = prefix_artist_tag_in_token(inner, artist_name)
    {
        return Some(format!("{open}{rewritten}{close}"));
    }

    if let Some((name, weight)) = split_colon_weight(token)
        && let Some(rewritten) = prefix_artist_tag_in_token(name, artist_name)
    {
        return Some(format!("{rewritten}{weight}"));
    }

    (token == artist_name).then(|| format!("artist:{token}"))
}

fn split_novelai_closer(token: &str) -> Option<(&str, &str)> {
    // Numerical emphasis can span comma-delimited tags, leaving its closing `::`
    // attached to a fragment whose opening weight appeared in an earlier fragment.
    let without_closer = token.strip_suffix("::")?;
    let inner = without_closer.trim_end();
    (!inner.is_empty()).then_some((inner, &token[inner.len()..]))
}

fn split_novelai_weight(token: &str) -> Option<(&str, &str, &str)> {
    let weight_end = token.find("::")? + "::".len();
    let rest = &token[weight_end..];
    if rest.is_empty() {
        return None;
    }

    let (inner, suffix) = rest
        .strip_suffix("::")
        .map_or((rest, ""), |inner| (inner, "::"));
    if inner.is_empty() {
        return None;
    }

    Some((&token[..weight_end], inner, suffix))
}

fn split_outer_wrapper(token: &str) -> Option<(char, &str, char)> {
    let open = token.chars().next()?;
    let close = token.chars().next_back()?;
    if !matches!((open, close), ('(', ')') | ('{', '}') | ('[', ']')) {
        return None;
    }

    let start = open.len_utf8();
    let end = token.len() - close.len_utf8();
    Some((open, &token[start..end], close))
}

fn split_colon_weight(token: &str) -> Option<(&str, &str)> {
    let index = token.rfind(':')?;
    let weight = &token[index + ':'.len_utf8()..];
    if weight.is_empty() || weight.parse::<f32>().is_err() {
        return None;
    }

    let name = &token[..index];
    (!name.is_empty()).then_some((name, &token[index..]))
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
        transaction.commit()?;
        Ok(SinglePromptEditResult {
            affected_rows: updated as u64,
            new_artists: artists_str,
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
        transaction.commit()?;
        Ok(SinglePromptEditResult {
            affected_rows: updated as u64,
            new_artists: artists_str,
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

pub(super) fn combined_artists(
    positive_prompt: &str,
    character_prompt: Option<&str>,
) -> Option<String> {
    let combined = match character_prompt.filter(|value| !value.trim().is_empty()) {
        Some(character) => format!("{positive_prompt}\n{character}"),
        None => positive_prompt.to_owned(),
    };
    let artists = extract_artist_tags(&combined);
    (!artists.is_empty()).then(|| artists.join("\n"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::super::tags::RowSelection;
    use super::super::test_support::database_with_rows;
    use super::{
        normalized_bare_tag_in_fragment, normalized_explicit_artist_tag_in_fragment,
        prefix_known_artist_tags_in_prompt,
    };

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
    fn update_prompt_clears_artists_when_none_present() {
        let mut db = database_with_rows(1);
        db.update_positive_prompt(1, "artist:x").unwrap();
        db.update_positive_prompt(1, "best quality, 1girl").unwrap();

        let artists: Option<String> = db
            .connection
            .query_row("SELECT artists FROM rows WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(artists, None);
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
