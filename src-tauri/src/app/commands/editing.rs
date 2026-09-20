use super::error_text;
use crate::app::AppRuntime;
use crate::db::{MutableRowState, PromptEditResult, RowSelection, SinglePromptEditResult};
use tauri::State;

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
pub(crate) fn restore_mutable_row_states(
    states: Vec<MutableRowState>,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .restore_mutable_row_states(&states)
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
