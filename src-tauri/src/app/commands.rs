use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use super::runtime::{AppRuntime, RuntimeSnapshot};
use crate::db::WorkbookSummary;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSnapshotDto {
    data_directory: Option<String>,
    workbook: Option<WorkbookSummaryDto>,
    startup_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkbookSummaryDto {
    imported_name: String,
    imported_at: String,
    sheet_name: String,
    row_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportResultDto {
    snapshot: AppSnapshotDto,
    imported_rows: usize,
    embedded_images: usize,
    previous_copy_cleanup: Option<String>,
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
            imported_rows: outcome.row_count,
            embedded_images: outcome.embedded_image_count,
            previous_copy_cleanup: outcome
                .previous_copy_cleanup
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
            workbook: snapshot.workbook.map(WorkbookSummaryDto::from),
            startup_error: snapshot.startup_error,
        }
    }
}

impl From<WorkbookSummary> for WorkbookSummaryDto {
    fn from(summary: WorkbookSummary) -> Self {
        Self {
            imported_name: summary.imported_name,
            imported_at: summary.imported_at,
            sheet_name: summary.sheet_name,
            row_count: summary.row_count,
        }
    }
}

fn error_text(error: impl std::fmt::Display) -> String {
    error.to_string()
}
