use super::files::drag_row_ids;
use crate::db::{DedupeMode, RowSelection, TagMatchMode};

#[test]
fn file_drag_keeps_the_whole_selection_and_moves_the_anchor_first() {
    assert_eq!(drag_row_ids(3, vec![1, 2, 3]), vec![3, 1, 2]);
}

#[test]
fn file_drag_ignores_a_selection_that_does_not_contain_the_anchor() {
    assert_eq!(drag_row_ids(4, vec![1, 2, 3]), vec![4]);
}

#[test]
fn deserializes_filtered_selection_preserving_case_and_exclusions() {
    let json = serde_json::json!({
        "kind": "filtered",
        "tags": ["Landscape", "landscape"],
        "tagMode": "or",
        "dedupe": "artists",
        "singleArtistOnly": false,
        "excludedRowIds": [2, 9]
    });
    let selection: RowSelection = serde_json::from_value(json).unwrap();

    assert_eq!(
        selection,
        RowSelection::Filtered {
            tags: vec!["Landscape".into(), "landscape".into()],
            tag_mode: TagMatchMode::Or,
            dedupe: DedupeMode::Artists,
            single_artist_only: false,
            artist_filter: String::new(),
            has_vibe: false,
            untagged_only: false,
            filters: vec![],
            search: String::new(),
            excluded_row_ids: vec![2, 9],
        }
    );
}
