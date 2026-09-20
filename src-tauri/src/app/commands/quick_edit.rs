use super::error_text;
use crate::app::AppRuntime;
use crate::db::{
    ArtistTextPrefixResult, AutoArtistPrefixApplyResult, AutoArtistPrefixPreview,
    QuickArtistPrefixApplyResult, QuickArtistPrefixChange, QuickArtistPrefixPreview,
    QuickEditCondition, QuickGroupApplyResult, QuickGroupChange, QuickGroupPreview,
    QuickTagApplyResult, QuickTagAssociation, QuickTagPreview,
};
use tauri::{Manager, State};

#[tauri::command]
pub(crate) async fn preview_quick_tag(
    condition: QuickEditCondition,
    tags: Vec<String>,
    app: tauri::AppHandle,
) -> Result<QuickTagPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .preview_quick_tag(&condition, &tags)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("快速整理预览任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) async fn apply_quick_tag(
    condition: QuickEditCondition,
    tags: Vec<String>,
    app: tauri::AppHandle,
) -> Result<QuickTagApplyResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .apply_quick_tag(&condition, &tags)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("快速打标任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn revert_quick_tag_changes(
    changes: Vec<QuickTagAssociation>,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .revert_quick_tag_changes(&changes)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn reapply_quick_tag_changes(
    changes: Vec<QuickTagAssociation>,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .reapply_quick_tag_changes(&changes)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) async fn preview_quick_group(
    condition: QuickEditCondition,
    group_id: i64,
    only_ungrouped: bool,
    app: tauri::AppHandle,
) -> Result<QuickGroupPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .preview_quick_group(&condition, group_id, only_ungrouped)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("快速整理预览任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) async fn apply_quick_group(
    condition: QuickEditCondition,
    group_id: i64,
    only_ungrouped: bool,
    app: tauri::AppHandle,
) -> Result<QuickGroupApplyResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .apply_quick_group(&condition, group_id, only_ungrouped)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("批量分组任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn revert_quick_group_changes(
    changes: Vec<QuickGroupChange>,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .revert_quick_group_changes(&changes)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn reapply_quick_group_changes(
    changes: Vec<QuickGroupChange>,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .reapply_quick_group_changes(&changes)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) async fn preview_quick_artist_prefix(
    artist_name: String,
    app: tauri::AppHandle,
) -> Result<QuickArtistPrefixPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .preview_quick_artist_prefix(&artist_name)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("画师前缀预览任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) async fn apply_quick_artist_prefix(
    artist_name: String,
    app: tauri::AppHandle,
) -> Result<QuickArtistPrefixApplyResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .apply_quick_artist_prefix(&artist_name)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("画师前缀修正任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn revert_quick_artist_prefix_changes(
    changes: Vec<QuickArtistPrefixChange>,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .revert_quick_artist_prefix_changes(&changes)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn reapply_quick_artist_prefix_changes(
    changes: Vec<QuickArtistPrefixChange>,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .reapply_quick_artist_prefix_changes(&changes)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) async fn preview_auto_artist_prefix(
    app: tauri::AppHandle,
) -> Result<AutoArtistPrefixPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>()
            .preview_auto_artist_prefix()
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("画师 Tag 扫描任务失败: {error}"))?
}

#[tauri::command]
pub(crate) async fn prefix_confirmed_artists_in_text(
    text: String,
    app: tauri::AppHandle,
) -> Result<ArtistTextPrefixResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>()
            .prefix_confirmed_artists_in_text(&text)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("纯文本画师前缀处理任务失败: {error}"))?
}

#[tauri::command]
pub(crate) async fn apply_auto_artist_prefix(
    selected_names: Vec<String>,
    app: tauri::AppHandle,
) -> Result<AutoArtistPrefixApplyResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>()
            .apply_auto_artist_prefix(&selected_names)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("画师前缀修正任务失败: {error}"))?
}
