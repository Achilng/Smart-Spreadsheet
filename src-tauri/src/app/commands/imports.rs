use super::dto::{
    BatchSummaryDto, DeleteResultDto, ExistingImageUpdateResultDto, ImageImportResultDto,
};
use super::error_text;
use crate::app::AppRuntime;
use crate::db::RowSelection;
use std::path::PathBuf;
use tauri::{Emitter, Manager, State};

/// 文件夹/压缩包导入：在阻塞线程上执行避免卡住 UI，进度经
/// `import-images://progress` 事件推送给前端。
#[tauri::command]
pub(crate) async fn import_images(
    path: String,
    app: tauri::AppHandle,
) -> Result<ImageImportResultDto, String> {
    crate::pipeline::cancel::begin();
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .import_images(PathBuf::from(path), |progress| {
                let _ = app.emit("import-images://progress", progress);
            })
            .map(|(snapshot, outcome)| ImageImportResultDto {
                snapshot: snapshot.into(),
                batch_id: outcome.batch_id,
                source_type: outcome.source_type.as_str(),
                total_found: outcome.total_found,
                added: outcome.added,
                skipped_existing: outcome.skipped_existing,
                skipped_content: outcome.skipped_content,
                changed_existing: outcome.changed_existing,
                metadata_rejected: outcome.metadata_rejected,
                rejected_moved: outcome.rejected_moved,
                rejected_move_failures: outcome.rejected_move_failures,
                rule_execution: outcome.rule_execution,
                artist_prefix_enabled: outcome.artist_prefix_enabled,
                artist_prefix_scanned_rows: outcome.artist_prefix_scanned_rows,
                artist_prefix_changed_rows: outcome.artist_prefix_changed_rows,
                artist_prefix_changed_fields: outcome.artist_prefix_changed_fields,
                artist_prefix_error: outcome.artist_prefix_error,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("导入任务异常中止: {error}"))?
}

/// 请求取消当前长任务（导入等支持取消的任务会在阶段边界响应）。
#[tauri::command]
pub(crate) fn cancel_current_task() {
    crate::pipeline::cancel::request();
}

/// 仅更新身份键已存在的图片；进度复用 `import-images://progress`。
#[tauri::command]
pub(crate) async fn update_existing_images(
    path: String,
    app: tauri::AppHandle,
) -> Result<ExistingImageUpdateResultDto, String> {
    crate::pipeline::cancel::begin();
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
                matched_by_identity: outcome.matched_by_identity,
                relinked_by_content: outcome.relinked_by_content,
                relinked_by_metadata: outcome.relinked_by_metadata,
                ambiguous: outcome.ambiguous,
                unmatched: outcome.unmatched,
                metadata_rejected: outcome.metadata_rejected,
                copy_failures: outcome.copy_failures,
                rule_execution: outcome.rule_execution,
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
pub(crate) fn undo_import_batch(
    batch_id: i64,
    runtime: State<'_, AppRuntime>,
) -> Result<DeleteResultDto, String> {
    runtime
        .undo_import_batch(batch_id)
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
