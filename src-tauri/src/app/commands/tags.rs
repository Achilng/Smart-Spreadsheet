use super::error_text;
use crate::app::AppRuntime;
use crate::db::{RowSelection, TagMutationResult, TagSelectionSummary, TagSummary};
use tauri::State;

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
pub(crate) fn rename_tag(
    old_name: String,
    new_name: String,
    runtime: State<'_, AppRuntime>,
) -> Result<bool, String> {
    runtime.rename_tag(&old_name, &new_name).map_err(error_text)
}

#[tauri::command]
pub(crate) fn get_recent_tags(runtime: State<'_, AppRuntime>) -> Result<String, String> {
    runtime.get_recent_tags().map_err(error_text)
}

#[tauri::command]
pub(crate) fn set_recent_tags(json: String, runtime: State<'_, AppRuntime>) -> Result<(), String> {
    runtime.set_recent_tags(&json).map_err(error_text)
}

#[tauri::command]
pub(crate) fn list_selection_tags(
    selection: RowSelection,
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<TagSelectionSummary>, String> {
    runtime.list_selection_tags(&selection).map_err(error_text)
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
    runtime.set_tags_for_row(row_id, &tags).map_err(error_text)
}
