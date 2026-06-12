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
  library: LibrarySummary | null;
  startupError: string | null;
}

export interface ImportResult {
  snapshot: AppSnapshot;
  added: number;
  skippedExisting: number;
  skippedContent: number;
  changedExisting: number;
  embeddedImagesStored: number;
}

export interface DeleteResult {
  snapshot: AppSnapshot;
  deletedRows: number;
  cleanupFailures: number;
}

export interface ImageImportResult {
  snapshot: AppSnapshot;
  sourceType: SourceType;
  totalFound: number;
  added: number;
  skippedExisting: number;
  skippedContent: number;
  changedExisting: number;
  metadataFailed: number;
}

export type ImageImportStage = "extracting" | "scanning" | "hashing" | "processing";

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

export type DuplicateKeyKind = "positivePrompt" | "artists";

export interface DuplicateRow {
  id: number;
  batchId: number;
  sourceOrdinal: number;
  time: string | null;
  imagePath: string | null;
  storedImagePath: string | null;
  tags: string[];
}

export interface DuplicateGroup {
  key: string;
  rows: DuplicateRow[];
}

export interface DuplicateReport {
  totalGroups: number;
  totalRedundantRows: number;
  groups: DuplicateGroup[];
}

export type TagMatchMode = "and" | "or";
export type DedupeMode = "none" | "positivePrompt" | "artists";

export interface RowQuery {
  offset: number;
  limit: number;
  tags: string[];
  tagMode: TagMatchMode;
  dedupe: DedupeMode;
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

export type RowSelection =
  | { kind: "explicit"; rowIds: number[] }
  | {
      kind: "filtered";
      tags: string[];
      tagMode: TagMatchMode;
      dedupe: DedupeMode;
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

export function getAppSnapshot(): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("get_app_snapshot");
}

export function initializeDataDirectory(path: string): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("initialize_data_directory", { path });
}

export function openDataDirectory(path: string): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("open_data_directory", { path });
}

export function importWorkbook(path: string): Promise<ImportResult> {
  return invoke<ImportResult>("import_workbook", { path });
}

export function importImages(path: string): Promise<ImageImportResult> {
  return invoke<ImageImportResult>("import_images", { path });
}

export function deleteRows(selection: RowSelection): Promise<DeleteResult> {
  return invoke<DeleteResult>("delete_rows", { selection });
}

export function findDuplicates(
  key: DuplicateKeyKind,
  groupLimit: number,
): Promise<DuplicateReport> {
  return invoke<DuplicateReport>("find_duplicates", { key, groupLimit });
}

export function listImportBatches(): Promise<BatchSummary[]> {
  return invoke<BatchSummary[]>("list_import_batches");
}

export function queryRows(query: RowQuery): Promise<RowPage> {
  return invoke<RowPage>("query_rows", { query });
}

export function listTags(): Promise<TagSummary[]> {
  return invoke<TagSummary[]>("list_tags");
}

export function createTag(name: string): Promise<boolean> {
  return invoke<boolean>("create_tag", { name });
}

export function countSelectedRows(selection: RowSelection): Promise<number> {
  return invoke<number>("count_selected_rows", { selection });
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
