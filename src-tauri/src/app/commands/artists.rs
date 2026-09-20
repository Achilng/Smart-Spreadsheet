use super::error_text;
use crate::app::AppRuntime;
use tauri::State;

#[tauri::command]
pub(crate) fn list_distinct_artists(runtime: State<'_, AppRuntime>) -> Result<Vec<String>, String> {
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
