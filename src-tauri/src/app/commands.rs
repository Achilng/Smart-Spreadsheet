use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{Emitter, Manager, State, ipc::Response};

use super::runtime::{AppRuntime, RuntimeSnapshot};
use crate::db::{
    BatchSummary, DedupeCluster, DedupeMode, GroupSummary, LibrarySummary, PromptEditResult,
    RowPage, RowQuery, RowRecord, RowSelection, SinglePromptEditResult, TagMatchMode,
    TagMutationResult, TagSelectionSummary, TagSummary,
};
use crate::storage::{
    PerceptualHashProgress, PromptDocAsset, PromptDocDetail, PromptDocSummary, SimilarImageMatch,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSnapshotDto {
    data_directory: Option<String>,
    rejected_images_directory: Option<String>,
    library: Option<LibrarySummaryDto>,
    startup_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibrarySummaryDto {
    row_count: u64,
    batch_count: u64,
    last_batch: Option<BatchSummaryDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchSummaryDto {
    id: i64,
    source_type: &'static str,
    source_path: String,
    imported_at: String,
    added_count: u64,
    skipped_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeleteResultDto {
    snapshot: AppSnapshotDto,
    deleted_rows: u64,
    cleanup_failures: usize,
    trashed_original_files: usize,
    original_file_failures: usize,
    archive_rows_skipped: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageImportResultDto {
    snapshot: AppSnapshotDto,
    source_type: &'static str,
    total_found: usize,
    added: u64,
    skipped_existing: u64,
    skipped_content: u64,
    changed_existing: u64,
    metadata_rejected: u64,
    rejected_moved: u64,
    rejected_move_failures: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExistingImageUpdateResultDto {
    snapshot: AppSnapshotDto,
    source_type: &'static str,
    total_found: usize,
    matched: u64,
    updated: u64,
    unmatched: u64,
    metadata_rejected: u64,
    copy_failures: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowPageDto {
    rows: Vec<RowRecord>,
    total_count: u64,
    offset: u64,
    limit: u32,
    has_more: bool,
}

/// 三种导出共用的进度事件载荷，经 `export://progress` 推送。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportProgressDto {
    processed: usize,
    total: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DedupeClusterDto {
    key: String,
    member_count: u64,
    alias: Option<String>,
}

impl From<DedupeCluster> for DedupeClusterDto {
    fn from(cluster: DedupeCluster) -> Self {
        Self {
            key: cluster.key,
            member_count: cluster.member_count,
            alias: cluster.alias,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XlsxExportResultDto {
    path: String,
    row_count: usize,
    images_embedded: usize,
    image_failures: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonExportResultDto {
    path: String,
    exported: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonExportNoteInspectionDto {
    total: usize,
    empty_notes: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageFilesExportResultDto {
    directory: String,
    exported: usize,
    hardlink_fallbacks: usize,
    missing: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MigrationResultDto {
    snapshot: AppSnapshotDto,
    retired_source: Option<String>,
}

#[tauri::command]
pub(crate) fn get_app_snapshot(runtime: State<'_, AppRuntime>) -> Result<AppSnapshotDto, String> {
    runtime
        .snapshot()
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn reset_configuration(
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .reset_configuration()
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn reset_data(
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .reset_data()
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn initialize_data_directory(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .initialize_directory(PathBuf::from(path))
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

/// 打开已有受管目录可能需要为历史行补算内容哈希，在阻塞线程执行并上报进度。
#[tauri::command]
pub(crate) async fn open_data_directory(
    path: String,
    app: tauri::AppHandle,
) -> Result<AppSnapshotDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .open_directory(PathBuf::from(path), |progress| {
                let _ = app.emit("content-hash://progress", progress);
            })
            .map(AppSnapshotDto::from)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("打开数据目录任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn set_rejected_images_directory(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .set_rejected_images_directory(PathBuf::from(path))
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

/// 文件夹/压缩包导入：在阻塞线程上执行避免卡住 UI，进度经
/// `import-images://progress` 事件推送给前端。
#[tauri::command]
pub(crate) async fn import_images(
    path: String,
    app: tauri::AppHandle,
) -> Result<ImageImportResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .import_images(PathBuf::from(path), |progress| {
                let _ = app.emit("import-images://progress", progress);
            })
            .map(|(snapshot, outcome)| ImageImportResultDto {
                snapshot: snapshot.into(),
                source_type: outcome.source_type.as_str(),
                total_found: outcome.total_found,
                added: outcome.added,
                skipped_existing: outcome.skipped_existing,
                skipped_content: outcome.skipped_content,
                changed_existing: outcome.changed_existing,
                metadata_rejected: outcome.metadata_rejected,
                rejected_moved: outcome.rejected_moved,
                rejected_move_failures: outcome.rejected_move_failures,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("导入任务异常中止: {error}"))?
}

/// 仅更新身份键已存在的图片；进度复用 `import-images://progress`。
#[tauri::command]
pub(crate) async fn update_existing_images(
    path: String,
    app: tauri::AppHandle,
) -> Result<ExistingImageUpdateResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .update_existing_images(PathBuf::from(path), |progress| {
                let _ = app.emit("import-images://progress", progress);
            })
            .map(|(snapshot, outcome)| ExistingImageUpdateResultDto {
                snapshot: snapshot.into(),
                source_type: outcome.source_type.as_str(),
                total_found: outcome.total_found,
                matched: outcome.matched,
                updated: outcome.updated,
                unmatched: outcome.unmatched,
                metadata_rejected: outcome.metadata_rejected,
                copy_failures: outcome.copy_failures,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("更新现有图片任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn delete_rows(
    selection: RowSelection,
    trash_originals: bool,
    runtime: State<'_, AppRuntime>,
) -> Result<DeleteResultDto, String> {
    runtime
        .delete_rows(&selection, trash_originals)
        .map(|(snapshot, report)| DeleteResultDto {
            snapshot: snapshot.into(),
            deleted_rows: report.deleted_rows,
            cleanup_failures: report.cleanup_failures,
            trashed_original_files: report.trashed_original_files,
            original_file_failures: report.original_file_failures,
            archive_rows_skipped: report.archive_rows_skipped,
        })
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn list_import_batches(
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<BatchSummaryDto>, String> {
    runtime
        .list_batches()
        .map(|batches| batches.into_iter().map(BatchSummaryDto::from).collect())
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn create_group(
    name: String,
    runtime: State<'_, AppRuntime>,
) -> Result<GroupSummary, String> {
    runtime.create_group(&name).map_err(error_text)
}

#[tauri::command]
pub(crate) fn rename_group(
    group_id: i64,
    new_name: String,
    runtime: State<'_, AppRuntime>,
) -> Result<GroupSummary, String> {
    runtime.rename_group(group_id, &new_name).map_err(error_text)
}

#[tauri::command]
pub(crate) fn delete_group(
    group_id: i64,
    runtime: State<'_, AppRuntime>,
) -> Result<bool, String> {
    runtime.delete_group(group_id).map_err(error_text)
}

#[tauri::command]
pub(crate) fn delete_empty_groups(
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime.delete_empty_groups().map_err(error_text)
}

#[tauri::command]
pub(crate) fn list_groups(
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<GroupSummary>, String> {
    runtime.list_groups().map_err(error_text)
}

#[tauri::command]
pub(crate) fn assign_rows_to_group(
    selection: RowSelection,
    group_id: i64,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .assign_rows_to_group(&selection, group_id)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn ungroup_rows(
    selection: RowSelection,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime.ungroup_rows(&selection).map_err(error_text)
}

#[tauri::command]
pub(crate) fn update_positive_prompt(
    row_id: i64,
    new_prompt: String,
    runtime: State<'_, AppRuntime>,
) -> Result<SinglePromptEditResult, String> {
    runtime
        .update_positive_prompt(row_id, &new_prompt)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn update_negative_prompt(
    row_id: i64,
    new_prompt: String,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .update_negative_prompt(row_id, &new_prompt)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn update_note(
    row_id: i64,
    note: String,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime.update_note(row_id, &note).map_err(error_text)
}

#[tauri::command]
pub(crate) fn update_character_prompt(
    row_id: i64,
    new_prompt: String,
    runtime: State<'_, AppRuntime>,
) -> Result<SinglePromptEditResult, String> {
    runtime
        .update_character_prompt(row_id, &new_prompt)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn find_replace_prompt(
    selection: RowSelection,
    find: String,
    replace: String,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptEditResult, String> {
    runtime
        .find_replace_prompt(&selection, &find, &replace)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn prepend_artist(
    selection: RowSelection,
    artist_name: String,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptEditResult, String> {
    runtime
        .prepend_artist(&selection, &artist_name)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) async fn suggest_groups(
    mode: crate::pipeline::similarity::SimilarityMode,
    threshold: f64,
    app: tauri::AppHandle,
) -> Result<Vec<crate::pipeline::similarity::SuggestedGroup>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        let rows = runtime.ungrouped_keys(mode).map_err(error_text)?;
        Ok(crate::pipeline::similarity::suggest_groups(rows, mode, threshold, |progress| {
            let _ = app.emit("group-suggest://progress", progress);
        }))
    })
    .await
    .map_err(|error| format!("分组建议任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn get_group_members(
    group_id: i64,
    offset: u64,
    limit: u32,
    runtime: State<'_, AppRuntime>,
) -> Result<RowPageDto, String> {
    runtime
        .get_group_members(group_id, offset, limit)
        .map(RowPageDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn list_dedupe_clusters(
    dedupe: DedupeMode,
    tags: Vec<String>,
    tag_mode: TagMatchMode,
    single_artist_only: bool,
    hide_grouped: bool,
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<DedupeClusterDto>, String> {
    runtime
        .list_dedupe_clusters(dedupe, &tags, tag_mode, single_artist_only, hide_grouped)
        .map(|clusters| clusters.into_iter().map(DedupeClusterDto::from).collect())
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn list_distinct_artists(
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<String>, String> {
    runtime.list_distinct_artists().map_err(error_text)
}

#[tauri::command]
pub(crate) fn row_ids_with_artists(
    artists: String,
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<i64>, String> {
    runtime.row_ids_with_artists(&artists).map_err(error_text)
}

#[tauri::command]
pub(crate) fn get_custom_artists(runtime: State<'_, AppRuntime>) -> Result<String, String> {
    runtime.get_custom_artists().map_err(error_text)
}

#[tauri::command]
pub(crate) fn set_custom_artists(
    text: String,
    runtime: State<'_, AppRuntime>,
) -> Result<(), String> {
    runtime.set_custom_artists(&text).map_err(error_text)
}

#[tauri::command]
pub(crate) fn list_prompt_docs(
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<PromptDocSummary>, String> {
    runtime.list_prompt_docs().map_err(error_text)
}

#[tauri::command]
pub(crate) fn create_prompt_doc(
    title: String,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptDocDetail, String> {
    runtime.create_prompt_doc(&title).map_err(error_text)
}

#[tauri::command]
pub(crate) fn load_prompt_doc(
    doc_id: String,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptDocDetail, String> {
    runtime.load_prompt_doc(&doc_id).map_err(error_text)
}

#[tauri::command]
pub(crate) fn save_prompt_doc(
    doc_id: String,
    title: String,
    content: serde_json::Value,
    plain_text: String,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptDocDetail, String> {
    runtime
        .save_prompt_doc(&doc_id, &title, &content, &plain_text)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn delete_prompt_doc(
    doc_id: String,
    runtime: State<'_, AppRuntime>,
) -> Result<(), String> {
    runtime.delete_prompt_doc(&doc_id).map_err(error_text)
}

#[tauri::command]
pub(crate) fn import_prompt_doc_image_from_path(
    doc_id: String,
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptDocAsset, String> {
    runtime
        .import_prompt_doc_image_from_path(&doc_id, PathBuf::from(path))
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn import_prompt_doc_image_bytes(
    doc_id: String,
    file_name: String,
    bytes: Vec<u8>,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptDocAsset, String> {
    runtime
        .import_prompt_doc_image_bytes(&doc_id, &file_name, &bytes)
        .map_err(error_text)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) fn get_dedupe_cluster_members(
    dedupe: DedupeMode,
    key: String,
    tags: Vec<String>,
    tag_mode: TagMatchMode,
    single_artist_only: bool,
    hide_grouped: bool,
    offset: u64,
    limit: u32,
    runtime: State<'_, AppRuntime>,
) -> Result<RowPageDto, String> {
    runtime
        .get_dedupe_cluster_members(
            dedupe,
            &key,
            &tags,
            tag_mode,
            single_artist_only,
            hide_grouped,
            offset,
            limit,
        )
        .map(RowPageDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn set_dedupe_alias(
    dedupe: DedupeMode,
    key: String,
    alias: String,
    runtime: State<'_, AppRuntime>,
) -> Result<(), String> {
    runtime
        .set_dedupe_alias(dedupe, &key, &alias)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn query_rows(
    query: RowQuery,
    runtime: State<'_, AppRuntime>,
) -> Result<RowPageDto, String> {
    runtime
        .query_rows(&query)
        .map(RowPageDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn get_rows_by_ids(
    row_ids: Vec<i64>,
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<RowRecord>, String> {
    runtime.get_rows_by_ids(&row_ids).map_err(error_text)
}

#[tauri::command]
pub(crate) fn list_tags(runtime: State<'_, AppRuntime>) -> Result<Vec<TagSummary>, String> {
    runtime.list_tags().map_err(error_text)
}

#[tauri::command]
pub(crate) fn create_tag(name: String, runtime: State<'_, AppRuntime>) -> Result<bool, String> {
    runtime.create_tag(&name).map_err(error_text)
}

#[tauri::command]
pub(crate) fn delete_tag(name: String, runtime: State<'_, AppRuntime>) -> Result<bool, String> {
    runtime.delete_tag(&name).map_err(error_text)
}

#[tauri::command]
pub(crate) fn count_selected_rows(
    selection: RowSelection,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime.count_selected_rows(&selection).map_err(error_text)
}

#[tauri::command]
pub(crate) fn list_selection_tags(
    selection: RowSelection,
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<TagSelectionSummary>, String> {
    runtime
        .list_selection_tags(&selection)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn selected_row_ids(
    selection: RowSelection,
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<i64>, String> {
    runtime.selected_row_ids(&selection).map_err(error_text)
}

#[tauri::command]
pub(crate) fn add_tags_to_selection(
    selection: RowSelection,
    tags: Vec<String>,
    runtime: State<'_, AppRuntime>,
) -> Result<TagMutationResult, String> {
    runtime
        .add_tags_to_selection(&selection, &tags)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn remove_tags_from_selection(
    selection: RowSelection,
    tags: Vec<String>,
    runtime: State<'_, AppRuntime>,
) -> Result<TagMutationResult, String> {
    runtime
        .remove_tags_from_selection(&selection, &tags)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn set_tags_for_row(
    row_id: i64,
    tags: Vec<String>,
    runtime: State<'_, AppRuntime>,
) -> Result<TagMutationResult, String> {
    runtime
        .set_tags_for_row(row_id, &tags)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) async fn get_row_thumbnail(
    row_id: i64,
    app: tauri::AppHandle,
) -> Result<Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .row_thumbnail(row_id)
            .map(Response::new)
            .map_err(error_text)
    })
    .await
    .map_err(|e| format!("缩略图加载异常: {e}"))?
}

#[tauri::command]
pub(crate) async fn get_row_preview(
    row_id: i64,
    app: tauri::AppHandle,
) -> Result<Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .row_preview(row_id)
            .map(Response::new)
            .map_err(error_text)
    })
    .await
    .map_err(|e| format!("预览图加载异常: {e}"))?
}

#[tauri::command]
pub(crate) async fn export_row_image(
    row_id: i64,
    destination: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_row_image(row_id, PathBuf::from(destination))
            .map_err(error_text)
    })
    .await
    .map_err(|e| format!("图片导出异常: {e}"))?
}

/// 导出带缩略图的 xlsx；在阻塞线程上执行，进度经 `export://progress` 推送。
#[tauri::command]
pub(crate) async fn export_xlsx(
    selection: RowSelection,
    path: String,
    app: tauri::AppHandle,
) -> Result<XlsxExportResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_xlsx(&selection, PathBuf::from(path), |progress| {
                emit_export_progress(&app, progress.processed, progress.total);
            })
            .map(|outcome| XlsxExportResultDto {
                path: outcome.destination.to_string_lossy().into_owned(),
                row_count: outcome.row_count,
                images_embedded: outcome.images_embedded,
                image_failures: outcome.image_failures,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("导出任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn inspect_zhihuiji_export_notes(
    selection: RowSelection,
    runtime: State<'_, AppRuntime>,
) -> Result<JsonExportNoteInspectionDto, String> {
    runtime
        .inspect_zhihuiji_export_notes(&selection)
        .map(|(total, empty_notes)| JsonExportNoteInspectionDto { total, empty_notes })
        .map_err(error_text)
}

#[tauri::command]
pub(crate) async fn export_zhihuiji_json(
    selection: RowSelection,
    path: String,
    use_numeric_names_for_empty: bool,
    app: tauri::AppHandle,
) -> Result<JsonExportResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_zhihuiji_json(
                &selection,
                PathBuf::from(path),
                use_numeric_names_for_empty,
                |progress| {
                    emit_export_progress(&app, progress.processed, progress.total);
                },
            )
            .map(|outcome| JsonExportResultDto {
                path: outcome.destination.to_string_lossy().into_owned(),
                exported: outcome.exported,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("导出任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) async fn export_image_files(
    selection: RowSelection,
    parent_dir: String,
    mode: String,
    app: tauri::AppHandle,
) -> Result<ImageFilesExportResultDto, String> {
    let mode = crate::storage::ImageFileExportMode::parse(&mode)
        .ok_or_else(|| format!("未知的图片导出方式: {mode}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_image_files(&selection, PathBuf::from(parent_dir), mode, |progress| {
                emit_export_progress(&app, progress.processed, progress.total);
            })
            .map(|outcome| ImageFilesExportResultDto {
                directory: outcome.directory.to_string_lossy().into_owned(),
                exported: outcome.exported,
                hardlink_fallbacks: outcome.hardlink_fallbacks,
                missing: outcome.missing,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("导出任务异常中止: {error}"))?
}

fn emit_export_progress(app: &tauri::AppHandle, processed: usize, total: usize) {
    let _ = app.emit("export://progress", ExportProgressDto { processed, total });
}

/// 智绘姬 JSON 工具：检查重复项（只读，不修改文件）。
#[tauri::command]
pub(crate) async fn inspect_zhihuiji_json(
    path: String,
) -> Result<crate::pipeline::json_dedupe::JsonDedupeInspection, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::pipeline::json_dedupe::inspect_zhihuiji_json_file(Path::new(&path))
            .map_err(|error| format!("{error:#}"))
    })
    .await
    .map_err(|error| format!("检查任务异常中止: {error}"))?
}

/// 智绘姬 JSON 工具：去重并写出，进度经 `json-dedupe://progress` 推送。
#[tauri::command]
pub(crate) async fn dedupe_zhihuiji_json(
    input_path: String,
    output_path: String,
    app: tauri::AppHandle,
) -> Result<crate::pipeline::json_dedupe::JsonDedupeSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::pipeline::json_dedupe::dedupe_zhihuiji_json_file(
            Path::new(&input_path),
            Path::new(&output_path),
            |progress| {
                let _ = app.emit("json-dedupe://progress", progress);
            },
        )
        .map_err(|error| format!("{error:#}"))
    })
    .await
    .map_err(|error| format!("去重任务异常中止: {error}"))?
}

/// 手动刷新感知哈希：为库中缺少 pHash 的行补算。
#[tauri::command]
pub(crate) async fn backfill_perceptual_hashes(
    app: tauri::AppHandle,
) -> Result<PerceptualHashProgress, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .backfill_perceptual_hashes(|progress| {
                let _ = app.emit("perceptual-hash://progress", progress);
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("感知哈希计算任务异常中止: {error}"))?
}

/// 以图搜图：选择一张图片，返回库中相似的行。
#[tauri::command]
pub(crate) async fn search_similar_images(
    path: String,
    threshold: u32,
    app: tauri::AppHandle,
) -> Result<Vec<SimilarImageMatch>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .search_similar_images(PathBuf::from(path), threshold)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("以图搜图任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn show_item_in_explorer(
    row_id: i64,
    runtime: State<'_, AppRuntime>,
) -> Result<(), String> {
    let directory = runtime.active_directory().map_err(error_text)?;
    let locator = directory.open_database().map_err(error_text)?.row_image_locator(row_id).map_err(error_text)?;
    let source = crate::storage::resolve_image_source(&directory, &locator)
        .ok_or_else(|| format!("第 {row_id} 行没有可用的图片文件"))?;
    open_path_in_explorer(&source);
    Ok(())
}

#[tauri::command]
pub(crate) fn open_rejected_images_directory(
    runtime: State<'_, AppRuntime>,
) -> Result<(), String> {
    let directory = runtime.active_directory().map_err(error_text)?;
    let rejected_dir = directory
        .rejected_images_directory()
        .map_err(error_text)?
        .unwrap_or_else(|| directory.default_rejected_images_directory());
    if rejected_dir.is_dir() {
        open_path_in_explorer(&rejected_dir);
    } else {
        return Err(format!("失败图片目录不存在: {}", rejected_dir.display()));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileDragInfo {
    file_path: String,
    icon_path: String,
}

#[tauri::command]
pub(crate) fn prepare_file_drag(
    row_id: i64,
    runtime: State<'_, AppRuntime>,
) -> Result<FileDragInfo, String> {
    let directory = runtime.active_directory().map_err(error_text)?;
    let locator = directory
        .open_database()
        .map_err(error_text)?
        .row_image_locator(row_id)
        .map_err(error_text)?;
    // 拖出的文件会被下游（如 NovelAI）读取元数据，必须是完整原件，
    // 不能静默回退到无元数据的 xlsx 嵌入缩略图。
    let file_path = crate::storage::resolve_original_source(&directory, &locator)
        .map_err(|error| format!("第 {row_id} 行无法拖出：{error}"))?;

    let thumb_dir = directory.thumbnail_cache_path();
    let prefix = format!("row-{row_id}-");
    let find_icon = || -> Option<PathBuf> {
        std::fs::read_dir(&thumb_dir)
            .ok()?
            .filter_map(|e| e.ok())
            .find(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .map(|e| e.path())
    };
    let icon_path = find_icon().unwrap_or_else(|| {
        let _ = directory.load_row_image(row_id, crate::images::ImageVariant::Thumbnail);
        find_icon().unwrap_or_else(|| file_path.clone())
    });

    Ok(FileDragInfo {
        file_path: file_path.to_string_lossy().into_owned(),
        icon_path: icon_path.to_string_lossy().into_owned(),
    })
}

/// 行图片的 vibe 引用数：读取图片 Comment 元数据中的 `reference_image_multiple`。
/// 文件缺失或元数据无法解析时返回 None（前端不显示徽标）。
#[tauri::command]
pub(crate) async fn get_row_vibe_status(
    row_id: i64,
    app: tauri::AppHandle,
) -> Result<Option<u32>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        let directory = runtime.active_directory().map_err(error_text)?;
        let locator = directory
            .open_database()
            .map_err(error_text)?
            .row_image_locator(row_id)
            .map_err(error_text)?;
        let Some(path) = crate::storage::resolve_image_source(&directory, &locator) else {
            return Ok(None);
        };
        Ok(vibe_reference_count(&path))
    })
    .await
    .map_err(|e| format!("vibe 状态读取异常: {e}"))?
}

fn vibe_reference_count(path: &Path) -> Option<u32> {
    let chunks = crate::pipeline::png_text::read_png_text_chunks(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(chunks.get("Comment")?).ok()?;
    let count = value
        .get("reference_image_multiple")
        .and_then(|refs| refs.as_array())
        .map_or(0, Vec::len);
    u32::try_from(count).ok()
}

fn open_path_in_explorer(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        if path.is_file() {
            let _ = std::process::Command::new("explorer")
                .arg("/select,")
                .arg(path)
                .spawn();
        } else {
            let _ = std::process::Command::new("explorer")
                .arg(path)
                .spawn();
        }
    }
    #[cfg(target_os = "macos")]
    {
        if path.is_file() {
            let _ = std::process::Command::new("open")
                .arg("-R")
                .arg(path)
                .spawn();
        } else {
            let _ = std::process::Command::new("open")
                .arg(path)
                .spawn();
        }
    }
    #[cfg(target_os = "linux")]
    {
        let target = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let _ = std::process::Command::new("xdg-open")
            .arg(target)
            .spawn();
    }
}

#[tauri::command]
pub(crate) fn migrate_data_directory(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<MigrationResultDto, String> {
    runtime
        .migrate_directory(PathBuf::from(path))
        .map(|outcome| MigrationResultDto {
            snapshot: outcome.snapshot.into(),
            retired_source: outcome
                .retired_source
                .map(|path| path.to_string_lossy().into_owned()),
        })
        .map_err(error_text)
}

impl From<RuntimeSnapshot> for AppSnapshotDto {
    fn from(snapshot: RuntimeSnapshot) -> Self {
        Self {
            data_directory: snapshot
                .data_directory
                .map(|path| path.to_string_lossy().into_owned()),
            rejected_images_directory: snapshot
                .rejected_images_directory
                .map(|path| path.to_string_lossy().into_owned()),
            library: snapshot.library.map(LibrarySummaryDto::from),
            startup_error: snapshot.startup_error,
        }
    }
}

impl From<LibrarySummary> for LibrarySummaryDto {
    fn from(summary: LibrarySummary) -> Self {
        Self {
            row_count: summary.row_count,
            batch_count: summary.batch_count,
            last_batch: summary.last_batch.map(BatchSummaryDto::from),
        }
    }
}

impl From<BatchSummary> for BatchSummaryDto {
    fn from(batch: BatchSummary) -> Self {
        Self {
            id: batch.id,
            source_type: batch.source_type.as_str(),
            source_path: batch.source_path,
            imported_at: batch.imported_at,
            added_count: batch.added_count,
            skipped_count: batch.skipped_count,
        }
    }
}

impl From<RowPage> for RowPageDto {
    fn from(page: RowPage) -> Self {
        let has_more = page.has_more();
        Self {
            rows: page.rows,
            total_count: page.total_count,
            offset: page.offset,
            limit: page.limit,
            has_more,
        }
    }
}

fn error_text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use crate::db::{DedupeMode, RowSelection, TagMatchMode};

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
                search: String::new(),
                excluded_row_ids: vec![2, 9],
            }
        );
    }
}
