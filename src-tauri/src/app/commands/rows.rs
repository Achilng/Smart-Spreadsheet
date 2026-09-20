use super::dto::RowPageDto;
use super::error_text;
use crate::app::AppRuntime;
use crate::db::{RowQuery, RowRecord, RowSelection, SortMode};
use tauri::{Manager, State};

#[tauri::command]
pub(crate) async fn query_rows(
    query: RowQuery,
    sort: Option<SortMode>,
    app: tauri::AppHandle,
) -> Result<RowPageDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>()
            .query_rows_sorted(&query, sort.unwrap_or_default())
            .map(RowPageDto::from)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("搜索任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn get_rows_by_ids(
    row_ids: Vec<i64>,
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<RowRecord>, String> {
    runtime.get_rows_by_ids(&row_ids).map_err(error_text)
}

#[tauri::command]
pub(crate) fn get_row_index(
    row_id: i64,
    sort: Option<SortMode>,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .row_index_by_id_sorted(row_id, sort.unwrap_or_default())
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn count_selected_rows(
    selection: RowSelection,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime.count_selected_rows(&selection).map_err(error_text)
}

#[tauri::command]
pub(crate) fn selected_row_ids(
    selection: RowSelection,
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<i64>, String> {
    runtime.selected_row_ids(&selection).map_err(error_text)
}
