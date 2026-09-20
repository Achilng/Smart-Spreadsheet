use super::error_text;
use crate::app::AppRuntime;
use crate::db::{CompareModelSection, CompareSample, CompareSectionPage};
use tauri::Manager;

/// 图片对比窗口：样本完整信息（提示词、画师串、参数与签名状态）。
/// 样本行不存在（打开期间被删除）时返回明确错误。
#[tauri::command]
pub(crate) async fn get_compare_sample(
    row_id: i64,
    app: tauri::AppHandle,
) -> Result<CompareSample, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime.get_compare_sample(row_id).map_err(error_text)
    })
    .await
    .map_err(|error| format!("对比任务异常中止: {error}"))?
}

/// 对比分区①：画师串整串精确相同的行（时间倒序分页）。
#[tauri::command]
pub(crate) async fn query_compare_same_artists(
    row_id: i64,
    offset: u64,
    limit: u32,
    app: tauri::AppHandle,
) -> Result<CompareSectionPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .query_compare_same_artists(row_id, offset, limit)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("对比任务异常中止: {error}"))?
}

/// 对比分区②：相同 VIBE 组合且提示词（画风签名）不同的行。
#[tauri::command]
pub(crate) async fn query_compare_same_vibe_diff_style(
    row_id: i64,
    offset: u64,
    limit: u32,
    app: tauri::AppHandle,
) -> Result<CompareSectionPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .query_compare_same_vibe_diff_style(row_id, offset, limit)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("对比任务异常中止: {error}"))?
}

/// 对比分区③：相同提示词（画风签名）且 VIBE 组合不同的行。
#[tauri::command]
pub(crate) async fn query_compare_same_style_diff_vibe(
    row_id: i64,
    offset: u64,
    limit: u32,
    app: tauri::AppHandle,
) -> Result<CompareSectionPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .query_compare_same_style_diff_vibe(row_id, offset, limit)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("对比任务异常中止: {error}"))?
}

/// 对比分区④：提示词（画风签名）相同的全部行，前端按模型版本分组。
#[tauri::command]
pub(crate) async fn query_compare_same_style_all_models(
    row_id: i64,
    app: tauri::AppHandle,
) -> Result<CompareModelSection, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .query_compare_same_style_all_models(row_id)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("对比任务异常中止: {error}"))?
}
