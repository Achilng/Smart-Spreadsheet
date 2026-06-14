import { invoke } from "@tauri-apps/api/core";

export type SourceType = "xlsx" | "folder" | "archive";

export interface BatchSummary {
  id: number;
  sourceType: SourceType;
  sourcePath: string;
  importedAt: string;
  addedCount: number;
  skippedCount: number;
}

export interface LibrarySummary {
  rowCount: number;
  batchCount: number;
  lastBatch: BatchSummary | null;
}

export interface AppSnapshot {
  dataDirectory: string | null;
  rejectedImagesDirectory: string | null;
  library: LibrarySummary | null;
  startupError: string | null;
}

export interface DeleteResult {
  snapshot: AppSnapshot;
  deletedRows: number;
  cleanupFailures: number;
  trashedOriginalFiles: number;
  originalFileFailures: number;
  archiveRowsSkipped: number;
}

export interface ImageImportResult {
  snapshot: AppSnapshot;
  sourceType: SourceType;
  totalFound: number;
  added: number;
  skippedExisting: number;
  skippedContent: number;
  changedExisting: number;
  metadataRejected: number;
  rejectedMoved: number;
  rejectedMoveFailures: number;
}

export type ImageImportStage = "extracting" | "scanning" | "hashing" | "processing" | "perceptualHashing";

export interface ImageImportProgress {
  stage: ImageImportStage;
  processed: number;
  total: number;
}

export interface ContentHashProgress {
  processed: number;
  total: number;
  updated: number;
  unreadable: number;
}

export type TagMatchMode = "and" | "or";
export type DedupeMode = "none" | "positivePrompt" | "artists";

export interface RowQuery {
  offset: number;
  limit: number;
  tags: string[];
  tagMode: TagMatchMode;
  dedupe: DedupeMode;
  singleArtistOnly: boolean;
  groupView: boolean;
  hideGrouped: boolean;
}

export interface RowRecord {
  id: number;
  batchId: number;
  sourceOrdinal: number;
  time: string | null;
  positivePrompt: string | null;
  negativePrompt: string | null;
  artists: string | null;
  imageFolder: string | null;
  imagePath: string | null;
  storedImagePath: string | null;
  metadataFailed: boolean;
  tags: string[];
  groupId: number | null;
  groupName: string | null;
}

export interface RowPage {
  rows: RowRecord[];
  totalCount: number;
  offset: number;
  limit: number;
  hasMore: boolean;
}

export interface TagSummary {
  name: string;
  rowCount: number;
}

export interface TagSelectionSummary {
  name: string;
  selectedRows: number;
}

export type RowSelection =
  | { kind: "explicit"; rowIds: number[] }
  | {
      kind: "filtered";
      tags: string[];
      tagMode: TagMatchMode;
      dedupe: DedupeMode;
      singleArtistOnly: boolean;
      excludedRowIds: number[];
    };

export interface TagMutationResult {
  affectedRows: number;
  normalizedTags: string[];
  associationsChanged: number;
}

/** 三种导出共用的进度事件（`export://progress`）。 */
export interface ExportProgress {
  processed: number;
  total: number;
}

export interface XlsxExportResult {
  path: string;
  rowCount: number;
  imagesEmbedded: number;
  imageFailures: number;
}

export interface JsonExportResult {
  path: string;
  exported: number;
}

export type ImageFileExportMode = "copy" | "hardlink";

export interface ImageFilesExportResult {
  directory: string;
  exported: number;
  hardlinkFallbacks: number;
  missing: number;
}

export interface JsonDedupePreviewItem {
  presetKey: string;
  fixedPrompt: string;
  negativePrompt: string;
}

export interface JsonDedupeInspection {
  originalCount: number;
  duplicateCount: number;
  uniqueCount: number;
  preview: JsonDedupePreviewItem[];
}

/** 智绘姬 JSON 去重进度事件（`json-dedupe://progress`）。 */
export interface JsonDedupeProgress {
  total: number;
  processed: number;
  duplicateCount: number;
}

export interface JsonDedupeSummary {
  originalCount: number;
  duplicateCount: number;
  uniqueCount: number;
  outputPath: string;
}

export interface MigrationResult {
  snapshot: AppSnapshot;
  retiredSource: string | null;
}

export interface PerceptualHashProgress {
  processed: number;
  total: number;
  updated: number;
  unreadable: number;
}

export interface SimilarImageMatch {
  rowId: number;
  distance: number;
}

export function getAppSnapshot(): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("get_app_snapshot");
}

export function resetConfiguration(): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("reset_configuration");
}

export function initializeDataDirectory(path: string): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("initialize_data_directory", { path });
}

export function openDataDirectory(path: string): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("open_data_directory", { path });
}

export function setRejectedImagesDirectory(path: string): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("set_rejected_images_directory", { path });
}

export function importImages(path: string): Promise<ImageImportResult> {
  return invoke<ImageImportResult>("import_images", { path });
}

export function deleteRows(selection: RowSelection, trashOriginals: boolean): Promise<DeleteResult> {
  return invoke<DeleteResult>("delete_rows", { selection, trashOriginals });
}

export function listImportBatches(): Promise<BatchSummary[]> {
  return invoke<BatchSummary[]>("list_import_batches");
}

export function queryRows(query: RowQuery): Promise<RowPage> {
  return invoke<RowPage>("query_rows", { query });
}

export function getRowsByIds(rowIds: number[]): Promise<RowRecord[]> {
  return invoke<RowRecord[]>("get_rows_by_ids", { rowIds });
}

export function listTags(): Promise<TagSummary[]> {
  return invoke<TagSummary[]>("list_tags");
}

export function createTag(name: string): Promise<boolean> {
  return invoke<boolean>("create_tag", { name });
}

export function deleteTag(name: string): Promise<boolean> {
  return invoke<boolean>("delete_tag", { name });
}

export function countSelectedRows(selection: RowSelection): Promise<number> {
  return invoke<number>("count_selected_rows", { selection });
}

export function listSelectionTags(selection: RowSelection): Promise<TagSelectionSummary[]> {
  return invoke<TagSelectionSummary[]>("list_selection_tags", { selection });
}

export function selectedRowIds(selection: RowSelection): Promise<number[]> {
  return invoke<number[]>("selected_row_ids", { selection });
}

export function addTagsToSelection(
  selection: RowSelection,
  tags: string[],
): Promise<TagMutationResult> {
  return invoke<TagMutationResult>("add_tags_to_selection", { selection, tags });
}

export function removeTagsFromSelection(
  selection: RowSelection,
  tags: string[],
): Promise<TagMutationResult> {
  return invoke<TagMutationResult>("remove_tags_from_selection", { selection, tags });
}

export function setTagsForRow(rowId: number, tags: string[]): Promise<TagMutationResult> {
  return invoke<TagMutationResult>("set_tags_for_row", { rowId, tags });
}

export function getRowThumbnail(rowId: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("get_row_thumbnail", { rowId });
}

export function getRowPreview(rowId: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("get_row_preview", { rowId });
}

export function exportRowImage(rowId: number, destination: string): Promise<void> {
  return invoke("export_row_image", { rowId, destination });
}

export function exportXlsx(
  selection: RowSelection,
  path: string,
): Promise<XlsxExportResult> {
  return invoke<XlsxExportResult>("export_xlsx", { selection, path });
}

export function exportZhihuijiJson(
  selection: RowSelection,
  path: string,
): Promise<JsonExportResult> {
  return invoke<JsonExportResult>("export_zhihuiji_json", { selection, path });
}

export function exportImageFiles(
  selection: RowSelection,
  parentDir: string,
  mode: ImageFileExportMode,
): Promise<ImageFilesExportResult> {
  return invoke<ImageFilesExportResult>("export_image_files", {
    selection,
    parentDir,
    mode,
  });
}

export function inspectZhihuijiJson(path: string): Promise<JsonDedupeInspection> {
  return invoke<JsonDedupeInspection>("inspect_zhihuiji_json", { path });
}

export function dedupeZhihuijiJson(
  inputPath: string,
  outputPath: string,
): Promise<JsonDedupeSummary> {
  return invoke<JsonDedupeSummary>("dedupe_zhihuiji_json", { inputPath, outputPath });
}

export function migrateDataDirectory(path: string): Promise<MigrationResult> {
  return invoke<MigrationResult>("migrate_data_directory", { path });
}

export function backfillPerceptualHashes(): Promise<PerceptualHashProgress> {
  return invoke<PerceptualHashProgress>("backfill_perceptual_hashes");
}

export function searchSimilarImages(
  path: string,
  threshold: number,
): Promise<SimilarImageMatch[]> {
  return invoke<SimilarImageMatch[]>("search_similar_images", { path, threshold });
}

// ── Groups ──────────────────────────────────────────────────────

export interface GroupSummary {
  id: number;
  name: string;
  memberCount: number;
  createdAt: string;
}

export function createGroup(name: string): Promise<GroupSummary> {
  return invoke<GroupSummary>("create_group", { name });
}

export function renameGroup(groupId: number, newName: string): Promise<GroupSummary> {
  return invoke<GroupSummary>("rename_group", { groupId, newName });
}

export function deleteGroup(groupId: number): Promise<boolean> {
  return invoke<boolean>("delete_group", { groupId });
}

export function deleteEmptyGroups(): Promise<number> {
  return invoke<number>("delete_empty_groups");
}

export function listGroups(): Promise<GroupSummary[]> {
  return invoke<GroupSummary[]>("list_groups");
}

export function assignRowsToGroup(
  selection: RowSelection,
  groupId: number,
): Promise<number> {
  return invoke<number>("assign_rows_to_group", { selection, groupId });
}

export function ungroupRows(selection: RowSelection): Promise<number> {
  return invoke<number>("ungroup_rows", { selection });
}

export function getGroupMembers(
  groupId: number,
  offset: number,
  limit: number,
): Promise<RowPage> {
  return invoke<RowPage>("get_group_members", { groupId, offset, limit });
}

// ── Prompt Editing ──────────────────────────────────────────────

export interface PromptEditResult {
  affectedRows: number;
}

export function updatePositivePrompt(
  rowId: number,
  newPrompt: string,
): Promise<PromptEditResult> {
  return invoke<PromptEditResult>("update_positive_prompt", { rowId, newPrompt });
}

export function findReplacePrompt(
  selection: RowSelection,
  find: string,
  replace: string,
): Promise<PromptEditResult> {
  return invoke<PromptEditResult>("find_replace_prompt", { selection, find, replace });
}

export function prependArtist(
  selection: RowSelection,
  artistName: string,
): Promise<PromptEditResult> {
  return invoke<PromptEditResult>("prepend_artist", { selection, artistName });
}
