use crate::{
    app::AppRuntime,
    db::{RowSelection, style_extraction::*},
};
use std::path::Path;
use tauri::Manager;

#[tauri::command]
pub(crate) async fn export_style_request(
    selection: Option<RowSelection>,
    include_processed: bool,
    path: String,
    app: tauri::AppHandle,
) -> Result<ExportSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>()
            .export_style_request(selection.as_ref(), include_processed, Path::new(&path))
            .map_err(super::error_text)
    })
    .await
    .map_err(super::error_text)?
}
#[tauri::command]
pub(crate) async fn preview_style_result(
    path: String,
    app: tauri::AppHandle,
) -> Result<ImportPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>()
            .preview_style_result(Path::new(&path))
            .map_err(super::error_text)
    })
    .await
    .map_err(super::error_text)?
}
#[tauri::command]
pub(crate) async fn apply_style_changes(
    library_id: String,
    changes: Vec<StyleChange>,
    reverse: bool,
    strict: bool,
    app: tauri::AppHandle,
) -> Result<ApplyResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>()
            .apply_style_changes(&library_id, &changes, reverse, strict)
            .map_err(super::error_text)
    })
    .await
    .map_err(super::error_text)?
}
