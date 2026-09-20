use super::dto::{DedupeClusterDto, RowPageDto};
use super::error_text;
use crate::app::AppRuntime;
use crate::db::{DedupeMode, LibraryFilter, TagMatchMode};
use tauri::State;

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) fn list_dedupe_clusters(
    dedupe: DedupeMode,
    tags: Vec<String>,
    tag_mode: TagMatchMode,
    single_artist_only: bool,
    has_vibe: bool,
    untagged_only: bool,
    filters: Vec<LibraryFilter>,
    hide_grouped: bool,
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<DedupeClusterDto>, String> {
    runtime
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
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) fn get_dedupe_cluster_members(
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
    runtime: State<'_, AppRuntime>,
) -> Result<RowPageDto, String> {
    runtime
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
        )
        .map(RowPageDto::from)
        .map_err(error_text)
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
