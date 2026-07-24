import { invoke } from "@tauri-apps/api/core";

import type { AppSnapshot } from "./app";
import type { DedupeMode, RowSelection, SourceType, TagMatchMode } from "./types";
import type { RuleExecutionSummary } from "./automation-rules";

export interface ImageImportResult {
  snapshot: AppSnapshot;
  batchId: number;
  sourceType: SourceType;
  totalFound: number;
  added: number;
  skippedExisting: number;
  skippedContent: number;
  changedExisting: number;
  metadataRejected: number;
  rejectedMoved: number;
  rejectedMoveFailures: number;
  ruleExecution: RuleExecutionSummary;
}

export interface ExistingImageUpdateResult {
  snapshot: AppSnapshot;
  sourceType: SourceType;
  totalFound: number;
  matched: number;
  updated: number;
  matchedByIdentity: number;
  relinkedByContent: number;
  relinkedByMetadata: number;
  ambiguous: number;
  unmatched: number;
  metadataRejected: number;
  copyFailures: number;
  ruleExecution: RuleExecutionSummary;
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

export interface DeleteResult {
  snapshot: AppSnapshot;
  deletedRows: number;
  cleanupFailures: number;
  trashedOriginalFiles: number;
  originalFileFailures: number;
  archiveRowsSkipped: number;
}

export interface RowQuery {
  offset: number;
  limit: number;
  tags: string[];
  tagMode: TagMatchMode;
  dedupe: DedupeMode;
  singleArtistOnly: boolean;
  hasVibe: boolean;
  groupView: boolean;
  hideGrouped: boolean;
  search: string;
  sort: SortMode;
}

export type SortMode = "timeAsc" | "timeDesc" | "recentlyUpdated";

export interface RowRecord {
  id: number;
  batchId: number;
  sourceOrdinal: number;
  time: string | null;
  positivePrompt: string | null;
  characterPrompt: string | null;
  negativePrompt: string | null;
  note: string | null;
  artists: string | null;
  imageFolder: string | null;
  imagePath: string | null;
  storedImagePath: string | null;
  metadataFailed: boolean;
  vibeReferenceCount: number | null;
  tags: string[];
  groupId: number | null;
  groupName: string | null;
}

export interface MutableRowState {
  rowId: number;
  positivePrompt: string | null;
  characterPrompt: string | null;
  negativePrompt: string | null;
  note: string | null;
  artists: string | null;
  tags: string[];
  groupId: number | null;
}

export function mutableRowState(row: RowRecord): MutableRowState {
  return {
    rowId: row.id,
    positivePrompt: row.positivePrompt,
    characterPrompt: row.characterPrompt,
    negativePrompt: row.negativePrompt,
    note: row.note,
    artists: row.artists,
    tags: [...row.tags],
    groupId: row.groupId,
  };
}

export interface RowPage {
  rows: RowRecord[];
  totalCount: number;
  offset: number;
  limit: number;
  hasMore: boolean;
}

export function importImages(path: string): Promise<ImageImportResult> {
  return invoke<ImageImportResult>("import_images", { path });
}

export function updateExistingImages(path: string): Promise<ExistingImageUpdateResult> {
  return invoke<ExistingImageUpdateResult>("update_existing_images", { path });
}

export function deleteRows(selection: RowSelection, trashOriginals: boolean): Promise<DeleteResult> {
  return invoke<DeleteResult>("delete_rows", { selection, trashOriginals });
}

export function undoImportBatch(batchId: number): Promise<DeleteResult> {
  return invoke<DeleteResult>("undo_import_batch", { batchId });
}

export function restoreMutableRowStates(states: MutableRowState[]): Promise<number> {
  return invoke<number>("restore_mutable_row_states", { states });
}

export function queryRows(query: RowQuery): Promise<RowPage> {
  const { sort, ...filters } = query;
  return invoke<RowPage>("query_rows", { query: filters, sort });
}

export function getRowsByIds(rowIds: number[]): Promise<RowRecord[]> {
  return invoke<RowRecord[]>("get_rows_by_ids", { rowIds });
}

export function getRowIndex(rowId: number, sort: SortMode = "timeAsc"): Promise<number> {
  return invoke<number>("get_row_index", { rowId, sort });
}

export function countSelectedRows(selection: RowSelection): Promise<number> {
  return invoke<number>("count_selected_rows", { selection });
}

export function selectedRowIds(selection: RowSelection): Promise<number[]> {
  return invoke<number[]>("selected_row_ids", { selection });
}

export function getRowThumbnail(rowId: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("get_row_thumbnail", { rowId });
}

export function getRowGalleryPreview(rowId: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("get_row_gallery_preview", { rowId });
}

export function getRowPreview(rowId: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("get_row_preview", { rowId });
}

export function getRowOriginal(rowId: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("get_row_original", { rowId });
}

export function exportRowImage(rowId: number, destination: string): Promise<void> {
  return invoke("export_row_image", { rowId, destination });
}

export interface FileDragInfo {
  filePath: string;
  iconPath: string;
}

export function prepareFileDrag(rowId: number): Promise<FileDragInfo> {
  return invoke<FileDragInfo>("prepare_file_drag", { rowId });
}

/** 行图片 Comment 元数据里的 vibe 引用数；文件缺失或解析失败时为 null。 */
export function getRowVibeStatus(rowId: number): Promise<number | null> {
  return invoke<number | null>("get_row_vibe_status", { rowId });
}

export function showItemInExplorer(rowId: number): Promise<void> {
  return invoke<void>("show_item_in_explorer", { rowId });
}
