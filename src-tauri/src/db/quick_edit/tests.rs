use super::matching::normalize_prompt_token;
use super::model::*;
use crate::db::{Database, DatabaseError};
use crate::db::{
    NewRow,
    test_support::{append_rows, database_with_rows},
};

fn prompt_condition(tokens: &[&str]) -> QuickEditCondition {
    QuickEditCondition {
        fields: vec![
            QuickEditTextField::PositivePrompt,
            QuickEditTextField::CharacterPrompt,
            QuickEditTextField::NegativePrompt,
            QuickEditTextField::Artists,
            QuickEditTextField::Note,
        ],
        required_tokens: tokens.iter().map(|token| (*token).to_owned()).collect(),
    }
}

#[test]
fn strict_matching_ignores_only_case_and_weight_syntax() {
    let mut database = Database::open_in_memory().unwrap();
    append_rows(
        &mut database,
        &[
            NewRow {
                source_ordinal: 2,
                identity: "one".into(),
                positive_prompt: Some("genshin, masterpiece".into()),
                character_prompt: Some("1.2::{HuTao}::".into()),
                ..NewRow::default()
            },
            NewRow {
                source_ordinal: 3,
                identity: "two".into(),
                positive_prompt: Some("genshin impact, hutao".into()),
                ..NewRow::default()
            },
            NewRow {
                source_ordinal: 4,
                identity: "three".into(),
                positive_prompt: Some("genshin, hu_tao".into()),
                ..NewRow::default()
            },
            NewRow {
                source_ordinal: 5,
                identity: "four".into(),
                positive_prompt: Some("[HUTAO], {GENSHIN:1.1}".into()),
                ..NewRow::default()
            },
            NewRow {
                source_ordinal: 6,
                identity: "five".into(),
                positive_prompt: Some("genshin".into()),
                negative_prompt: Some("hutao".into()),
                ..NewRow::default()
            },
        ],
    );
    database.create_tag("原神").unwrap();

    let preview = database
        .preview_quick_tag(&prompt_condition(&["genshin", "hutao"]), &["原神".into()])
        .unwrap();

    assert_eq!(preview.scanned_rows, 5);
    assert_eq!(preview.matched_rows, 3);
    assert_eq!(preview.sample_row_ids, vec![1, 4, 5]);
    assert_eq!(preview.normalized_tokens, vec!["genshin", "hutao"]);
}

#[test]
fn matching_includes_artists_and_notes() {
    let mut database = Database::open_in_memory().unwrap();
    append_rows(
        &mut database,
        &[
            NewRow {
                source_ordinal: 2,
                identity: "artist-and-note".into(),
                artists: Some("GENSHIN".into()),
                note: Some("1.1::hutao::".into()),
                ..NewRow::default()
            },
            NewRow {
                source_ordinal: 3,
                identity: "note-only".into(),
                note: Some("genshin, {HUTAO}".into()),
                ..NewRow::default()
            },
        ],
    );
    database.create_tag("原神").unwrap();

    let preview = database
        .preview_quick_tag(&prompt_condition(&["genshin", "hutao"]), &["原神".into()])
        .unwrap();

    assert_eq!(preview.scanned_rows, 2);
    assert_eq!(preview.matched_rows, 2);
    assert_eq!(preview.sample_row_ids, vec![1, 2]);
}

#[test]
fn girl_aliases_share_one_quick_edit_match_key() {
    let mut database = Database::open_in_memory().unwrap();
    append_rows(
        &mut database,
        &[
            NewRow {
                source_ordinal: 2,
                identity: "girl".into(),
                positive_prompt: Some("best quality, girl".into()),
                ..NewRow::default()
            },
            NewRow {
                source_ordinal: 3,
                identity: "one-girl".into(),
                character_prompt: Some("{1girl}".into()),
                ..NewRow::default()
            },
            NewRow {
                source_ordinal: 4,
                identity: "spaced-one-girl".into(),
                negative_prompt: Some("1.1::1 GIRL::".into()),
                ..NewRow::default()
            },
            NewRow {
                source_ordinal: 5,
                identity: "plural".into(),
                positive_prompt: Some("2girls".into()),
                ..NewRow::default()
            },
            NewRow {
                source_ordinal: 6,
                identity: "similar".into(),
                positive_prompt: Some("girlfriend".into()),
                ..NewRow::default()
            },
        ],
    );
    database.create_tag("人物").unwrap();

    let preview = database
        .preview_quick_tag(
            &prompt_condition(&["girl", "1girl", "1 girl"]),
            &["人物".into()],
        )
        .unwrap();

    assert_eq!(preview.scanned_rows, 5);
    assert_eq!(preview.matched_rows, 3);
    assert_eq!(preview.sample_row_ids, vec![1, 2, 3]);
    assert_eq!(preview.normalized_tokens, vec!["girl"]);
}

#[test]
fn preview_apply_revert_and_reapply_preserve_preexisting_tags() {
    let mut database = database_with_rows(3);
    database.create_tag("原神").unwrap();
    database.create_tag("胡桃").unwrap();
    database
        .add_tags_to_rows(&[1], &["原神".into(), "胡桃".into()])
        .unwrap();
    database.add_tags_to_rows(&[2], &["原神".into()]).unwrap();
    database
        .connection
        .execute(
            "UPDATE rows SET positive_prompt = 'genshin', character_prompt = 'hutao'
             WHERE id IN (1, 2)",
            [],
        )
        .unwrap();

    let condition = prompt_condition(&["Genshin", "1.1::HUTAO::"]);
    let tags = vec!["原神".into(), "胡桃".into()];
    let preview = database.preview_quick_tag(&condition, &tags).unwrap();
    assert_eq!(preview.matched_rows, 2);
    assert_eq!(preview.rows_needing_changes, 1);
    assert_eq!(preview.already_tagged_rows, 1);
    assert_eq!(preview.associations_to_add, 1);

    let applied = database.apply_quick_tag(&condition, &tags).unwrap();
    assert_eq!(applied.matched_rows, 2);
    assert_eq!(applied.changed_rows, 1);
    assert_eq!(applied.associations_changed, 1);
    assert_eq!(
        applied.changes,
        vec![QuickTagAssociation {
            row_id: 2,
            tag: "胡桃".into(),
        }]
    );

    assert_eq!(
        database.revert_quick_tag_changes(&applied.changes).unwrap(),
        1
    );
    assert_eq!(
        database.get_rows_by_ids(&[1]).unwrap()[0].tags,
        vec!["原神", "胡桃"]
    );
    assert_eq!(
        database.get_rows_by_ids(&[2]).unwrap()[0].tags,
        vec!["原神"]
    );

    assert_eq!(
        database
            .reapply_quick_tag_changes(&applied.changes)
            .unwrap(),
        1
    );
    assert_eq!(
        database.get_rows_by_ids(&[2]).unwrap()[0].tags,
        vec!["原神", "胡桃"]
    );
}

#[test]
fn unknown_target_tag_rolls_back_without_changes() {
    let mut database = database_with_rows(1);
    database
        .connection
        .execute("UPDATE rows SET positive_prompt = 'genshin, hutao'", [])
        .unwrap();

    let result =
        database.apply_quick_tag(&prompt_condition(&["genshin", "hutao"]), &["不存在".into()]);

    assert!(matches!(result, Err(QuickEditError::UnknownTags(_))));
    assert!(database.get_rows_by_ids(&[1]).unwrap()[0].tags.is_empty());
}

#[test]
fn preview_apply_revert_and_reapply_group_restore_each_previous_group() {
    let mut database = database_with_rows(4);
    let previous_group = database.create_group("原分组").unwrap();
    let target_group = database.create_group("目标分组").unwrap();
    database
        .assign_rows_to_group(
            &crate::db::RowSelection::Explicit { row_ids: vec![1] },
            target_group.id,
        )
        .unwrap();
    database
        .assign_rows_to_group(
            &crate::db::RowSelection::Explicit { row_ids: vec![2] },
            previous_group.id,
        )
        .unwrap();
    database
        .connection
        .execute(
            "UPDATE rows SET positive_prompt = 'genshin, hutao' WHERE id IN (1, 2, 3)",
            [],
        )
        .unwrap();

    let condition = prompt_condition(&["genshin", "hutao"]);
    let preview = database
        .preview_quick_group(&condition, target_group.id, false)
        .unwrap();
    assert_eq!(preview.scanned_rows, 4);
    assert_eq!(preview.matched_rows, 3);
    assert_eq!(preview.rows_needing_changes, 2);
    assert_eq!(preview.already_in_group_rows, 1);
    assert_eq!(preview.skipped_grouped_rows, 0);
    assert!(!preview.only_ungrouped);
    assert_eq!(preview.sample_row_ids, vec![1, 2, 3]);

    let applied = database
        .apply_quick_group(&condition, target_group.id, false)
        .unwrap();
    assert_eq!(applied.changed_rows, 2);
    assert_eq!(
        applied.changes,
        vec![
            QuickGroupChange {
                row_id: 2,
                previous_group_id: Some(previous_group.id),
                target_group_id: target_group.id,
            },
            QuickGroupChange {
                row_id: 3,
                previous_group_id: None,
                target_group_id: target_group.id,
            },
        ]
    );
    assert_eq!(
        database
            .get_rows_by_ids(&[1, 2, 3])
            .unwrap()
            .iter()
            .map(|row| row.group_id)
            .collect::<Vec<_>>(),
        vec![
            Some(target_group.id),
            Some(target_group.id),
            Some(target_group.id)
        ]
    );

    assert_eq!(
        database
            .revert_quick_group_changes(&applied.changes)
            .unwrap(),
        2
    );
    assert_eq!(
        database
            .get_rows_by_ids(&[1, 2, 3])
            .unwrap()
            .iter()
            .map(|row| row.group_id)
            .collect::<Vec<_>>(),
        vec![Some(target_group.id), Some(previous_group.id), None]
    );

    assert_eq!(
        database
            .reapply_quick_group_changes(&applied.changes)
            .unwrap(),
        2
    );
    assert_eq!(
        database
            .get_rows_by_ids(&[2, 3])
            .unwrap()
            .iter()
            .map(|row| row.group_id)
            .collect::<Vec<_>>(),
        vec![Some(target_group.id), Some(target_group.id)]
    );
}

#[test]
fn quick_group_can_skip_every_already_grouped_match() {
    let mut database = database_with_rows(4);
    let previous_group = database.create_group("原分组").unwrap();
    let target_group = database.create_group("目标分组").unwrap();
    database
        .assign_rows_to_group(
            &crate::db::RowSelection::Explicit { row_ids: vec![1] },
            previous_group.id,
        )
        .unwrap();
    database
        .assign_rows_to_group(
            &crate::db::RowSelection::Explicit { row_ids: vec![2] },
            target_group.id,
        )
        .unwrap();
    database
        .connection
        .execute(
            "UPDATE rows SET positive_prompt = 'genshin' WHERE id IN (1, 2, 3)",
            [],
        )
        .unwrap();

    let condition = prompt_condition(&["genshin"]);
    let preview = database
        .preview_quick_group(&condition, target_group.id, true)
        .unwrap();
    assert_eq!(preview.scanned_rows, 4);
    assert_eq!(preview.matched_rows, 3);
    assert_eq!(preview.rows_needing_changes, 1);
    assert_eq!(preview.already_in_group_rows, 0);
    assert_eq!(preview.skipped_grouped_rows, 2);
    assert!(preview.only_ungrouped);
    assert_eq!(preview.sample_row_ids, vec![3]);

    let applied = database
        .apply_quick_group(&condition, target_group.id, true)
        .unwrap();
    assert_eq!(applied.matched_rows, 3);
    assert_eq!(applied.changed_rows, 1);
    assert_eq!(applied.skipped_grouped_rows, 2);
    assert!(applied.only_ungrouped);
    assert_eq!(
        applied.changes,
        vec![QuickGroupChange {
            row_id: 3,
            previous_group_id: None,
            target_group_id: target_group.id,
        }]
    );
    assert_eq!(
        database
            .get_rows_by_ids(&[1, 2, 3])
            .unwrap()
            .iter()
            .map(|row| row.group_id)
            .collect::<Vec<_>>(),
        vec![
            Some(previous_group.id),
            Some(target_group.id),
            Some(target_group.id),
        ]
    );

    assert_eq!(
        database
            .revert_quick_group_changes(&applied.changes)
            .unwrap(),
        1
    );
    assert_eq!(database.get_rows_by_ids(&[3]).unwrap()[0].group_id, None);
}

#[test]
fn quick_group_rejects_unknown_target_without_changes() {
    let mut database = database_with_rows(1);
    database
        .connection
        .execute("UPDATE rows SET positive_prompt = 'genshin, hutao'", [])
        .unwrap();

    let result = database.apply_quick_group(&prompt_condition(&["genshin"]), 999, false);

    assert!(matches!(
        result,
        Err(QuickEditError::Database(DatabaseError::GroupNotFound(999)))
    ));
    assert_eq!(database.get_rows_by_ids(&[1]).unwrap()[0].group_id, None);
}

#[test]
fn quick_artist_prefix_covers_all_prompt_fields_and_supports_undo_redo() {
    let mut database = Database::open_in_memory().unwrap();
    append_rows(
        &mut database,
        &[
            NewRow {
                source_ordinal: 2,
                identity: "all-fields".into(),
                positive_prompt: Some("best quality, parsley_f".into()),
                character_prompt: Some("(parsley_f:1.2), 1girl".into()),
                negative_prompt: Some("0.7::parsley_f::, lowres".into()),
                ..NewRow::default()
            },
            NewRow {
                source_ordinal: 3,
                identity: "negative-only".into(),
                positive_prompt: Some("artist:existing".into()),
                negative_prompt: Some("{parsley_f}".into()),
                artists: Some("artist:existing".into()),
                ..NewRow::default()
            },
            NewRow {
                source_ordinal: 4,
                identity: "already-or-similar".into(),
                positive_prompt: Some("artist:parsley_f, parsley_fx".into()),
                artists: Some("artist:parsley_f".into()),
                ..NewRow::default()
            },
        ],
    );

    let preview = database
        .preview_quick_artist_prefix("artist:parsley_f")
        .unwrap();
    assert_eq!(preview.scanned_rows, 3);
    assert_eq!(preview.matched_rows, 2);
    assert_eq!(preview.rows_needing_changes, 2);
    assert_eq!(preview.prompt_fields_needing_changes, 4);
    assert_eq!(preview.sample_row_ids, vec![1, 2]);
    assert_eq!(preview.artist_name, "parsley_f");

    let applied = database.apply_quick_artist_prefix("parsley_f").unwrap();
    assert_eq!(applied.changed_rows, 2);
    assert_eq!(applied.prompt_fields_changed, 4);

    let first: (String, String, String, String) = database
        .connection
        .query_row(
            "SELECT positive_prompt, character_prompt, negative_prompt, artists
             FROM rows WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        first,
        (
            "best quality, artist:parsley_f".into(),
            "(artist:parsley_f:1.2), 1girl".into(),
            "0.7::artist:parsley_f::, lowres".into(),
            "artist:parsley_f\n(artist:parsley_f:1.2)".into(),
        )
    );

    let second: (String, String) = database
        .connection
        .query_row(
            "SELECT negative_prompt, artists FROM rows WHERE id = 2",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        second,
        ("{artist:parsley_f}".into(), "artist:existing".into())
    );

    assert_eq!(
        database
            .revert_quick_artist_prefix_changes(&applied.changes)
            .unwrap(),
        2
    );
    let reverted: (String, String, String, Option<String>) = database
        .connection
        .query_row(
            "SELECT positive_prompt, character_prompt, negative_prompt, artists
             FROM rows WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        reverted,
        (
            "best quality, parsley_f".into(),
            "(parsley_f:1.2), 1girl".into(),
            "0.7::parsley_f::, lowres".into(),
            None,
        )
    );

    assert_eq!(
        database
            .reapply_quick_artist_prefix_changes(&applied.changes)
            .unwrap(),
        2
    );
    let redone: String = database
        .connection
        .query_row("SELECT positive_prompt FROM rows WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(redone, "best quality, artist:parsley_f");
}

#[test]
fn quick_artist_prefix_handles_numerical_weight_closer_after_comma() {
    let mut database = Database::open_in_memory().unwrap();
    append_rows(
        &mut database,
        &[NewRow {
            source_ordinal: 1,
            identity: "cross-comma-weight".into(),
            positive_prompt: Some("1::artist:huangdanlan, rourow::,".into()),
            ..NewRow::default()
        }],
    );

    let preview = database.preview_quick_artist_prefix("rourow").unwrap();
    assert_eq!(preview.scanned_rows, 1);
    assert_eq!(preview.matched_rows, 1);
    assert_eq!(preview.prompt_fields_needing_changes, 1);

    let applied = database.apply_quick_artist_prefix("rourow").unwrap();
    assert_eq!(applied.changed_rows, 1);
    assert_eq!(applied.prompt_fields_changed, 1);

    let row: (String, String) = database
        .connection
        .query_row(
            "SELECT positive_prompt, artists FROM rows WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "1::artist:huangdanlan, artist:rourow::,".into(),
            "1::artist:huangdanlan\nartist:rourow::".into(),
        )
    );
}

#[test]
fn quick_artist_prefix_rejects_multiple_names() {
    let database = database_with_rows(1);
    assert!(matches!(
        database.preview_quick_artist_prefix("alice, bob"),
        Err(QuickEditError::InvalidArtistName)
    ));
}

#[test]
fn normalization_keeps_spaces_and_underscores_distinct() {
    assert_eq!(normalize_prompt_token("Hu Tao"), "hu tao");
    assert_eq!(normalize_prompt_token("hu_tao"), "hu_tao");
    assert_eq!(normalize_prompt_token("1.2::{{HUTAO:1.1}}::"), "hutao");
    assert_eq!(normalize_prompt_token("girl"), "girl");
    assert_eq!(normalize_prompt_token("1girl"), "girl");
    assert_eq!(normalize_prompt_token("(1 GIRL:1.2)"), "girl");
}
