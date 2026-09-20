use crate::automation::actions::parse_note_sequence_number;
use crate::automation::error::AutomationRuleError;
use crate::automation::matching::{PreparedConditionSet, number_matches};
use crate::automation::model::{
    ArtistOperator, AutomationRuleDraft, FileTextField, GenerationNumberField, GenerationTextField,
    GroupOperator, ImageDimensionField, ImageOrientation, NoteOperator, NumericComparison,
    NumericOperator, PromptActionField, PromptOperator, PromptScope, RuleAction, RuleCondition,
    RuleConditionGroup, RuleConditionSet, RuleExecutionTrigger, RuleMatchMode, RuleRow,
    RuleSourceType, TagOperator, TextOperator, VibeOperator,
};
use crate::automation::text::normalized_strings;
use crate::db::Database;
use crate::db::automation_rules::transfer::{
    AUTOMATION_RULE_FILE_FORMAT, AUTOMATION_RULE_FILE_VERSION,
};
use crate::storage::rule_files::{MAX_AUTOMATION_RULE_FILE_BYTES, parse_automation_rule_text};
use rusqlite::params;

use crate::db::{NewRow, RowSelection, SourceType};

fn comparison(operator: NumericOperator, value: f64, second: Option<f64>) -> NumericComparison {
    NumericComparison {
        operator,
        value,
        second_value: second,
    }
}

fn condition_set(condition: RuleCondition) -> RuleConditionSet {
    RuleConditionSet {
        mode: RuleMatchMode::Any,
        negate: false,
        groups: vec![RuleConditionGroup {
            mode: RuleMatchMode::All,
            conditions: vec![condition],
        }],
    }
}

fn draft(name: &str, condition: RuleCondition, actions: Vec<RuleAction>) -> AutomationRuleDraft {
    AutomationRuleDraft {
        name: name.into(),
        description: String::new(),
        enabled: true,
        run_on_import: true,
        run_on_update: false,
        conditions: condition_set(condition),
        actions,
    }
}

fn sample_row() -> RuleRow {
    RuleRow {
        id: 1,
        positive_prompt: Some("1.2::girl::, white long hair, blue eyes".into()),
        character_prompt: Some("hair flower(white flower), artist:alice".into()),
        negative_prompt: Some("low quality, bad hands".into()),
        artists: Some("artist:alice\nartist:bob".into()),
        note: Some("favorite portrait".into()),
        group_id: Some(7),
        tags: ["花绘".to_owned(), "OC".to_owned()].into_iter().collect(),
        image_path: Some(r"D:\Pictures\sample.png".into()),
        source_size: Some(2_048),
        metadata_failed: false,
        vibe_count: 2,
        image_width: Some(832),
        image_height: Some(1216),
        generation_model: Some("NovelAI Diffusion V4.5 Full".into()),
        generation_sampler: Some("k_euler_ancestral".into()),
        generation_steps: Some(28),
        generation_seed: Some("18446744073709551615".into()),
        generation_scale: Some(5.5),
        generation_cfg_rescale: Some(0.2),
        generation_noise_schedule: Some("karras".into()),
        source_type: "folder".into(),
        source_path: r"D:\Imports\July".into(),
    }
}

fn assert_matches(row: &RuleRow, condition: RuleCondition) {
    let set = condition_set(condition);
    assert!(PreparedConditionSet::new(&set).unwrap().matches(row));
}

fn append_row(database: &mut Database, identity: &str, prompt: &str) -> i64 {
    let outcome = database
        .append_batch(
            SourceType::Folder,
            r"D:\Imports",
            &[NewRow {
                source_ordinal: 1,
                identity: identity.into(),
                positive_prompt: Some(prompt.into()),
                character_prompt: Some("old character, bare_artist".into()),
                negative_prompt: Some("remove me, keep me".into()),
                image_path: Some(format!(r"D:\Imports\{identity}.png")),
                source_size: Some(2048),
                vibe_reference_count: 2,
                image_width: Some(832),
                image_height: Some(1216),
                generation_model: Some("NovelAI V4.5".into()),
                generation_sampler: Some("k_euler".into()),
                generation_steps: Some(28),
                generation_seed: Some("12345678901234567890".into()),
                generation_scale: Some(5.0),
                generation_cfg_rescale: Some(0.2),
                generation_noise_schedule: Some("karras".into()),
                ..NewRow::default()
            }],
            |_| Ok(()),
        )
        .unwrap();
    outcome.added_row_ids[0]
}

#[test]
fn every_condition_family_matches_expected_row() {
    let row = sample_row();
    for condition in [
        RuleCondition::Prompt {
            scope: PromptScope::PositiveAndCharacter,
            operator: PromptOperator::ContainsAll,
            value: "girl, white long hair, hair flower(white flower)".into(),
            case_sensitive: false,
        },
        RuleCondition::Prompt {
            scope: PromptScope::Negative,
            operator: PromptOperator::ContainsAny,
            value: "missing, bad hands".into(),
            case_sensitive: false,
        },
        RuleCondition::Prompt {
            scope: PromptScope::Positive,
            operator: PromptOperator::ContainsNone,
            value: "red hair, green eyes".into(),
            case_sensitive: false,
        },
        RuleCondition::Prompt {
            scope: PromptScope::All,
            operator: PromptOperator::TextContains,
            value: "BAD HANDS".into(),
            case_sensitive: false,
        },
        RuleCondition::Prompt {
            scope: PromptScope::Character,
            operator: PromptOperator::TextEquals,
            value: "hair flower(white flower), artist:alice".into(),
            case_sensitive: true,
        },
        RuleCondition::Prompt {
            scope: PromptScope::Positive,
            operator: PromptOperator::Regex,
            value: r"white\s+long\s+hair".into(),
            case_sensitive: false,
        },
        RuleCondition::Tag {
            operator: TagOperator::HasAll,
            tags: vec!["花绘".into(), "OC".into()],
        },
        RuleCondition::Tag {
            operator: TagOperator::HasAny,
            tags: vec!["missing".into(), "OC".into()],
        },
        RuleCondition::Tag {
            operator: TagOperator::HasNone,
            tags: vec!["missing".into()],
        },
        RuleCondition::Group {
            operator: GroupOperator::Is,
            group_id: Some(7),
        },
        RuleCondition::Group {
            operator: GroupOperator::IsNot,
            group_id: Some(8),
        },
        RuleCondition::Artist {
            operator: ArtistOperator::ContainsAny,
            artists: vec!["alice".into()],
        },
        RuleCondition::Artist {
            operator: ArtistOperator::ContainsNone,
            artists: vec!["carol".into()],
        },
        RuleCondition::Artist {
            operator: ArtistOperator::IsMultiple,
            artists: vec![],
        },
        RuleCondition::Note {
            operator: NoteOperator::Contains,
            value: "PORTRAIT".into(),
            case_sensitive: false,
        },
        RuleCondition::FileText {
            field: FileTextField::FileName,
            operator: TextOperator::Equals,
            value: "sample.png".into(),
            case_sensitive: false,
        },
        RuleCondition::FileText {
            field: FileTextField::OriginalPath,
            operator: TextOperator::Regex,
            value: r"Pictures\\sample\.png$".into(),
            case_sensitive: false,
        },
        RuleCondition::FileText {
            field: FileTextField::ImportSource,
            operator: TextOperator::Contains,
            value: "imports".into(),
            case_sensitive: false,
        },
        RuleCondition::FileSize {
            comparison: comparison(NumericOperator::GreaterOrEqual, 2048.0, None),
        },
        RuleCondition::SourceType {
            source_type: RuleSourceType::Folder,
            negate: false,
        },
        RuleCondition::Vibe {
            operator: VibeOperator::HasAny,
            comparison: None,
        },
        RuleCondition::Vibe {
            operator: VibeOperator::Count,
            comparison: Some(comparison(NumericOperator::Equal, 2.0, None)),
        },
        RuleCondition::Metadata { parsed: true },
        RuleCondition::ImageDimension {
            field: ImageDimensionField::Width,
            comparison: comparison(NumericOperator::Equal, 832.0, None),
        },
        RuleCondition::ImageDimension {
            field: ImageDimensionField::Height,
            comparison: comparison(NumericOperator::GreaterThan, 1000.0, None),
        },
        RuleCondition::ImageDimension {
            field: ImageDimensionField::AspectRatio,
            comparison: comparison(NumericOperator::Between, 0.68, Some(0.69)),
        },
        RuleCondition::Orientation {
            orientation: ImageOrientation::Portrait,
            negate: false,
        },
        RuleCondition::GenerationText {
            field: GenerationTextField::Model,
            operator: TextOperator::Contains,
            value: "v4.5".into(),
            case_sensitive: false,
        },
        RuleCondition::GenerationText {
            field: GenerationTextField::Sampler,
            operator: TextOperator::Equals,
            value: "k_euler_ancestral".into(),
            case_sensitive: true,
        },
        RuleCondition::GenerationText {
            field: GenerationTextField::NoiseSchedule,
            operator: TextOperator::Regex,
            value: "^kar+as$".into(),
            case_sensitive: false,
        },
        RuleCondition::GenerationText {
            field: GenerationTextField::Seed,
            operator: TextOperator::Equals,
            value: "18446744073709551615".into(),
            case_sensitive: true,
        },
        RuleCondition::GenerationNumber {
            field: GenerationNumberField::Steps,
            comparison: comparison(NumericOperator::Equal, 28.0, None),
        },
        RuleCondition::GenerationNumber {
            field: GenerationNumberField::Scale,
            comparison: comparison(NumericOperator::LessOrEqual, 5.5, None),
        },
        RuleCondition::GenerationNumber {
            field: GenerationNumberField::CfgRescale,
            comparison: comparison(NumericOperator::NotEqual, 0.0, None),
        },
    ] {
        assert_matches(&row, condition);
    }

    let mut empty = row.clone();
    empty.tags.clear();
    empty.group_id = None;
    empty.artists = None;
    empty.note = Some("  ".into());
    empty.vibe_count = 0;
    empty.source_type = "archive".into();
    assert_matches(
        &empty,
        RuleCondition::Tag {
            operator: TagOperator::IsEmpty,
            tags: vec![],
        },
    );
    assert_matches(
        &empty,
        RuleCondition::Group {
            operator: GroupOperator::IsEmpty,
            group_id: None,
        },
    );
    assert_matches(
        &empty,
        RuleCondition::Artist {
            operator: ArtistOperator::IsEmpty,
            artists: vec![],
        },
    );
    assert_matches(
        &empty,
        RuleCondition::Note {
            operator: NoteOperator::IsEmpty,
            value: String::new(),
            case_sensitive: false,
        },
    );
    assert_matches(
        &empty,
        RuleCondition::Vibe {
            operator: VibeOperator::HasNone,
            comparison: None,
        },
    );
    assert_matches(
        &empty,
        RuleCondition::SourceType {
            source_type: RuleSourceType::Folder,
            negate: true,
        },
    );
    empty.artists = Some("artist:alice".into());
    assert_matches(
        &empty,
        RuleCondition::Artist {
            operator: ArtistOperator::IsSingle,
            artists: vec![],
        },
    );
    empty.metadata_failed = true;
    assert_matches(&empty, RuleCondition::Metadata { parsed: false });
    assert_matches(
        &empty,
        RuleCondition::Orientation {
            orientation: ImageOrientation::Landscape,
            negate: true,
        },
    );
}

#[test]
fn condition_groups_support_and_or_and_whole_expression_negation() {
    let row = sample_row();
    let group_a = RuleConditionGroup {
        mode: RuleMatchMode::All,
        conditions: vec![
            RuleCondition::Prompt {
                scope: PromptScope::PositiveAndCharacter,
                operator: PromptOperator::ContainsAll,
                value: "girl, white long hair, blue eyes, hair flower(white flower)".into(),
                case_sensitive: false,
            },
            RuleCondition::Tag {
                operator: TagOperator::HasAny,
                tags: vec!["OC".into()],
            },
        ],
    };
    let group_b = RuleConditionGroup {
        mode: RuleMatchMode::All,
        conditions: vec![RuleCondition::Prompt {
            scope: PromptScope::Positive,
            operator: PromptOperator::ContainsAll,
            value: "red hair, green eyes".into(),
            case_sensitive: false,
        }],
    };
    let set = RuleConditionSet {
        mode: RuleMatchMode::Any,
        negate: false,
        groups: vec![group_a, group_b],
    };
    assert!(PreparedConditionSet::new(&set).unwrap().matches(&row));

    let negated = RuleConditionSet {
        negate: true,
        ..set
    };
    assert!(!PreparedConditionSet::new(&negated).unwrap().matches(&row));
}

#[test]
fn prompt_matching_is_exact_per_tag_and_has_no_character_aliases() {
    let row = sample_row();
    for value in ["1girl", "long hair", "blue eye"] {
        let set = condition_set(RuleCondition::Prompt {
            scope: PromptScope::Positive,
            operator: PromptOperator::ContainsAll,
            value: value.into(),
            case_sensitive: false,
        });
        assert!(!PreparedConditionSet::new(&set).unwrap().matches(&row));
    }
}

#[test]
fn full_width_comma_safely_splits_all_list_style_inputs() {
    assert_eq!(
        normalized_strings(&["alice，bob, carol\ndave".into()]),
        vec!["alice", "bob", "carol", "dave"]
    );

    let mut database = Database::open_in_memory().unwrap();
    let row_id = append_row(&mut database, "full-width-comma", "girl, blue eyes");
    let rule = database
        .create_automation_rule(&draft(
            "全角逗号防呆",
            RuleCondition::Prompt {
                scope: PromptScope::Positive,
                operator: PromptOperator::ContainsAll,
                value: "girl，blue eyes".into(),
                case_sensitive: false,
            },
            vec![RuleAction::AddTags {
                tags: vec!["人物，蓝眼".into()],
            }],
        ))
        .unwrap();

    let result = database.run_automation_rule_on_library(rule.id).unwrap();
    assert_eq!(result.changed_rows, 1);
    let tags = {
        let mut statement = database
            .connection
            .prepare(
                "SELECT tags.name FROM row_tags
                 JOIN tags ON tags.id = row_tags.tag_id
                 WHERE row_tags.row_id = ?1 ORDER BY tags.name",
            )
            .unwrap();
        statement
            .query_map([row_id], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(tags, vec!["人物", "蓝眼"]);
}

#[test]
fn every_numeric_operator_has_defined_boundary_behavior() {
    assert!(number_matches(
        5.0,
        &comparison(NumericOperator::Equal, 5.0, None)
    ));
    assert!(number_matches(
        5.0,
        &comparison(NumericOperator::NotEqual, 4.0, None)
    ));
    assert!(number_matches(
        5.0,
        &comparison(NumericOperator::GreaterThan, 4.0, None)
    ));
    assert!(number_matches(
        5.0,
        &comparison(NumericOperator::GreaterOrEqual, 5.0, None)
    ));
    assert!(number_matches(
        5.0,
        &comparison(NumericOperator::LessThan, 6.0, None)
    ));
    assert!(number_matches(
        5.0,
        &comparison(NumericOperator::LessOrEqual, 5.0, None)
    ));
    assert!(number_matches(
        5.0,
        &comparison(NumericOperator::Between, 5.0, Some(9.0))
    ));
    assert!(number_matches(
        5.0,
        &comparison(NumericOperator::Between, 9.0, Some(5.0))
    ));
}

#[test]
fn rule_crud_persists_definition_and_normalizes_order_after_delete() {
    let mut database = Database::open_in_memory().unwrap();
    assert!(database.list_automation_rules().unwrap().is_empty());
    let condition = RuleCondition::Metadata { parsed: true };
    let first = database
        .create_automation_rule(&draft(
            "第一条",
            condition.clone(),
            vec![RuleAction::AddTags {
                tags: vec!["A".into()],
            }],
        ))
        .unwrap();
    let second = database
        .create_automation_rule(&draft(
            "第二条",
            condition,
            vec![RuleAction::AddTags {
                tags: vec!["B".into()],
            }],
        ))
        .unwrap();

    database
        .reorder_automation_rules(&[second.id, first.id])
        .unwrap();
    assert_eq!(
        database
            .list_automation_rules()
            .unwrap()
            .iter()
            .map(|rule| rule.name.as_str())
            .collect::<Vec<_>>(),
        vec!["第二条", "第一条"]
    );
    database
        .set_automation_rule_enabled(second.id, false)
        .unwrap();
    assert!(!database.list_automation_rules().unwrap()[0].enabled);
    assert!(database.delete_automation_rule(second.id).unwrap());
    let remaining = database.list_automation_rules().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].position, 0);
}

#[test]
fn all_mutating_actions_apply_in_sequence_and_manual_preview_is_non_mutating() {
    let mut database = Database::open_in_memory().unwrap();
    let row_id = append_row(&mut database, "actions", "target, bare_artist");
    database.create_tag("旧标签").unwrap();
    database
        .set_tags_for_row(row_id, &["旧标签".into()])
        .unwrap();
    let group = database.create_group("目标分组").unwrap();
    let rule = database
        .create_automation_rule(&draft(
            "动作全集",
            RuleCondition::Prompt {
                scope: PromptScope::Positive,
                operator: PromptOperator::ContainsAll,
                value: "target".into(),
                case_sensitive: false,
            },
            vec![
                RuleAction::AddTags {
                    tags: vec!["新标签".into()],
                },
                RuleAction::RemoveTags {
                    tags: vec!["旧标签".into()],
                },
                RuleAction::SetGroup {
                    group_id: group.id,
                    only_if_ungrouped: false,
                },
                RuleAction::AppendPrompt {
                    field: PromptActionField::Positive,
                    value: "added prompt".into(),
                },
                RuleAction::DeletePromptTags {
                    field: PromptActionField::Negative,
                    value: "remove me".into(),
                },
                RuleAction::ReplacePrompt {
                    field: PromptActionField::Character,
                    find: "old character".into(),
                    replace: "new character".into(),
                    case_sensitive: true,
                },
                RuleAction::PrefixArtist {
                    artists: vec!["bare_artist".into()],
                },
                RuleAction::SetNote {
                    value: "base".into(),
                },
                RuleAction::AppendNote {
                    value: "tail".into(),
                    separator: " | ".into(),
                },
            ],
        ))
        .unwrap();

    let preview = database.preview_automation_rule(rule.id).unwrap();
    assert_eq!(preview.matched_rows, 1);
    assert_eq!(preview.rows_needing_changes, 1);
    let before_note: Option<String> = database
        .connection
        .query_row("SELECT note FROM rows WHERE id = ?1", [row_id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(before_note, None);

    let result = database.run_automation_rule_on_library(rule.id).unwrap();
    assert_eq!(result.changed_rows, 1);
    assert_eq!(result.reports[0].actions_changed, 9);
    let stored: (String, String, String, Option<String>, Option<i64>) = database
        .connection
        .query_row(
            "SELECT positive_prompt, character_prompt, negative_prompt, note, group_id
             FROM rows WHERE id = ?1",
            [row_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert!(stored.0.contains("added prompt"));
    assert!(stored.0.contains("artist:bare_artist"));
    assert!(stored.1.contains("new character"));
    assert!(stored.1.contains("artist:bare_artist"));
    assert_eq!(stored.2, "keep me");
    assert_eq!(stored.3.as_deref(), Some("base | tail"));
    assert_eq!(stored.4, Some(group.id));
    let tags = database
        .list_selection_tags(&RowSelection::Explicit {
            row_ids: vec![row_id],
        })
        .unwrap();
    assert_eq!(
        tags.iter()
            .filter(|tag| tag.selected_rows > 0)
            .map(|tag| tag.name.as_str())
            .collect::<Vec<_>>(),
        vec!["新标签"]
    );

    let clear = database
        .create_automation_rule(&draft(
            "清理",
            RuleCondition::Tag {
                operator: TagOperator::HasAny,
                tags: vec!["新标签".into()],
            },
            vec![RuleAction::ClearGroup, RuleAction::ClearNote],
        ))
        .unwrap();
    database.run_automation_rule_on_library(clear.id).unwrap();
    let cleared: (Option<i64>, Option<String>) = database
        .connection
        .query_row(
            "SELECT group_id, note FROM rows WHERE id = ?1",
            [row_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(cleared, (None, None));
}

#[test]
fn unsaved_draft_preview_is_read_only_and_matches_saved_preview() {
    let mut database = Database::open_in_memory().unwrap();
    let row_id = append_row(&mut database, "draft-preview", "target, other");
    let rule_draft = draft(
        "未保存草稿",
        RuleCondition::Prompt {
            scope: PromptScope::Positive,
            operator: PromptOperator::ContainsAll,
            value: "target".into(),
            case_sensitive: false,
        },
        vec![RuleAction::AddTags {
            tags: vec!["草稿标签".into()],
        }],
    );

    // 草稿预览：不要求先入库
    let preview = database.preview_automation_rule_draft(&rule_draft).unwrap();
    assert_eq!(preview.matched_rows, 1);
    assert_eq!(preview.rows_needing_changes, 1);
    assert_eq!(preview.sample_row_ids, vec![row_id]);

    // 只读：不产生任何规则记录，也不修改行
    let rules = database.list_automation_rules().unwrap();
    assert!(rules.is_empty());
    let tags = database
        .list_selection_tags(&RowSelection::Explicit {
            row_ids: vec![row_id],
        })
        .unwrap();
    assert!(tags.iter().all(|tag| tag.selected_rows == 0));

    // 与保存后的预览结果一致
    let saved = database.create_automation_rule(&rule_draft).unwrap();
    let saved_preview = database.preview_automation_rule(saved.id).unwrap();
    assert_eq!(saved_preview.matched_rows, preview.matched_rows);
    assert_eq!(
        saved_preview.rows_needing_changes,
        preview.rows_needing_changes
    );

    // 校验仍然生效：空值草稿被拒绝
    let invalid = draft(
        "无效草稿",
        RuleCondition::Prompt {
            scope: PromptScope::Positive,
            operator: PromptOperator::ContainsAll,
            value: "  ".into(),
            case_sensitive: false,
        },
        vec![RuleAction::AddTags {
            tags: vec!["x".into()],
        }],
    );
    assert!(database.preview_automation_rule_draft(&invalid).is_err());
}

#[test]
fn set_note_sequence_continues_from_library_max_and_skips_existing() {
    let mut database = Database::open_in_memory().unwrap();
    let first = append_row(&mut database, "seq-1", "watercolor");
    let existing = append_row(&mut database, "seq-2", "watercolor");
    let later = append_row(&mut database, "seq-3", "watercolor");
    let skip_prompt = append_row(&mut database, "seq-4", "oil painting");
    database.update_note(existing, "水彩7").unwrap();
    database.update_note(first, "草稿").unwrap();

    let rule_draft = draft(
        "水彩编号",
        RuleCondition::Prompt {
            scope: PromptScope::Positive,
            operator: PromptOperator::ContainsAll,
            value: "watercolor".into(),
            case_sensitive: false,
        },
        vec![RuleAction::SetNoteSequence {
            prefix: "水彩".into(),
        }],
    );
    let preview = database.preview_automation_rule_draft(&rule_draft).unwrap();
    assert_eq!(preview.matched_rows, 3);
    assert_eq!(preview.rows_needing_changes, 2);

    let saved = database.create_automation_rule(&rule_draft).unwrap();
    let result = database.run_automation_rule_on_library(saved.id).unwrap();
    assert_eq!(result.changed_rows, 2);
    assert_eq!(
        database.get_rows_by_ids(&[first]).unwrap()[0]
            .note
            .as_deref(),
        Some("水彩8")
    );
    assert_eq!(
        database.get_rows_by_ids(&[existing]).unwrap()[0]
            .note
            .as_deref(),
        Some("水彩7")
    );
    assert_eq!(
        database.get_rows_by_ids(&[later]).unwrap()[0]
            .note
            .as_deref(),
        Some("水彩9")
    );
    assert_eq!(
        database.get_rows_by_ids(&[skip_prompt]).unwrap()[0].note,
        None
    );

    let rerun = database.run_automation_rule_on_library(saved.id).unwrap();
    assert_eq!(rerun.changed_rows, 0);
    assert_eq!(parse_note_sequence_number("水彩10", "水彩"), Some(10));
    assert_eq!(parse_note_sequence_number("水彩", "水彩"), None);
    assert_eq!(parse_note_sequence_number("草稿", "水彩"), None);
    assert_eq!(parse_note_sequence_number("水彩7稿", "水彩"), None);
}

#[test]
fn only_ungrouped_action_skips_existing_group() {
    let mut database = Database::open_in_memory().unwrap();
    let row_id = append_row(&mut database, "grouped", "target");
    let original = database.create_group("原分组").unwrap();
    let target = database.create_group("目标分组").unwrap();
    database
        .assign_rows_to_group(
            &RowSelection::Explicit {
                row_ids: vec![row_id],
            },
            original.id,
        )
        .unwrap();
    let rule = database
        .create_automation_rule(&draft(
            "不抢已有分组",
            RuleCondition::Metadata { parsed: true },
            vec![RuleAction::SetGroup {
                group_id: target.id,
                only_if_ungrouped: true,
            }],
        ))
        .unwrap();
    let result = database.run_automation_rule_on_library(rule.id).unwrap();
    assert_eq!(result.changed_rows, 0);
    let group_id: i64 = database
        .connection
        .query_row("SELECT group_id FROM rows WHERE id = ?1", [row_id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(group_id, original.id);
}

#[test]
fn stop_processing_only_removes_matching_rows_from_later_rules() {
    let mut database = Database::open_in_memory().unwrap();
    let stopped = append_row(&mut database, "stopped", "stop, target");
    let continuing = append_row(&mut database, "continuing", "target");
    database
        .create_automation_rule(&draft(
            "停止一张",
            RuleCondition::Prompt {
                scope: PromptScope::Positive,
                operator: PromptOperator::ContainsAll,
                value: "stop".into(),
                case_sensitive: false,
            },
            vec![
                RuleAction::AddTags {
                    tags: vec!["先执行".into()],
                },
                RuleAction::StopProcessing,
            ],
        ))
        .unwrap();
    database
        .create_automation_rule(&draft(
            "后续规则",
            RuleCondition::Prompt {
                scope: PromptScope::Positive,
                operator: PromptOperator::ContainsAll,
                value: "target".into(),
                case_sensitive: false,
            },
            vec![RuleAction::AddTags {
                tags: vec!["后执行".into()],
            }],
        ))
        .unwrap();

    let result = database
        .execute_automation_rules(RuleExecutionTrigger::Import, &[stopped, continuing])
        .unwrap();
    assert_eq!(result.reports.len(), 2);
    assert_eq!(result.reports[0].stopped_rows, 1);
    assert_eq!(result.reports[1].scanned_rows, 1);
    assert!(row_has_tag(&database, stopped, "先执行"));
    assert!(!row_has_tag(&database, stopped, "后执行"));
    assert!(row_has_tag(&database, continuing, "后执行"));
}

#[test]
fn broken_rule_is_reported_and_later_rules_continue() {
    let mut database = Database::open_in_memory().unwrap();
    let row_id = append_row(&mut database, "errors", "target");
    let group = database.create_group("即将删除").unwrap();
    let broken = database
        .create_automation_rule(&draft(
            "失效规则",
            RuleCondition::Metadata { parsed: true },
            vec![RuleAction::SetGroup {
                group_id: group.id,
                only_if_ungrouped: false,
            }],
        ))
        .unwrap();
    database.delete_group(group.id).unwrap();
    database
        .create_automation_rule(&draft(
            "仍然执行",
            RuleCondition::Metadata { parsed: true },
            vec![RuleAction::AddTags {
                tags: vec!["成功".into()],
            }],
        ))
        .unwrap();

    let result = database
        .execute_automation_rules(RuleExecutionTrigger::Import, &[row_id])
        .unwrap();
    assert_eq!(result.reports.len(), 2);
    assert_eq!(result.reports[0].rule_id, broken.id);
    assert!(result.reports[0].error.is_some());
    assert_eq!(result.reports[1].changed_rows, 1);
    assert!(row_has_tag(&database, row_id, "成功"));
}

#[test]
fn trigger_flags_are_independent() {
    let mut database = Database::open_in_memory().unwrap();
    let row_id = append_row(&mut database, "triggers", "target");
    let mut import_draft = draft(
        "仅导入",
        RuleCondition::Metadata { parsed: true },
        vec![RuleAction::AddTags {
            tags: vec!["import".into()],
        }],
    );
    import_draft.run_on_update = false;
    database.create_automation_rule(&import_draft).unwrap();
    let mut update_draft = draft(
        "仅更新",
        RuleCondition::Metadata { parsed: true },
        vec![RuleAction::AddTags {
            tags: vec!["update".into()],
        }],
    );
    update_draft.run_on_import = false;
    update_draft.run_on_update = true;
    database.create_automation_rule(&update_draft).unwrap();

    database
        .execute_automation_rules(RuleExecutionTrigger::Import, &[row_id])
        .unwrap();
    assert!(row_has_tag(&database, row_id, "import"));
    assert!(!row_has_tag(&database, row_id, "update"));
    database
        .execute_automation_rules(RuleExecutionTrigger::Update, &[row_id])
        .unwrap();
    assert!(row_has_tag(&database, row_id, "update"));
}

#[test]
fn rule_json_round_trip_uses_names_and_imports_disabled() {
    let mut source = Database::open_in_memory().unwrap();
    let group = source.create_group("角色归档").unwrap();
    source.create_tag("人物").unwrap();
    let saved = source
        .create_automation_rule(&draft(
            "角色归档规则",
            RuleCondition::Group {
                operator: GroupOperator::Is,
                group_id: Some(group.id),
            },
            vec![
                RuleAction::AddTags {
                    tags: vec!["人物".into()],
                },
                RuleAction::SetGroup {
                    group_id: group.id,
                    only_if_ungrouped: true,
                },
            ],
        ))
        .unwrap();

    let document = source.export_automation_rule_document(&[saved.id]).unwrap();
    let serialized = serde_json::to_string(&document).unwrap();
    assert!(serialized.contains("groupName"));
    assert!(serialized.contains("角色归档"));
    assert!(!serialized.contains("groupId"));

    let mut destination = Database::open_in_memory().unwrap();
    destination
        .create_automation_rule(&draft(
            "角色归档规则",
            RuleCondition::Metadata { parsed: true },
            vec![RuleAction::ClearNote],
        ))
        .unwrap();
    let inspection = destination
        .inspect_automation_rule_document(&document, "test-hash".into())
        .unwrap();
    assert_eq!(inspection.rule_count, 1);
    assert_eq!(inspection.content_hash, "test-hash");
    assert_eq!(inspection.missing_tags, vec!["人物"]);
    assert_eq!(inspection.missing_groups, vec!["角色归档"]);
    assert_eq!(inspection.renamed_rules, 1);
    assert_eq!(inspection.rules[0].imported_name, "角色归档规则（导入）");

    let result = destination
        .import_automation_rule_document(&document)
        .unwrap();
    assert_eq!(result.imported_rules, 1);
    assert_eq!(result.created_tags, 1);
    assert_eq!(result.created_groups, 1);
    assert_eq!(result.renamed_rules, 1);
    let imported = destination
        .list_automation_rules()
        .unwrap()
        .into_iter()
        .find(|rule| rule.id == result.imported_rule_ids[0])
        .unwrap();
    assert!(!imported.enabled);
    assert_eq!(imported.name, "角色归档规则（导入）");
    let imported_group_id: i64 = destination
        .connection
        .query_row(
            "SELECT id FROM groups WHERE name = '角色归档'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(imported.actions.iter().any(|action| matches!(
        action,
        RuleAction::SetGroup { group_id, only_if_ungrouped: true }
            if *group_id == imported_group_id
    )));
}

#[test]
fn automation_rule_text_accepts_plain_json_and_one_ai_code_block() {
    let mut source = Database::open_in_memory().unwrap();
    let saved = source
        .create_automation_rule(&draft(
            "粘贴导入测试",
            RuleCondition::Metadata { parsed: true },
            vec![RuleAction::ClearNote],
        ))
        .unwrap();
    let exported = source.export_automation_rule_document(&[saved.id]).unwrap();
    let plain = serde_json::to_string_pretty(&exported).unwrap();

    let (plain_document, plain_hash) = parse_automation_rule_text(&plain).unwrap();
    let ai_reply = format!("规则如下：\n\n```JSON\n{plain}\n```\n\n请先预览再启用。");
    let (block_document, block_hash) = parse_automation_rule_text(&ai_reply).unwrap();

    assert_eq!(plain_document, exported);
    assert_eq!(block_document, exported);
    assert_ne!(plain_hash, block_hash);
    assert_eq!(plain_hash.len(), 64);
    let destination = Database::open_in_memory().unwrap();
    let inspection = destination
        .inspect_automation_rule_document(&block_document, block_hash)
        .unwrap();
    assert_eq!(inspection.rule_count, 1);
    assert_eq!(inspection.rules[0].name, "粘贴导入测试");
}

#[test]
fn automation_rule_text_rejects_unsafe_or_ambiguous_input() {
    for input in [
        "   \n\t",
        "```json\n{}",
        "```javascript\n{}\n```",
        "```json\n{}\n```\n```json\n{}\n```",
        "```json\n\n```",
    ] {
        assert!(matches!(
            parse_automation_rule_text(input),
            Err(AutomationRuleError::InvalidRuleFile(_))
        ));
    }

    let oversized = "x".repeat(MAX_AUTOMATION_RULE_FILE_BYTES as usize + 1);
    assert!(matches!(
        parse_automation_rule_text(&oversized),
        Err(AutomationRuleError::InvalidRuleFile(_))
    ));
}

#[test]
fn rule_json_import_rejects_foreign_json_and_raw_group_ids() {
    let database = Database::open_in_memory().unwrap();
    let unrelated = serde_json::json!({ "images": {} });
    assert!(matches!(
        database.inspect_automation_rule_document(&unrelated, "hash".into()),
        Err(AutomationRuleError::InvalidRuleFile(_))
    ));

    let raw_ids = serde_json::json!({
        "format": AUTOMATION_RULE_FILE_FORMAT,
        "version": AUTOMATION_RULE_FILE_VERSION,
        "rules": [{
            "name": "错误分组引用",
            "description": "",
            "enabled": true,
            "runOnImport": true,
            "runOnUpdate": false,
            "conditions": {
                "mode": "any",
                "negate": false,
                "groups": [{
                    "mode": "all",
                    "conditions": [{ "type": "metadata", "parsed": true }]
                }]
            },
            "actions": [{
                "type": "setGroup",
                "groupId": 99,
                "onlyIfUngrouped": false
            }]
        }]
    });
    assert!(matches!(
        database.inspect_automation_rule_document(&raw_ids, "hash".into()),
        Err(AutomationRuleError::InvalidRuleFile(_))
    ));
}

fn row_has_tag(database: &Database, row_id: i64, tag: &str) -> bool {
    database
        .connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM row_tags JOIN tags ON tags.id = row_tags.tag_id
                WHERE row_tags.row_id = ?1 AND tags.name = ?2 COLLATE BINARY
             )",
            params![row_id, tag],
            |row| row.get(0),
        )
        .unwrap()
}
