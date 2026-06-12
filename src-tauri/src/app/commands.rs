use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State, ipc::Response};

use super::runtime::{AppRuntime, RuntimeSnapshot};
use crate::db::{
    BatchSummary, DuplicateGroup, DuplicateKey, DuplicateReport, DuplicateRow, LibrarySummary,
    RowPage, RowQuery, RowRecord, RowSelection, TagMatchMode, TagMutationResult, TagSummary,
};
use crate::storage::ImageImportProgress;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSnapshotDto {
    data_directory: Option<String>,
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
pub(crate) struct ImportResultDto {
    snapshot: AppSnapshotDto,
    added: u64,
    skipped_existing: u64,
    changed_existing: u64,
    embedded_images_stored: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeleteResultDto {
    snapshot: AppSnapshotDto,
    deleted_rows: u64,
    cleanup_failures: usize,
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
    metadata_failed: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DuplicateKeyDto {
    PositivePrompt,
    Artists,
}

impl From<DuplicateKeyDto> for DuplicateKey {
    fn from(key: DuplicateKeyDto) -> Self {
        match key {
            DuplicateKeyDto::PositivePrompt => Self::PositivePrompt,
            DuplicateKeyDto::Artists => Self::Artists,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DuplicateReportDto {
    total_groups: u64,
    total_redundant_rows: u64,
    groups: Vec<DuplicateGroupDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateGroupDto {
    key: String,
    rows: Vec<DuplicateRowDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateRowDto {
    id: i64,
    batch_id: i64,
    source_ordinal: u32,
    time: Option<String>,
    image_path: Option<String>,
    stored_image_path: Option<String>,
    tags: Vec<String>,
}

impl From<DuplicateReport> for DuplicateReportDto {
    fn from(report: DuplicateReport) -> Self {
        Self {
            total_groups: report.total_groups,
            total_redundant_rows: report.total_redundant_rows,
            groups: report.groups.into_iter().map(DuplicateGroupDto::from).collect(),
        }
    }
}

impl From<DuplicateGroup> for DuplicateGroupDto {
    fn from(group: DuplicateGroup) -> Self {
        Self {
            key: group.key,
            rows: group.rows.into_iter().map(DuplicateRowDto::from).collect(),
        }
    }
}

impl From<DuplicateRow> for DuplicateRowDto {
    fn from(row: DuplicateRow) -> Self {
        Self {
            id: row.id,
            batch_id: row.batch_id,
            source_ordinal: row.source_ordinal,
            time: row.time,
            image_path: row.image_path,
            stored_image_path: row.stored_image_path,
            tags: row.tags,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageImportProgressDto {
    stage: &'static str,
    processed: usize,
    total: usize,
}

impl From<ImageImportProgress> for ImageImportProgressDto {
    fn from(progress: ImageImportProgress) -> Self {
        Self {
            stage: progress.stage.as_str(),
            processed: progress.processed,
            total: progress.total,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowQueryDto {
    offset: u64,
    limit: u32,
    tags: Vec<String>,
    tag_mode: TagMatchModeDto,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TagMatchModeDto {
    And,
    Or,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowPageDto {
    rows: Vec<RowRecordDto>,
    total_count: u64,
    offset: u64,
    limit: u32,
    has_more: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RowRecordDto {
    id: i64,
    batch_id: i64,
    source_ordinal: u32,
    time: Option<String>,
    positive_prompt: Option<String>,
    negative_prompt: Option<String>,
    artists: Option<String>,
    image_folder: Option<String>,
    image_path: Option<String>,
    stored_image_path: Option<String>,
    metadata_failed: bool,
    tags: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagSummaryDto {
    name: String,
    row_count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum RowSelectionDto {
    Explicit {
        row_ids: Vec<i64>,
    },
    Filtered {
        tags: Vec<String>,
        tag_mode: TagMatchModeDto,
        excluded_row_ids: Vec<i64>,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagMutationResultDto {
    affected_rows: u64,
    normalized_tags: Vec<String>,
    associations_changed: usize,
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
pub(crate) fn initialize_data_directory(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .initialize_directory(PathBuf::from(path))
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn open_data_directory(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .open_directory(PathBuf::from(path))
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn import_workbook(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<ImportResultDto, String> {
    runtime
        .import_workbook(PathBuf::from(path))
        .map(|(snapshot, outcome)| ImportResultDto {
            snapshot: snapshot.into(),
            added: outcome.added,
            skipped_existing: outcome.skipped_existing,
            changed_existing: outcome.changed_existing,
            embedded_images_stored: outcome.embedded_images_stored,
        })
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
                let _ = app.emit(
                    "import-images://progress",
                    ImageImportProgressDto::from(progress),
                );
            })
            .map(|(snapshot, outcome)| ImageImportResultDto {
                snapshot: snapshot.into(),
                source_type: outcome.source_type.as_str(),
                total_found: outcome.total_found,
                added: outcome.added,
                skipped_existing: outcome.skipped_existing,
                skipped_content: outcome.skipped_content,
                changed_existing: outcome.changed_existing,
                metadata_failed: outcome.metadata_failed,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("导入任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn delete_rows(
    selection: RowSelectionDto,
    runtime: State<'_, AppRuntime>,
) -> Result<DeleteResultDto, String> {
    runtime
        .delete_rows(&selection.into())
        .map(|(snapshot, report)| DeleteResultDto {
            snapshot: snapshot.into(),
            deleted_rows: report.deleted_rows,
            cleanup_failures: report.cleanup_failures,
        })
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn find_duplicates(
    key: DuplicateKeyDto,
    group_limit: u32,
    runtime: State<'_, AppRuntime>,
) -> Result<DuplicateReportDto, String> {
    runtime
        .find_duplicates(key.into(), group_limit)
        .map(DuplicateReportDto::from)
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
pub(crate) fn query_rows(
    query: RowQueryDto,
    runtime: State<'_, AppRuntime>,
) -> Result<RowPageDto, String> {
    let query = RowQuery {
        offset: query.offset,
        limit: query.limit,
        tags: query.tags,
        tag_mode: query.tag_mode.into(),
    };
    runtime
        .query_rows(&query)
        .map(RowPageDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn list_tags(runtime: State<'_, AppRuntime>) -> Result<Vec<TagSummaryDto>, String> {
    runtime
        .list_tags()
        .map(|tags| tags.into_iter().map(TagSummaryDto::from).collect())
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn create_tag(name: String, runtime: State<'_, AppRuntime>) -> Result<bool, String> {
    runtime.create_tag(&name).map_err(error_text)
}

#[tauri::command]
pub(crate) fn count_selected_rows(
    selection: RowSelectionDto,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .count_selected_rows(&selection.into())
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn add_tags_to_selection(
    selection: RowSelectionDto,
    tags: Vec<String>,
    runtime: State<'_, AppRuntime>,
) -> Result<TagMutationResultDto, String> {
    runtime
        .add_tags_to_selection(&selection.into(), &tags)
        .map(TagMutationResultDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn remove_tags_from_selection(
    selection: RowSelectionDto,
    tags: Vec<String>,
    runtime: State<'_, AppRuntime>,
) -> Result<TagMutationResultDto, String> {
    runtime
        .remove_tags_from_selection(&selection.into(), &tags)
        .map(TagMutationResultDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn set_tags_for_row(
    row_id: i64,
    tags: Vec<String>,
    runtime: State<'_, AppRuntime>,
) -> Result<TagMutationResultDto, String> {
    runtime
        .set_tags_for_row(row_id, &tags)
        .map(TagMutationResultDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn get_row_thumbnail(
    row_id: i64,
    runtime: State<'_, AppRuntime>,
) -> Result<Response, String> {
    runtime
        .row_thumbnail(row_id)
        .map(Response::new)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn get_row_preview(
    row_id: i64,
    runtime: State<'_, AppRuntime>,
) -> Result<Response, String> {
    runtime
        .row_preview(row_id)
        .map(Response::new)
        .map_err(error_text)
}

/// 导出带缩略图的 xlsx；在阻塞线程上执行，进度经 `export://progress` 推送。
#[tauri::command]
pub(crate) async fn export_xlsx(
    selection: RowSelectionDto,
    path: String,
    app: tauri::AppHandle,
) -> Result<XlsxExportResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_xlsx(&selection.into(), PathBuf::from(path), |progress| {
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
    selection: RowSelectionDto,
    path: String,
    app: tauri::AppHandle,
) -> Result<JsonExportResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_zhihuiji_json(&selection.into(), PathBuf::from(path), |progress| {
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
    selection: RowSelectionDto,
    parent_dir: String,
    mode: String,
    app: tauri::AppHandle,
) -> Result<ImageFilesExportResultDto, String> {
    let mode = crate::storage::ImageFileExportMode::parse(&mode)
        .ok_or_else(|| format!("未知的图片导出方式: {mode}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_image_files(&selection.into(), PathBuf::from(parent_dir), mode, |progress| {
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

impl From<TagMatchModeDto> for TagMatchMode {
    fn from(mode: TagMatchModeDto) -> Self {
        match mode {
            TagMatchModeDto::And => Self::And,
            TagMatchModeDto::Or => Self::Or,
        }
    }
}

impl From<RowPage> for RowPageDto {
    fn from(page: RowPage) -> Self {
        let has_more = page.has_more();
        Self {
            rows: page.rows.into_iter().map(RowRecordDto::from).collect(),
            total_count: page.total_count,
            offset: page.offset,
            limit: page.limit,
            has_more,
        }
    }
}

impl From<RowRecord> for RowRecordDto {
    fn from(row: RowRecord) -> Self {
        Self {
            id: row.id,
            batch_id: row.batch_id,
            source_ordinal: row.source_ordinal,
            time: row.time,
            positive_prompt: row.positive_prompt,
            negative_prompt: row.negative_prompt,
            artists: row.artists,
            image_folder: row.image_folder,
            image_path: row.image_path,
            stored_image_path: row.stored_image_path,
            metadata_failed: row.metadata_failed,
            tags: row.tags,
        }
    }
}

impl From<TagSummary> for TagSummaryDto {
    fn from(summary: TagSummary) -> Self {
        Self {
            name: summary.name,
            row_count: summary.row_count,
        }
    }
}

impl From<RowSelectionDto> for RowSelection {
    fn from(selection: RowSelectionDto) -> Self {
        match selection {
            RowSelectionDto::Explicit { row_ids } => Self::Explicit { row_ids },
            RowSelectionDto::Filtered {
                tags,
                tag_mode,
                excluded_row_ids,
            } => Self::Filtered {
                tags,
                tag_mode: tag_mode.into(),
                excluded_row_ids,
            },
        }
    }
}

impl From<TagMutationResult> for TagMutationResultDto {
    fn from(result: TagMutationResult) -> Self {
        Self {
            affected_rows: result.affected_rows,
            normalized_tags: result.normalized_tags,
            associations_changed: result.associations_changed,
        }
    }
}

fn error_text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_filtered_selection_without_losing_case_or_exclusions() {
        let selection = RowSelection::from(RowSelectionDto::Filtered {
            tags: vec!["Landscape".into(), "landscape".into()],
            tag_mode: TagMatchModeDto::Or,
            excluded_row_ids: vec![2, 9],
        });

        assert_eq!(
            selection,
            RowSelection::Filtered {
                tags: vec!["Landscape".into(), "landscape".into()],
                tag_mode: TagMatchMode::Or,
                excluded_row_ids: vec![2, 9],
            }
        );
    }
}
