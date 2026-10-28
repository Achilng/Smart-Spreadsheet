use super::dto::RowPageDto;
use super::error_text;
use crate::app::AppRuntime;
use crate::db::{GroupSummary, RowSelection, SortMode};
use tauri::{Manager, State};

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
pub(crate) async fn list_groups(app: tauri::AppHandle) -> Result<Vec<GroupSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>().list_groups().map_err(error_text)
    })
    .await
    .map_err(|error| format!("读取任务异常中止: {error}"))?
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
pub(crate) async fn get_group_members(
    group_id: i64,
    offset: u64,
    limit: u32,
    sort: Option<SortMode>,
    app: tauri::AppHandle,
) -> Result<RowPageDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>()
            .get_group_members(group_id, offset, limit, sort.unwrap_or_default())
            .map(RowPageDto::from)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("读取任务异常中止: {error}"))?
}
