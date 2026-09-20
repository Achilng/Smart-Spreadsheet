use super::dto::RowPageDto;
use super::error_text;
use crate::app::AppRuntime;
use crate::db::{GroupSummary, RowSelection};
use tauri::State;

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
    runtime
        .rename_group(group_id, &new_name)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn delete_group(group_id: i64, runtime: State<'_, AppRuntime>) -> Result<bool, String> {
    runtime.delete_group(group_id).map_err(error_text)
}

#[tauri::command]
pub(crate) fn delete_empty_groups(runtime: State<'_, AppRuntime>) -> Result<u64, String> {
    runtime.delete_empty_groups().map_err(error_text)
}

#[tauri::command]
pub(crate) fn list_groups(runtime: State<'_, AppRuntime>) -> Result<Vec<GroupSummary>, String> {
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
pub(crate) fn restore_group(
    group: GroupSummary,
    runtime: State<'_, AppRuntime>,
) -> Result<GroupSummary, String> {
    runtime.restore_group(&group).map_err(error_text)
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
