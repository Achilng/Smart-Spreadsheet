use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::db::{RowQuery, RowSelection};

#[test]
fn auto_initializes_and_reloads_configured_directory() {
    let temporary = TemporaryRuntime::new();
    let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());

    let snapshot = runtime.snapshot().unwrap();
    let reloaded = AppRuntime::load(temporary.locator.clone(), temporary.data.clone())
        .snapshot()
        .unwrap();

    assert_eq!(snapshot.data_directory, reloaded.data_directory);
    assert_eq!(reloaded.data_directory, Some(temporary.data.clone()));
    let library = reloaded.library.unwrap();
    assert_eq!(library.row_count, 0);
    assert_eq!(library.batch_count, 0);
    assert!(reloaded.startup_error.is_none());
}

#[test]
fn refuses_pointer_switch_after_auto_init() {
    let temporary = TemporaryRuntime::new();
    let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());

    let error = runtime
        .initialize_directory(temporary.root.join("other-data"))
        .unwrap_err();

    assert!(matches!(error, AppRuntimeError::AlreadyConfigured));
    assert!(!temporary.root.join("other-data").exists());
}

#[test]
fn auto_init_backfills_content_hashes_for_legacy_rows() {
    let temporary = TemporaryRuntime::new();
    let directory = DataDirectory::initialize(&temporary.data).unwrap();
    fs::create_dir_all(&temporary.root).unwrap();
    let image = temporary.root.join("legacy.png");
    fs::write(&image, b"legacy image bytes").unwrap();
    directory
        .open_database()
        .unwrap()
        .append_batch(
            crate::db::SourceType::Folder,
            &temporary.root.to_string_lossy(),
            &[crate::db::NewRow {
                source_ordinal: 1,
                identity: "file:legacy".into(),
                image_path: Some(image.to_string_lossy().into_owned()),
                ..crate::db::NewRow::default()
            }],
            |_| Ok(()),
        )
        .unwrap();

    let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
    let snapshot = runtime.snapshot().unwrap();

    assert_eq!(snapshot.library.unwrap().row_count, 1);
    assert!(
        directory
            .open_database()
            .unwrap()
            .content_hash_for_row(1)
            .unwrap()
            .is_some()
    );
}

#[test]
fn imports_images_and_restores_summary_after_reload() {
    let temporary = TemporaryRuntime::new();
    let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
    let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 5);

    let (_, outcome) = runtime.import_images(&folder, |_| {}).unwrap();
    let reloaded = AppRuntime::load(temporary.locator.clone(), temporary.data.clone())
        .snapshot()
        .unwrap();

    assert_eq!(outcome.added, 5);
    let library = reloaded.library.unwrap();
    assert_eq!(library.row_count, 5);
    assert_eq!(library.batch_count, 1);
    let last_batch = library.last_batch.unwrap();
    assert!(last_batch.source_path.contains("sample-images"));
    assert_eq!(last_batch.added_count, 5);
}

#[test]
fn undo_import_batch_removes_only_library_copies_and_batch_record() {
    let temporary = TemporaryRuntime::new();
    let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
    let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 3);
    let original = folder.join("sample-1.png");

    let (_, outcome) = runtime.import_images(&folder, |_| {}).unwrap();
    let (snapshot, report) = runtime.undo_import_batch(outcome.batch_id).unwrap();

    assert_eq!(report.deleted_rows, 3);
    assert_eq!(report.trashed_original_files, 0);
    assert!(original.is_file());
    let library = snapshot.library.unwrap();
    assert_eq!(library.row_count, 0);
    assert_eq!(library.batch_count, 0);
    assert!(library.last_batch.is_none());

    let (redone, outcome) = runtime.import_images(&folder, |_| {}).unwrap();
    assert_eq!(outcome.added, 3);
    let library = redone.library.unwrap();
    assert_eq!(library.row_count, 3);
    assert_eq!(library.batch_count, 1);
}

#[test]
fn appends_second_import_and_deletes_rows() {
    let temporary = TemporaryRuntime::new();
    let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
    let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 5);
    runtime.import_images(&folder, |_| {}).unwrap();

    // 重复导入：全部跳过，行数不变。
    let (snapshot, outcome) = runtime.import_images(&folder, |_| {}).unwrap();
    assert_eq!(outcome.added, 0);
    assert_eq!(outcome.skipped_existing, 5);
    assert_eq!(snapshot.library.as_ref().unwrap().row_count, 5);
    assert_eq!(snapshot.library.as_ref().unwrap().batch_count, 2);
    assert_eq!(runtime.list_batches().unwrap().len(), 2);

    let (after_delete, report) = runtime
        .delete_rows(
            &RowSelection::Explicit {
                row_ids: vec![1, 2],
            },
            false,
        )
        .unwrap();
    assert_eq!(report.deleted_rows, 2);
    assert_eq!(after_delete.library.unwrap().row_count, 3);
}

#[test]
fn exposes_tag_queries_and_filtered_mutations() {
    let temporary = TemporaryRuntime::new();
    let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
    let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 5);
    runtime.import_images(&folder, |_| {}).unwrap();

    let explicit = RowSelection::Explicit {
        row_ids: vec![1, 2, 3],
    };
    let added = runtime
        .add_tags_to_selection(&explicit, &[" Keep ".into(), "keep".into()])
        .unwrap();
    assert_eq!(added.affected_rows, 3);
    assert_eq!(added.associations_changed, 6);
    assert_eq!(runtime.list_tags().unwrap().len(), 2);

    let filtered = RowSelection::Filtered {
        tags: vec!["Keep".into()],
        tag_mode: crate::db::TagMatchMode::And,
        dedupe: crate::db::DedupeMode::None,
        single_artist_only: false,
        artist_filter: String::new(),
        has_vibe: false,
        untagged_only: false,
        filters: vec![],
        search: String::new(),
        excluded_row_ids: vec![2],
    };
    assert_eq!(runtime.count_selected_rows(&filtered).unwrap(), 2);
    let removed = runtime
        .remove_tags_from_selection(&filtered, &["Keep".into()])
        .unwrap();
    assert_eq!(removed.affected_rows, 2);
    assert_eq!(removed.associations_changed, 2);
}

#[test]
fn loads_all_image_tiers_for_imported_row() {
    let temporary = TemporaryRuntime::new();
    let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
    let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 1);
    runtime.import_images(&folder, |_| {}).unwrap();

    let thumbnail = runtime.row_thumbnail(1).unwrap();
    let gallery = runtime.row_gallery_preview(1).unwrap();
    let preview = runtime.row_preview(1).unwrap();
    let original = runtime.row_original(1).unwrap();

    assert!(thumbnail.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(gallery.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(preview.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(original.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(thumbnail.len() <= preview.len());
}

#[test]
fn migrates_directory_then_reloads_from_new_locator() {
    let temporary = TemporaryRuntime::new();
    let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
    let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 3);
    runtime.import_images(&folder, |_| {}).unwrap();
    runtime
        .add_tags_to_selection(
            &RowSelection::Explicit { row_ids: vec![1] },
            &["migrated".into()],
        )
        .unwrap();
    let destination = temporary.root.join("migrated-data");

    let outcome = runtime.migrate_directory(&destination).unwrap();
    let reloaded = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());

    let expected_directory = destination.canonicalize().unwrap();
    assert_eq!(
        outcome
            .snapshot
            .data_directory
            .unwrap()
            .canonicalize()
            .unwrap(),
        expected_directory
    );
    assert!(outcome.retired_source.is_none());
    assert!(!temporary.data.exists());
    assert_eq!(
        reloaded
            .snapshot()
            .unwrap()
            .data_directory
            .unwrap()
            .canonicalize()
            .unwrap(),
        expected_directory
    );
    assert_eq!(reloaded.list_tags().unwrap()[0].name, "migrated");
}

#[test]
fn locator_write_failure_restores_source_and_disables_destination() {
    let temporary = TemporaryRuntime::new();
    let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
    let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 2);
    runtime.import_images(&folder, |_| {}).unwrap();
    let destination = temporary.root.join("failed-migration");
    let blocked_temporary = temporary.locator.parent().unwrap().join(format!(
        ".smart-spreadsheet-state-{}.tmp",
        std::process::id()
    ));
    fs::create_dir(&blocked_temporary).unwrap();

    assert!(runtime.migrate_directory(&destination).is_err());

    assert!(DataDirectory::open(&temporary.data).is_ok());
    assert!(DataDirectory::open(&destination).is_err());
    assert_eq!(
        AppRuntime::load(temporary.locator.clone(), temporary.data.clone())
            .snapshot()
            .unwrap()
            .data_directory,
        Some(temporary.data.clone())
    );
}

#[test]
fn query_cache_invalidates_across_tag_mutations_and_deletes() {
    let temporary = TemporaryRuntime::new();
    let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
    let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 3);
    runtime.import_images(&folder, |_| {}).unwrap();

    let tagged_query = RowQuery {
        offset: 0,
        limit: 100,
        tags: vec!["Keep".into()],
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
    };
    assert_eq!(runtime.query_rows(&tagged_query).unwrap().total_count, 0);

    runtime
        .add_tags_to_selection(
            &RowSelection::Explicit {
                row_ids: vec![1, 2],
            },
            &["Keep".into()],
        )
        .unwrap();
    assert_eq!(runtime.query_rows(&tagged_query).unwrap().total_count, 2);

    runtime
        .delete_rows(&RowSelection::Explicit { row_ids: vec![1] }, false)
        .unwrap();
    assert_eq!(runtime.query_rows(&tagged_query).unwrap().total_count, 1);
}

struct TemporaryRuntime {
    root: PathBuf,
    locator: PathBuf,
    data: PathBuf,
}

impl TemporaryRuntime {
    fn new() -> Self {
        let local_agent_temp = Path::new(r"D:\Agent\Agent_temp");
        let parent = if local_agent_temp.is_dir() {
            local_agent_temp.to_owned()
        } else {
            std::env::temp_dir()
        };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        let root = parent.join(format!(
            "smart-spreadsheet-runtime-{}-{nonce}",
            std::process::id()
        ));
        Self {
            locator: root.join("config").join("state.json"),
            data: root.join("data"),
            root,
        }
    }
}

impl Drop for TemporaryRuntime {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
