use super::types::{ImageImportError, ImageImportStage};
use crate::db::{NewRow, SourceType};
use crate::pipeline::{metadata_fingerprint, png_text};
use crate::storage::DataDirectory;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use std::io::Write;

use flate2::{Compression, write::GzEncoder};

use crate::db::{
    AutomationRuleDraft, GenerationNumberField, GenerationTextField, ImageDimensionField,
    NumericComparison, NumericOperator, PromptOperator, PromptScope, RowSelection, RuleAction,
    RuleCondition, RuleConditionGroup, RuleConditionSet, RuleMatchMode, TagOperator, TextOperator,
};
use crate::storage::test_fixtures::{metadata_png_bytes, write_metadata_png};

/// 仅含文本元数据的最小 PNG（签名 + tEXt + IEND，CRC 占位即可，
/// 文本读取器跳过 CRC 校验）。
pub(super) fn create_metadata_png(path: &Path, description: &str) {
    create_text_png(path, "Description", description);
}

pub(super) fn create_text_png(path: &Path, keyword: &str, text: &str) {
    let mut data = keyword.as_bytes().to_vec();
    data.push(0);
    data.extend(text.as_bytes());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    for (chunk_type, payload) in [(b"tEXt", data.as_slice()), (b"IEND", &[][..])] {
        png.extend((payload.len() as u32).to_be_bytes());
        png.extend(chunk_type);
        png.extend(payload);
        png.extend(0_u32.to_be_bytes());
    }
    fs::write(path, png).unwrap();
}

pub(super) fn create_stealth_png(path: &Path, description: &str, comment: &str) {
    let metadata = serde_json::json!({
        "Description": description,
        "Comment": comment,
        "Source": "NovelAI"
    });
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(serde_json::to_string(&metadata).unwrap().as_bytes())
        .unwrap();
    let compressed = encoder.finish().unwrap();
    let mut payload = b"stealth_pngcomp".to_vec();
    payload.extend_from_slice(&u32::try_from(compressed.len() * 8).unwrap().to_be_bytes());
    payload.extend_from_slice(&compressed);

    let mut image = image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 20, 30, 255]));
    let height = image.height() as usize;
    for (position, bit) in payload
        .iter()
        .flat_map(|byte| (0..8).map(move |shift| (byte >> (7 - shift)) & 1))
        .enumerate()
    {
        let x = position / height;
        let y = position % height;
        let pixel = image.get_pixel_mut(x as u32, y as u32);
        pixel.0[3] = (pixel.0[3] & 0xfe) | bit;
    }
    image.save(path).unwrap();
}

#[test]
pub(super) fn imports_novelai_metadata_stored_only_in_alpha_channel() {
    let temporary = TemporaryImageImport::new();
    fs::create_dir_all(&temporary.root).unwrap();
    let input = temporary.root.join("stealth-only.png");
    create_stealth_png(
        &input,
        "best quality, artist:stealth",
        r#"{"seed":42,"uc":"bad hands"}"#,
    );
    let directory = temporary.initialize_directory();

    let outcome = directory.import_images(&input, |_| {}).unwrap();

    assert_eq!(outcome.added, 1);
    assert_eq!(outcome.metadata_rejected, 0);
    let row = directory
        .open_database()
        .unwrap()
        .get_rows_by_ids(&[1])
        .unwrap()
        .remove(0);
    assert_eq!(
        row.positive_prompt.as_deref(),
        Some("best quality, artist:stealth")
    );
    assert_eq!(row.negative_prompt.as_deref(), Some("bad hands"));
    assert_eq!(row.artists.as_deref(), Some("artist:stealth"));
}

#[test]
pub(super) fn imports_folder_then_appends_only_new_images() {
    let temporary = TemporaryImageImport::new();
    let input = temporary.root.join("input");
    fs::create_dir_all(input.join("nested")).unwrap();
    create_metadata_png(&input.join("a.png"), "best quality, artist:alpha");
    create_metadata_png(&input.join("nested").join("b.png"), "scenery");
    fs::write(input.join("broken.png"), b"not a png").unwrap();
    let directory = temporary.initialize_directory();

    let outcome = directory.import_images(&input, |_| {}).unwrap();

    assert_eq!(outcome.total_found, 3);
    assert_eq!(outcome.added, 2);
    assert_eq!(outcome.skipped_existing, 0);
    assert_eq!(outcome.metadata_rejected, 1);
    assert_eq!(outcome.rejected_moved, 1);
    assert_eq!(outcome.rejected_move_failures, 0);
    assert_eq!(outcome.source_type, SourceType::Folder);

    let mut database = directory.open_database().unwrap();
    let page = database
        .query_rows(&crate::db::RowQuery {
            offset: 0,
            limit: 10,
            tags: Vec::new(),
            tag_mode: crate::db::TagMatchMode::And,
            dedupe: crate::db::DedupeMode::None,
            single_artist_only: false,
            artist_filter: String::new(),
            has_vibe: false,
            untagged_only: false,
            filters: vec![],
            group_view: false,
            hide_grouped: false,
            search: String::new(),
        })
        .unwrap();
    assert_eq!(page.total_count, 2);
    let first = &page.rows[0];
    assert_eq!(
        first.positive_prompt.as_deref(),
        Some("best quality, artist:alpha")
    );
    assert_eq!(first.artists.as_deref(), Some("artist:alpha"));
    assert_eq!(page.rows[1].artists.as_deref(), Some("scenery"));
    assert!(!first.metadata_failed);
    assert!(first.image_path.as_deref().unwrap().ends_with("a.png"));
    assert!(first.time.is_some());
    assert!(!input.join("broken.png").exists());
    assert_eq!(
        fs::read(temporary.rejected.join("broken.png")).unwrap(),
        b"not a png"
    );

    // 追加 2 张新图后重新导入：只新增 2，已有 2 张跳过。
    create_metadata_png(&input.join("c.png"), "new one");
    create_metadata_png(&input.join("d.png"), "new two");
    let second = directory.import_images(&input, |_| {}).unwrap();
    assert_eq!(second.added, 2);
    assert_eq!(second.skipped_existing, 2);
    assert_eq!(
        directory
            .open_database()
            .unwrap()
            .library_summary()
            .unwrap()
            .row_count,
        4
    );
}

#[test]
pub(super) fn enabled_import_artist_prefix_only_repairs_new_rows_with_library_evidence() {
    let temporary = TemporaryImageImport::new();
    let input = temporary.root.join("artist-prefix-input");
    fs::create_dir_all(&input).unwrap();
    create_metadata_png(&input.join("evidence.png"), "artist:xy, masterpiece");
    create_metadata_png(&input.join("bare.png"), "xy, best quality");
    create_metadata_png(&input.join("unknown.png"), "watermark, scenery");
    let directory = temporary.initialize_directory();
    directory
        .open_database()
        .unwrap()
        .set_auto_artist_prefix_on_import(true)
        .unwrap();

    let outcome = directory.import_images(&input, |_| {}).unwrap();

    assert!(outcome.artist_prefix_enabled);
    assert_eq!(outcome.artist_prefix_scanned_rows, 3);
    assert_eq!(outcome.artist_prefix_changed_rows, 1);
    assert_eq!(outcome.artist_prefix_changed_fields, 1);
    assert_eq!(outcome.artist_prefix_error, None);
    let mut database = directory.open_database().unwrap();
    let page = database
        .query_rows(&crate::db::RowQuery {
            offset: 0,
            limit: 10,
            tags: Vec::new(),
            tag_mode: crate::db::TagMatchMode::And,
            dedupe: crate::db::DedupeMode::None,
            single_artist_only: false,
            artist_filter: String::new(),
            has_vibe: false,
            untagged_only: false,
            filters: vec![],
            group_view: false,
            hide_grouped: false,
            search: String::new(),
        })
        .unwrap();
    let prompts = page
        .rows
        .iter()
        .map(|row| row.positive_prompt.as_deref().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(prompts.contains(&"artist:xy, masterpiece"));
    assert!(prompts.contains(&"artist:xy, best quality"));
    assert!(prompts.contains(&"watermark, scenery"));
}

#[test]
pub(super) fn update_import_changes_only_existing_rows_and_preserves_tags_and_group() {
    let temporary = TemporaryImageImport::new();
    let input = temporary.root.join("update-input");
    fs::create_dir_all(&input).unwrap();
    let existing = input.join("existing.png");
    let rejected_update = input.join("failing.png");
    write_metadata_png(&existing, "old prompt, artist:old");
    write_metadata_png(&rejected_update, "keep this old prompt");
    let directory = temporary.initialize_directory();
    directory.import_images(&input, |_| {}).unwrap();

    let mut database = directory.open_database().unwrap();
    database.create_tag("保留标签").unwrap();
    database.set_tags_for_row(1, &["保留标签".into()]).unwrap();
    let group = database.create_group("保留分组").unwrap();
    database
        .assign_rows_to_group(&RowSelection::Explicit { row_ids: vec![1] }, group.id)
        .unwrap();
    let stored_path = database
        .row_image_locator(1)
        .unwrap()
        .stored_image_path
        .unwrap();
    drop(database);

    fs::write(
        &existing,
        metadata_png_bytes(
            "new base prompt, artist:base",
            Some(
                r#"{"v4_prompt":{"caption":{"char_captions":[{"char_caption":"1girl, artist:character"}]}}}"#,
            ),
        ),
    )
    .unwrap();
    fs::write(&rejected_update, b"not a valid png").unwrap();
    write_metadata_png(&input.join("brand-new.png"), "must not be added");

    let outcome = directory.update_existing_images(&input, |_| {}).unwrap();

    assert_eq!(outcome.total_found, 3);
    assert_eq!(outcome.matched, 2);
    assert_eq!(outcome.updated, 1);
    assert_eq!(outcome.matched_by_identity, 2);
    assert_eq!(outcome.relinked_by_content, 0);
    assert_eq!(outcome.relinked_by_metadata, 0);
    assert_eq!(outcome.ambiguous, 0);
    assert_eq!(outcome.unmatched, 1);
    assert_eq!(outcome.metadata_rejected, 1);
    assert_eq!(outcome.copy_failures, 0);
    let mut database = directory.open_database().unwrap();
    assert_eq!(database.library_summary().unwrap().row_count, 2);
    let rows = database.get_rows_by_ids(&[1, 2]).unwrap();
    assert_eq!(
        rows[0].positive_prompt.as_deref(),
        Some("new base prompt, artist:base")
    );
    assert_eq!(
        rows[0].character_prompt.as_deref(),
        Some("1girl, artist:character")
    );
    assert_eq!(
        rows[0].artists.as_deref(),
        Some("artist:base\nartist:character")
    );
    assert_eq!(rows[0].tags, vec!["保留标签"]);
    assert_eq!(rows[0].group_id, Some(group.id));
    assert_eq!(rows[0].group_name.as_deref(), Some("保留分组"));
    assert_eq!(
        rows[1].positive_prompt.as_deref(),
        Some("keep this old prompt")
    );
    assert_eq!(
        fs::read(directory.root().join(stored_path)).unwrap(),
        fs::read(existing).unwrap()
    );
}

#[test]
pub(super) fn update_import_relinks_moved_original_by_content_hash() {
    let temporary = TemporaryImageImport::new();
    let original_dir = temporary.root.join("original");
    let moved_dir = temporary.root.join("moved");
    fs::create_dir_all(&original_dir).unwrap();
    fs::create_dir_all(&moved_dir).unwrap();
    let original = original_dir.join("same.png");
    let moved = moved_dir.join("renamed.png");
    fs::write(
        &original,
        metadata_png_bytes("artist:moved", Some(r#"{"seed":123,"steps":28}"#)),
    )
    .unwrap();
    let directory = temporary.initialize_directory();
    directory.import_images(&original_dir, |_| {}).unwrap();
    fs::rename(&original, &moved).unwrap();

    let outcome = directory
        .update_existing_images(&moved_dir, |_| {})
        .unwrap();

    assert_eq!(outcome.matched, 1);
    assert_eq!(outcome.updated, 1);
    assert_eq!(outcome.matched_by_identity, 0);
    assert_eq!(outcome.relinked_by_content, 1);
    assert_eq!(outcome.relinked_by_metadata, 0);
    assert_eq!(outcome.ambiguous, 0);
    assert_eq!(outcome.unmatched, 0);
    let locator = directory
        .open_database()
        .unwrap()
        .row_image_locator(1)
        .unwrap();
    assert_eq!(
        locator.image_path.as_deref(),
        Some(moved.to_string_lossy().as_ref())
    );
    assert!(locator.stored_image_is_original);
    assert_eq!(
        fs::read(directory.root().join(locator.stored_image_path.unwrap())).unwrap(),
        fs::read(moved).unwrap()
    );
}

#[test]
pub(super) fn update_import_relinks_reencoded_original_by_complete_metadata() {
    let temporary = TemporaryImageImport::new();
    let original_dir = temporary.root.join("metadata-original");
    let moved_dir = temporary.root.join("metadata-moved");
    fs::create_dir_all(&original_dir).unwrap();
    fs::create_dir_all(&moved_dir).unwrap();
    let original = original_dir.join("old.png");
    let moved = moved_dir.join("new.png");
    let bytes = metadata_png_bytes(
        "artist:metadata",
        Some(r#"{"seed":987654,"steps":28,"sampler":"k_euler"}"#),
    );
    fs::write(&original, &bytes).unwrap();
    let directory = temporary.initialize_directory();
    directory.import_images(&original_dir, |_| {}).unwrap();

    let mut reencoded = bytes;
    reencoded.extend_from_slice(b"harmless trailing bytes");
    fs::write(&moved, reencoded).unwrap();
    fs::remove_file(&original).unwrap();

    let outcome = directory
        .update_existing_images(&moved_dir, |_| {})
        .unwrap();

    assert_eq!(outcome.matched, 1);
    assert_eq!(outcome.updated, 1);
    assert_eq!(outcome.relinked_by_content, 0);
    assert_eq!(outcome.relinked_by_metadata, 1);
    assert_eq!(outcome.ambiguous, 0);
    assert_eq!(outcome.unmatched, 0);
    assert_eq!(
        directory
            .open_database()
            .unwrap()
            .row_image_locator(1)
            .unwrap()
            .image_path
            .as_deref(),
        Some(moved.to_string_lossy().as_ref())
    );
}

#[test]
pub(super) fn update_import_does_not_choose_between_duplicate_metadata_candidates() {
    let temporary = TemporaryImageImport::new();
    let input = temporary.root.join("ambiguous");
    fs::create_dir_all(&input).unwrap();
    let candidate = input.join("candidate.png");
    fs::write(
        &candidate,
        metadata_png_bytes("artist:ambiguous", Some(r#"{"seed":42}"#)),
    )
    .unwrap();
    let chunks = png_text::read_png_text_chunks(&candidate).unwrap();
    let fingerprint = metadata_fingerprint(&chunks).unwrap();
    let directory = temporary.initialize_directory();
    directory
        .open_database()
        .unwrap()
        .append_batch(
            SourceType::Folder,
            r"D:\old",
            &[
                NewRow {
                    source_ordinal: 1,
                    identity: r"file:d:\old\a.png".into(),
                    content_hash: Some("old-content-a".into()),
                    metadata_fingerprint: Some(fingerprint.clone()),
                    image_path: Some(r"D:\old\a.png".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 2,
                    identity: r"file:d:\old\b.png".into(),
                    content_hash: Some("old-content-b".into()),
                    metadata_fingerprint: Some(fingerprint),
                    image_path: Some(r"D:\old\b.png".into()),
                    ..NewRow::default()
                },
            ],
            |_| Ok(()),
        )
        .unwrap();

    let outcome = directory.update_existing_images(&input, |_| {}).unwrap();

    assert_eq!(outcome.matched, 0);
    assert_eq!(outcome.updated, 0);
    assert_eq!(outcome.ambiguous, 1);
    assert_eq!(outcome.unmatched, 0);
    assert_eq!(
        directory
            .open_database()
            .unwrap()
            .library_summary()
            .unwrap()
            .row_count,
        2
    );
}

#[test]
pub(super) fn imports_zip_archive_with_stored_copies_and_skips_reimport() {
    let temporary = TemporaryImageImport::new();
    fs::create_dir_all(&temporary.root).unwrap();
    let png_path = temporary.root.join("inner.png");
    create_metadata_png(&png_path, "artist:zip-sample");
    let archive_path = temporary.root.join("pack.zip");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("套图/图片 1.png", options).unwrap();
        writer.write_all(&fs::read(&png_path).unwrap()).unwrap();
        writer.start_file("套图/图片副本.png", options).unwrap();
        writer.write_all(&fs::read(&png_path).unwrap()).unwrap();
        writer.finish().unwrap();
    }
    let directory = temporary.initialize_directory();

    let outcome = directory.import_images(&archive_path, |_| {}).unwrap();

    assert_eq!(outcome.added, 1);
    assert_eq!(outcome.skipped_content, 1);
    assert_eq!(outcome.source_type, SourceType::Archive);
    let database = directory.open_database().unwrap();
    let locator = database.row_image_locator(1).unwrap();
    let stored = locator.stored_image_path.unwrap();
    assert!(stored.starts_with(&format!("files/{}/", outcome.batch_id)));
    assert_eq!(
        fs::read(directory.root().join(&stored)).unwrap(),
        fs::read(&png_path).unwrap()
    );
    assert_eq!(
        walkdir::WalkDir::new(directory.files_path().join(outcome.batch_id.to_string()))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .count(),
        1
    );
    assert!(locator.image_path.unwrap().contains(" > "));

    // 同一压缩包重复导入：全部跳过，不留新批次目录。
    let repeat = directory.import_images(&archive_path, |_| {}).unwrap();
    assert_eq!(repeat.added, 0);
    assert_eq!(repeat.skipped_existing, 1);
    assert_eq!(repeat.skipped_content, 1);
    assert_eq!(
        directory
            .open_database()
            .unwrap()
            .library_summary()
            .unwrap()
            .row_count,
        1
    );
}

#[test]
pub(super) fn update_import_refreshes_existing_archive_member_without_adding_new_member() {
    let temporary = TemporaryImageImport::new();
    fs::create_dir_all(&temporary.root).unwrap();
    let existing_png = temporary.root.join("archive-existing.png");
    let new_png = temporary.root.join("archive-new.png");
    let archive_path = temporary.root.join("update-pack.zip");
    write_metadata_png(&existing_png, "archive old prompt");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "nested/existing.png",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(&fs::read(&existing_png).unwrap()).unwrap();
        writer.finish().unwrap();
    }
    let directory = temporary.initialize_directory();
    directory.import_images(&archive_path, |_| {}).unwrap();
    let stored_path = directory
        .open_database()
        .unwrap()
        .row_image_locator(1)
        .unwrap()
        .stored_image_path
        .unwrap();

    write_metadata_png(&existing_png, "archive new prompt");
    write_metadata_png(&new_png, "archive member must not be added");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("nested/existing.png", options).unwrap();
        writer.write_all(&fs::read(&existing_png).unwrap()).unwrap();
        writer.start_file("nested/new.png", options).unwrap();
        writer.write_all(&fs::read(&new_png).unwrap()).unwrap();
        writer.finish().unwrap();
    }

    let outcome = directory
        .update_existing_images(&archive_path, |_| {})
        .unwrap();

    assert_eq!(outcome.source_type, SourceType::Archive);
    assert_eq!(outcome.total_found, 2);
    assert_eq!(outcome.matched, 1);
    assert_eq!(outcome.updated, 1);
    assert_eq!(outcome.unmatched, 1);
    let mut database = directory.open_database().unwrap();
    assert_eq!(database.library_summary().unwrap().row_count, 1);
    assert_eq!(
        database.get_rows_by_ids(&[1]).unwrap()[0]
            .positive_prompt
            .as_deref(),
        Some("archive new prompt")
    );
    assert_eq!(
        fs::read(directory.root().join(stored_path)).unwrap(),
        fs::read(existing_png).unwrap()
    );
}

#[test]
pub(super) fn emits_progress_and_rejects_empty_input() {
    let temporary = TemporaryImageImport::new();
    let input = temporary.root.join("input");
    fs::create_dir_all(&input).unwrap();
    create_metadata_png(&input.join("only.png"), "solo");
    let directory = temporary.initialize_directory();

    let events = Mutex::new(Vec::new());
    directory
        .import_images(&input, |progress| {
            events.lock().unwrap().push(progress);
        })
        .unwrap();
    let events = events.into_inner().unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.stage == ImageImportStage::Scanning)
    );
    assert!(events.iter().any(|event| {
        event.stage == ImageImportStage::Hashing && event.processed == event.total
    }));
    assert!(events.iter().any(|event| {
        event.stage == ImageImportStage::Processing && event.processed == event.total
    }));

    let empty = temporary.root.join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert!(matches!(
        directory.import_images(&empty, |_| {}),
        Err(ImageImportError::NoImagesFound(_))
    ));
}

/// M14 验收：万行库追加导入 5 张图 → 10005 行；重复导入不翻倍。
#[test]
pub(super) fn appends_five_images_to_ten_thousand_row_library() {
    let temporary = TemporaryImageImport::new();
    let input = temporary.root.join("pack");
    fs::create_dir_all(&input).unwrap();
    for index in 1..=5 {
        create_metadata_png(
            &input.join(format!("new-{index}.png")),
            &format!("appended {index}"),
        );
    }
    let directory = temporary.initialize_directory();
    {
        let mut database = directory.open_database().unwrap();
        crate::db::test_support::append_rows(
            &mut database,
            &crate::db::test_support::test_rows(10_000),
        );
    }

    let outcome = directory.import_images(&input, |_| {}).unwrap();
    assert_eq!(outcome.added, 5);
    assert_eq!(outcome.skipped_existing, 0);

    let summary = directory
        .open_database()
        .unwrap()
        .library_summary()
        .unwrap();
    assert_eq!(summary.row_count, 10_005);

    let repeat = directory.import_images(&input, |_| {}).unwrap();
    assert_eq!(repeat.added, 0);
    assert_eq!(repeat.skipped_existing, 5);
    assert_eq!(
        directory
            .open_database()
            .unwrap()
            .library_summary()
            .unwrap()
            .row_count,
        10_005
    );
}

#[test]
pub(super) fn moves_rejected_archive_member_and_keeps_valid_member_managed() {
    let temporary = TemporaryImageImport::new();
    fs::create_dir_all(&temporary.root).unwrap();
    let valid_png = temporary.root.join("valid.png");
    create_metadata_png(&valid_png, "archive prompt");
    let archive_path = temporary.root.join("mixed.zip");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("set/valid.png", options).unwrap();
        writer.write_all(&fs::read(&valid_png).unwrap()).unwrap();
        writer.start_file("set/broken.png", options).unwrap();
        writer.write_all(b"not a png").unwrap();
        writer.finish().unwrap();
    }
    let directory = temporary.initialize_directory();

    let outcome = directory.import_images(&archive_path, |_| {}).unwrap();

    assert_eq!(outcome.added, 1);
    assert_eq!(outcome.metadata_rejected, 1);
    assert_eq!(outcome.rejected_moved, 1);
    assert_eq!(outcome.rejected_move_failures, 0);
    assert_eq!(
        fs::read(temporary.rejected.join("set").join("broken.png")).unwrap(),
        b"not a png"
    );
    let stored = directory
        .open_database()
        .unwrap()
        .row_image_locator(1)
        .unwrap()
        .stored_image_path
        .unwrap();
    assert!(directory.root().join(stored).is_file());
}

#[test]
pub(super) fn rejects_empty_metadata_moves_without_overwriting_and_keeps_rows_out_of_database() {
    let temporary = TemporaryImageImport::new();
    let input = temporary.root.join("input");
    fs::create_dir_all(input.join("nested")).unwrap();
    create_metadata_png(&input.join("empty.png"), "   ");
    create_text_png(
        &input.join("nested").join("comment.png"),
        "Comment",
        r#"{"seed": 1}"#,
    );
    let directory = temporary.initialize_directory();
    fs::write(temporary.rejected.join("empty.png"), b"keep existing").unwrap();

    let outcome = directory.import_images(&input, |_| {}).unwrap();

    assert_eq!(outcome.total_found, 2);
    assert_eq!(outcome.added, 0);
    assert_eq!(outcome.metadata_rejected, 2);
    assert_eq!(outcome.rejected_moved, 2);
    assert_eq!(outcome.rejected_move_failures, 0);
    assert_eq!(
        directory
            .open_database()
            .unwrap()
            .library_summary()
            .unwrap()
            .row_count,
        0
    );
    assert_eq!(
        fs::read(temporary.rejected.join("empty.png")).unwrap(),
        b"keep existing"
    );
    assert!(temporary.rejected.join("empty_2.png").is_file());
    assert!(
        temporary
            .rejected
            .join("nested")
            .join("comment.png")
            .is_file()
    );
}

#[test]
pub(super) fn reports_rejected_move_failure_without_importing_the_image() {
    let temporary = TemporaryImageImport::new();
    let input = temporary.root.join("input");
    fs::create_dir_all(input.join("nested")).unwrap();
    fs::write(input.join("nested").join("broken.png"), b"not a png").unwrap();
    let directory = temporary.initialize_directory();
    fs::write(
        temporary.rejected.join("nested"),
        b"blocks directory creation",
    )
    .unwrap();

    let outcome = directory.import_images(&input, |_| {}).unwrap();

    assert_eq!(outcome.added, 0);
    assert_eq!(outcome.metadata_rejected, 1);
    assert_eq!(outcome.rejected_moved, 0);
    assert_eq!(outcome.rejected_move_failures, 1);
    assert!(input.join("nested").join("broken.png").is_file());
    assert_eq!(
        directory
            .open_database()
            .unwrap()
            .library_summary()
            .unwrap()
            .row_count,
        0
    );
}

#[test]
pub(super) fn rejects_output_directory_inside_import_folder() {
    let temporary = TemporaryImageImport::new();
    let input = temporary.root.join("input");
    fs::create_dir_all(&input).unwrap();
    create_metadata_png(&input.join("valid.png"), "valid prompt");
    let directory = temporary.initialize_directory();
    let inside = input.join("rejected");
    directory.set_rejected_images_directory(&inside).unwrap();

    assert!(matches!(
        directory.import_images(&input, |_| {}),
        Err(ImageImportError::RejectedDirectoryInsideInput(path)) if path == inside.canonicalize().unwrap()
    ));
}

#[test]
pub(super) fn single_png_can_move_to_sibling_output_directory() {
    let temporary = TemporaryImageImport::new();
    let input = temporary.root.join("input");
    fs::create_dir_all(&input).unwrap();
    let broken = input.join("broken.png");
    fs::write(&broken, b"not a png").unwrap();
    let directory = temporary.initialize_directory();
    let sibling_output = input.join("rejected");
    directory
        .set_rejected_images_directory(&sibling_output)
        .unwrap();

    let outcome = directory.import_images(&broken, |_| {}).unwrap();

    assert_eq!(outcome.metadata_rejected, 1);
    assert_eq!(outcome.rejected_moved, 1);
    assert_eq!(outcome.added, 0);
    assert!(!broken.exists());
    assert!(sibling_output.join("broken.png").is_file());
}

#[test]
pub(super) fn imports_only_one_of_five_hundred_identical_files_and_dedupes_across_folders() {
    let temporary = TemporaryImageImport::new();
    let first = temporary.root.join("first");
    let second = temporary.root.join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    let template = first.join("image-000.png");
    create_metadata_png(&template, "identical content");
    let bytes = fs::read(&template).unwrap();
    for index in 1..500 {
        fs::write(first.join(format!("image-{index:03}.png")), &bytes).unwrap();
    }
    fs::write(second.join("copy.png"), &bytes).unwrap();
    let directory = temporary.initialize_directory();

    let first_outcome = directory.import_images(&first, |_| {}).unwrap();
    assert_eq!(first_outcome.total_found, 500);
    assert_eq!(first_outcome.added, 1);
    assert_eq!(first_outcome.skipped_existing, 0);
    assert_eq!(first_outcome.skipped_content, 499);

    let second_outcome = directory.import_images(&second, |_| {}).unwrap();
    assert_eq!(second_outcome.added, 0);
    assert_eq!(second_outcome.skipped_existing, 0);
    assert_eq!(second_outcome.skipped_content, 1);
    assert_eq!(
        directory
            .open_database()
            .unwrap()
            .library_summary()
            .unwrap()
            .row_count,
        1
    );
}

#[test]
pub(super) fn deleted_archive_row_cleans_stored_copy_and_can_reimport() {
    let temporary = TemporaryImageImport::new();
    fs::create_dir_all(&temporary.root).unwrap();
    let png_path = temporary.root.join("inner.png");
    create_metadata_png(&png_path, "artist:cycle");
    let archive_path = temporary.root.join("cycle.zip");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("one.png", options).unwrap();
        writer.write_all(&fs::read(&png_path).unwrap()).unwrap();
        writer.finish().unwrap();
    }
    let directory = temporary.initialize_directory();
    directory.import_images(&archive_path, |_| {}).unwrap();

    let report = directory
        .delete_rows(&RowSelection::Explicit { row_ids: vec![1] }, false)
        .unwrap();
    assert_eq!(report.deleted_rows, 1);
    assert_eq!(report.removed_files, 1);

    let again = directory.import_images(&archive_path, |_| {}).unwrap();
    assert_eq!(again.added, 1);
}

#[test]
pub(super) fn user_authored_example_rule_runs_only_for_new_matching_imports() {
    let temporary = TemporaryImageImport::new();
    let input = temporary.root.join("rule-import");
    fs::create_dir_all(&input).unwrap();
    let fixed_form =
        "girl, white long hair, blue eyes, colored inner hair(blue), hair flower(white flower)";
    fs::write(
        input.join("matching.png"),
        metadata_png_bytes(
            fixed_form,
            Some(r#"{"model":"nai-diffusion-4","steps":28,"sampler":"k_euler","seed":123}"#),
        ),
    )
    .unwrap();
    fs::write(
        input.join("other.png"),
        metadata_png_bytes(
            "boy, black hair, brown eyes",
            Some(r#"{"model":"nai-diffusion-4","steps":28}"#),
        ),
    )
    .unwrap();
    let directory = temporary.initialize_directory();
    let mut database = directory.open_database().unwrap();
    assert!(database.list_automation_rules().unwrap().is_empty());
    database
        .create_automation_rule(&AutomationRuleDraft {
            name: "测试：识别固定形态".into(),
            description: "测试数据，不是内置规则".into(),
            enabled: true,
            run_on_import: true,
            run_on_update: false,
            conditions: RuleConditionSet {
                mode: RuleMatchMode::Any,
                negate: false,
                groups: vec![RuleConditionGroup {
                    mode: RuleMatchMode::All,
                    conditions: vec![
                        RuleCondition::Prompt {
                            scope: PromptScope::PositiveAndCharacter,
                            operator: PromptOperator::ContainsAll,
                            value: fixed_form.into(),
                            case_sensitive: false,
                        },
                        RuleCondition::ImageDimension {
                            field: ImageDimensionField::Width,
                            comparison: NumericComparison {
                                operator: NumericOperator::Equal,
                                value: 16.0,
                                second_value: None,
                            },
                        },
                        RuleCondition::GenerationText {
                            field: GenerationTextField::Model,
                            operator: TextOperator::Equals,
                            value: "nai-diffusion-4".into(),
                            case_sensitive: false,
                        },
                        RuleCondition::GenerationNumber {
                            field: GenerationNumberField::Steps,
                            comparison: NumericComparison {
                                operator: NumericOperator::Equal,
                                value: 28.0,
                                second_value: None,
                            },
                        },
                        RuleCondition::Tag {
                            operator: TagOperator::HasNone,
                            tags: vec!["花绘".into()],
                        },
                    ],
                }],
            },
            actions: vec![RuleAction::AddTags {
                tags: vec!["花绘".into()],
            }],
        })
        .unwrap();
    drop(database);

    let outcome = directory.import_images(&input, |_| {}).unwrap();
    assert_eq!(outcome.added, 2);
    assert_eq!(outcome.rule_execution.input_rows, 2);
    assert_eq!(outcome.rule_execution.changed_rows, 1);
    assert_eq!(outcome.rule_execution.reports[0].matched_rows, 1);

    let mut database = directory.open_database().unwrap();
    let row_ids = database.row_ids_for_batch(outcome.batch_id).unwrap();
    let selected = database
        .list_selection_tags(&RowSelection::Explicit { row_ids })
        .unwrap();
    let example = selected.iter().find(|tag| tag.name == "花绘").unwrap();
    assert_eq!(example.selected_rows, 1);
    drop(database);

    let duplicate = directory.import_images(&input, |_| {}).unwrap();
    assert_eq!(duplicate.added, 0);
    assert_eq!(duplicate.rule_execution.input_rows, 0);
    assert_eq!(duplicate.rule_execution.changed_rows, 0);
}

pub(super) struct TemporaryImageImport {
    root: PathBuf,
    data: PathBuf,
    rejected: PathBuf,
}

impl TemporaryImageImport {
    fn new() -> Self {
        let parent = Path::new(r"D:\Agent\Agent_temp");
        let parent = if parent.is_dir() {
            parent.to_owned()
        } else {
            std::env::temp_dir()
        };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        let root = parent.join(format!(
            "smart-spreadsheet-image-import-{}-{nonce}",
            std::process::id()
        ));
        Self {
            data: root.join("data"),
            rejected: root.join("rejected"),
            root,
        }
    }

    fn initialize_directory(&self) -> DataDirectory {
        let directory = DataDirectory::initialize(&self.data).unwrap();
        directory
            .set_rejected_images_directory(&self.rejected)
            .unwrap();
        directory
    }
}

impl Drop for TemporaryImageImport {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
