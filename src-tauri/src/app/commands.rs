use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{Emitter, Manager, State, ipc::Response};

use super::runtime::{AppRuntime, RuntimeSnapshot};
use crate::db::{
    BatchSummary, GroupSummary, LibrarySummary, RowPage, RowQuery, RowRecord, RowSelection,
    TagMutationResult, TagSelectionSummary, TagSummary,
};
use crate::storage::{PerceptualHashProgress, SimilarImageMatch};

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
pub(crate) async fn export_zhihuiji_json(
    selection: RowSelection,
    path: String,
    app: tauri::AppHandle,
) -> Result<JsonExportResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_zhihuiji_json(&selection, PathBuf::from(path), |progress| {
                emit_export_progress(&app, progress.processed, progress.total);
            })
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
                excluded_row_ids: vec![2, 9],
            }
        );
    }
}
