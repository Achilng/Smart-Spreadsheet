import { invoke } from "@tauri-apps/api/core";

import type { AppSnapshot } from "./app";
import type { DedupeMode, RowSelection, SourceType, TagMatchMode } from "./types";

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
  groupView: boolean;
  hideGrouped: boolean;
  search: string;
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

export function importImages(path: string): Promise<ImageImportResult> {
  return invoke<ImageImportResult>("import_images", { path });
}

export function deleteRows(selection: RowSelection, trashOriginals: boolean): Promise<DeleteResult> {
  return invoke<DeleteResult>("delete_rows", { selection, trashOriginals });
}

export function queryRows(query: RowQuery): Promise<RowPage> {
  return invoke<RowPage>("query_rows", { query });
}

export function getRowsByIds(rowIds: number[]): Promise<RowRecord[]> {
  return invoke<RowRecord[]>("get_rows_by_ids", { rowIds });
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

export function getRowPreview(rowId: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("get_row_preview", { rowId });
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

export function showItemInExplorer(rowId: number): Promise<void> {
  return invoke<void>("show_item_in_explorer", { rowId });
}
