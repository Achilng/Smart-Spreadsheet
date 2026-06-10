use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use super::runtime::{AppRuntime, RuntimeSnapshot};
use crate::db::{
    RowPage, RowQuery, RowRecord, RowSelection, TagMatchMode, TagMutationResult, TagSummary,
    WorkbookSummary,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSnapshotDto {
    data_directory: Option<String>,
    workbook: Option<WorkbookSummaryDto>,
    startup_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkbookSummaryDto {
    imported_name: String,
    imported_at: String,
    sheet_name: String,
    row_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportResultDto {
    snapshot: AppSnapshotDto,
    imported_rows: usize,
    embedded_images: usize,
    previous_copy_cleanup: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowQueryDto {
    offset: u64,
    limit: u32,
    tags: Vec<String>,
    tag_mode: TagMatchModeDto,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TagMatchModeDto {
    And,
    Or,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowPageDto {
    rows: Vec<RowRecordDto>,
    total_count: u64,
    offset: u64,
    limit: u32,
    has_more: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RowRecordDto {
    id: i64,
    source_row: u32,
    time: Option<String>,
    positive_prompt: Option<String>,
    negative_prompt: Option<String>,
    artists: Option<String>,
    image_folder: Option<String>,
    image_path: Option<String>,
    embedded_image_ref: Option<String>,
    tags: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagSummaryDto {
    name: String,
    row_count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum RowSelectionDto {
    Explicit {
        row_ids: Vec<i64>,
    },
    Filtered {
        tags: Vec<String>,
        tag_mode: TagMatchModeDto,
        excluded_row_ids: Vec<i64>,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagMutationResultDto {
    affected_rows: u64,
    normalized_tags: Vec<String>,
    associations_changed: usize,
}

#[tauri::command]
pub(crate) fn get_app_snapshot(runtime: State<'_, AppRuntime>) -> Result<AppSnapshotDto, String> {
    runtime
        .snapshot()
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn initialize_data_directory(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .initialize_directory(PathBuf::from(path))
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn open_data_directory(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .open_directory(PathBuf::from(path))
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn import_workbook(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<ImportResultDto, String> {
    runtime
        .import_workbook(PathBuf::from(path))
        .map(|(snapshot, outcome)| ImportResultDto {
            snapshot: snapshot.into(),
            imported_rows: outcome.row_count,
            embedded_images: outcome.embedded_image_count,
            previous_copy_cleanup: outcome
                .previous_copy_cleanup
                .map(|path| path.to_string_lossy().into_owned()),
        })
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn query_rows(
    query: RowQueryDto,
    runtime: State<'_, AppRuntime>,
) -> Result<RowPageDto, String> {
    let query = RowQuery {
        offset: query.offset,
        limit: query.limit,
        tags: query.tags,
        tag_mode: query.tag_mode.into(),
    };
    runtime
        .query_rows(&query)
        .map(RowPageDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn list_used_tags(runtime: State<'_, AppRuntime>) -> Result<Vec<TagSummaryDto>, String> {
    runtime
        .list_used_tags()
        .map(|tags| tags.into_iter().map(TagSummaryDto::from).collect())
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn count_selected_rows(
    selection: RowSelectionDto,
    runtime: State<'_, AppRuntime>,
) -> Result<u64, String> {
    runtime
        .count_selected_rows(&selection.into())
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn add_tags_to_selection(
    selection: RowSelectionDto,
    tags: Vec<String>,
    runtime: State<'_, AppRuntime>,
) -> Result<TagMutationResultDto, String> {
    runtime
        .add_tags_to_selection(&selection.into(), &tags)
        .map(TagMutationResultDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn remove_tags_from_selection(
    selection: RowSelectionDto,
    tags: Vec<String>,
    runtime: State<'_, AppRuntime>,
) -> Result<TagMutationResultDto, String> {
    runtime
        .remove_tags_from_selection(&selection.into(), &tags)
        .map(TagMutationResultDto::from)
        .map_err(error_text)
}

impl From<RuntimeSnapshot> for AppSnapshotDto {
    fn from(snapshot: RuntimeSnapshot) -> Self {
        Self {
            data_directory: snapshot
                .data_directory
                .map(|path| path.to_string_lossy().into_owned()),
            workbook: snapshot.workbook.map(WorkbookSummaryDto::from),
            startup_error: snapshot.startup_error,
        }
    }
}

impl From<WorkbookSummary> for WorkbookSummaryDto {
    fn from(summary: WorkbookSummary) -> Self {
        Self {
            imported_name: summary.imported_name,
            imported_at: summary.imported_at,
            sheet_name: summary.sheet_name,
            row_count: summary.row_count,
        }
    }
}

impl From<TagMatchModeDto> for TagMatchMode {
    fn from(mode: TagMatchModeDto) -> Self {
        match mode {
            TagMatchModeDto::And => Self::And,
            TagMatchModeDto::Or => Self::Or,
        }
    }
}

impl From<RowPage> for RowPageDto {
    fn from(page: RowPage) -> Self {
        let has_more = page.has_more();
        Self {
            rows: page.rows.into_iter().map(RowRecordDto::from).collect(),
            total_count: page.total_count,
            offset: page.offset,
            limit: page.limit,
            has_more,
        }
    }
}

impl From<RowRecord> for RowRecordDto {
    fn from(row: RowRecord) -> Self {
        Self {
            id: row.id,
            source_row: row.source_row,
            time: row.time,
            positive_prompt: row.positive_prompt,
            negative_prompt: row.negative_prompt,
            artists: row.artists,
            image_folder: row.image_folder,
            image_path: row.image_path,
            embedded_image_ref: row.embedded_image_ref,
            tags: row.tags,
        }
    }
}

impl From<TagSummary> for TagSummaryDto {
    fn from(summary: TagSummary) -> Self {
        Self {
            name: summary.name,
            row_count: summary.row_count,
        }
    }
}

impl From<RowSelectionDto> for RowSelection {
    fn from(selection: RowSelectionDto) -> Self {
        match selection {
            RowSelectionDto::Explicit { row_ids } => Self::Explicit { row_ids },
            RowSelectionDto::Filtered {
                tags,
                tag_mode,
                excluded_row_ids,
            } => Self::Filtered {
                tags,
                tag_mode: tag_mode.into(),
                excluded_row_ids,
            },
        }
    }
}

impl From<TagMutationResult> for TagMutationResultDto {
    fn from(result: TagMutationResult) -> Self {
        Self {
            affected_rows: result.affected_rows,
            normalized_tags: result.normalized_tags,
            associations_changed: result.associations_changed,
        }
    }
}

fn error_text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_filtered_selection_without_losing_case_or_exclusions() {
        let selection = RowSelection::from(RowSelectionDto::Filtered {
            tags: vec!["Landscape".into(), "landscape".into()],
            tag_mode: TagMatchModeDto::Or,
            excluded_row_ids: vec![2, 9],
        });

        assert_eq!(
            selection,
            RowSelection::Filtered {
                tags: vec!["Landscape".into(), "landscape".into()],
                tag_mode: TagMatchMode::Or,
                excluded_row_ids: vec![2, 9],
            }
        );
    }
}
