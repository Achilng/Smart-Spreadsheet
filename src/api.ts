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
  changedExisting: number;
  embeddedImagesStored: number;
}

export interface DeleteResult {
  snapshot: AppSnapshot;
  deletedRows: number;
  cleanupFailures: number;
}

export type TagMatchMode = "and" | "or";

export interface RowQuery {
  offset: number;
  limit: number;
  tags: string[];
  tagMode: TagMatchMode;
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
      excludedRowIds: number[];
    };

export interface TagMutationResult {
  affectedRows: number;
  normalizedTags: string[];
  associationsChanged: number;
}

export interface ExportResult {
  path: string;
  rowCount: number;
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

export function deleteRows(selection: RowSelection): Promise<DeleteResult> {
  return invoke<DeleteResult>("delete_rows", { selection });
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

export function exportWorkbook(path: string): Promise<ExportResult> {
  return invoke<ExportResult>("export_workbook", { path });
}

export function migrateDataDirectory(path: string): Promise<MigrationResult> {
  return invoke<MigrationResult>("migrate_data_directory", { path });
}
