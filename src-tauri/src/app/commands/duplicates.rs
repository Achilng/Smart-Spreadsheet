use super::dto::{DedupeClusterDto, RowPageDto};
use super::error_text;
use crate::app::AppRuntime;
use crate::db::{DedupeMode, LibraryFilter, SortMode, TagMatchMode};
use tauri::{Manager, State};

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn list_dedupe_clusters(
    dedupe: DedupeMode,
    tags: Vec<String>,
    tag_mode: TagMatchMode,
    single_artist_only: bool,
    has_vibe: bool,
    untagged_only: bool,
    filters: Vec<LibraryFilter>,
    hide_grouped: bool,
    app: tauri::AppHandle,
) -> Result<Vec<DedupeClusterDto>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>()
            .list_dedupe_clusters(
                dedupe,
                &tags,
                tag_mode,
                single_artist_only,
                has_vibe,
                untagged_only,
                &filters,
                hide_grouped,
            )
            .map(|clusters| clusters.into_iter().map(DedupeClusterDto::from).collect())
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("读取任务异常中止: {error}"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn get_dedupe_cluster_members(
    dedupe: DedupeMode,
    key: String,
    tags: Vec<String>,
    tag_mode: TagMatchMode,
    single_artist_only: bool,
    has_vibe: bool,
    untagged_only: bool,
    filters: Vec<LibraryFilter>,
    hide_grouped: bool,
    offset: u64,
    limit: u32,
    sort: Option<SortMode>,
    app: tauri::AppHandle,
) -> Result<RowPageDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>()
            .get_dedupe_cluster_members(
                dedupe,
                &key,
                &tags,
                tag_mode,
                single_artist_only,
                has_vibe,
                untagged_only,
                &filters,
                hide_grouped,
                offset,
                limit,
                sort.unwrap_or_default(),
            )
            .map(RowPageDto::from)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("读取任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn set_dedupe_alias(
    dedupe: DedupeMode,
    key: String,
    alias: String,
    runtime: State<'_, AppRuntime>,
) -> Result<(), String> {
    runtime
        .set_dedupe_alias(dedupe, &key, &alias)
        .map_err(error_text)
}
