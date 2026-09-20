use crate::app::runtime::RuntimeSnapshot;
use crate::db::{
    BatchSummary, DedupeCluster, LibrarySummary, RowPage, RowRecord, RuleExecutionSummary,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileDragInfo {
    pub(super) file_paths: Vec<String>,
    pub(super) icon_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSnapshotDto {
    pub(super) data_directory: Option<String>,
    pub(super) rejected_images_directory: Option<String>,
    pub(super) library: Option<LibrarySummaryDto>,
    pub(super) auto_artist_prefix_on_import: bool,
    pub(super) startup_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LibrarySummaryDto {
    pub(super) row_count: u64,
    pub(super) batch_count: u64,
    pub(super) last_batch: Option<BatchSummaryDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchSummaryDto {
    pub(super) id: i64,
    pub(super) source_type: &'static str,
    pub(super) source_path: String,
    pub(super) imported_at: String,
    pub(super) added_count: u64,
    pub(super) skipped_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeleteResultDto {
    pub(super) snapshot: AppSnapshotDto,
    pub(super) deleted_rows: u64,
    pub(super) cleanup_failures: usize,
    pub(super) trashed_original_files: usize,
    pub(super) original_file_failures: usize,
    pub(super) archive_rows_skipped: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageImportResultDto {
    pub(super) snapshot: AppSnapshotDto,
    pub(super) batch_id: i64,
    pub(super) source_type: &'static str,
    pub(super) total_found: usize,
    pub(super) added: u64,
    pub(super) skipped_existing: u64,
    pub(super) skipped_content: u64,
    pub(super) changed_existing: u64,
    pub(super) metadata_rejected: u64,
    pub(super) rejected_moved: u64,
    pub(super) rejected_move_failures: u64,
    pub(super) rule_execution: RuleExecutionSummary,
    pub(super) artist_prefix_enabled: bool,
    pub(super) artist_prefix_scanned_rows: u64,
    pub(super) artist_prefix_changed_rows: u64,
    pub(super) artist_prefix_changed_fields: u64,
    pub(super) artist_prefix_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExistingImageUpdateResultDto {
    pub(super) snapshot: AppSnapshotDto,
    pub(super) source_type: &'static str,
    pub(super) total_found: usize,
    pub(super) matched: u64,
    pub(super) updated: u64,
    pub(super) matched_by_identity: u64,
    pub(super) relinked_by_content: u64,
    pub(super) relinked_by_metadata: u64,
    pub(super) ambiguous: u64,
    pub(super) unmatched: u64,
    pub(super) metadata_rejected: u64,
    pub(super) copy_failures: u64,
    pub(super) rule_execution: RuleExecutionSummary,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowPageDto {
    pub(super) rows: Vec<RowRecord>,
    pub(super) total_count: u64,
    pub(super) offset: u64,
    pub(super) limit: u32,
    pub(super) has_more: bool,
}

/// 三种导出共用的进度事件载荷，经 `export://progress` 推送。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportProgressDto {
    pub(super) processed: usize,
    pub(super) total: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DedupeClusterDto {
    pub(super) key: String,
    pub(super) member_count: u64,
    pub(super) alias: Option<String>,
}

impl From<DedupeCluster> for DedupeClusterDto {
    fn from(cluster: DedupeCluster) -> Self {
        Self {
            key: cluster.key,
            member_count: cluster.member_count,
            alias: cluster.alias,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XlsxExportResultDto {
    pub(super) path: String,
    pub(super) row_count: usize,
    pub(super) images_embedded: usize,
    pub(super) image_failures: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonExportResultDto {
    pub(super) path: String,
    pub(super) exported: usize,
    pub(super) duplicates_removed: usize,
    pub(super) artists_added: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptRotationJsonExportResultDto {
    pub(super) path: String,
    pub(super) exported: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonExportNoteInspectionDto {
    pub(super) total: usize,
    pub(super) empty_notes: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageFilesExportResultDto {
    pub(super) directory: String,
    pub(super) exported: usize,
    pub(super) hardlink_fallbacks: usize,
    pub(super) missing: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MigrationResultDto {
    pub(super) snapshot: AppSnapshotDto,
    pub(super) retired_source: Option<String>,
}

impl From<RuntimeSnapshot> for AppSnapshotDto {
    fn from(snapshot: RuntimeSnapshot) -> Self {
        Self {
            data_directory: snapshot
                .data_directory
                .map(|path| path.to_string_lossy().into_owned()),
            rejected_images_directory: snapshot
                .rejected_images_directory
                .map(|path| path.to_string_lossy().into_owned()),
            library: snapshot.library.map(LibrarySummaryDto::from),
            auto_artist_prefix_on_import: snapshot.auto_artist_prefix_on_import,
            startup_error: snapshot.startup_error,
        }
    }
}

impl From<LibrarySummary> for LibrarySummaryDto {
    fn from(summary: LibrarySummary) -> Self {
        Self {
            row_count: summary.row_count,
            batch_count: summary.batch_count,
            last_batch: summary.last_batch.map(BatchSummaryDto::from),
        }
    }
}

impl From<BatchSummary> for BatchSummaryDto {
    fn from(batch: BatchSummary) -> Self {
        Self {
            id: batch.id,
            source_type: batch.source_type.as_str(),
            source_path: batch.source_path,
            imported_at: batch.imported_at,
            added_count: batch.added_count,
            skipped_count: batch.skipped_count,
        }
    }
}

impl From<RowPage> for RowPageDto {
    fn from(page: RowPage) -> Self {
        let has_more = page.has_more();
        Self {
            rows: page.rows,
            total_count: page.total_count,
            offset: page.offset,
            limit: page.limit,
            has_more,
        }
    }
}
