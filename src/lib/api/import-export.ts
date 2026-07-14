import { invoke } from "@tauri-apps/api/core";

import type { RowSelection } from "./types";

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

export interface JsonExportNoteInspection {
  total: number;
  emptyNotes: number;
}

export type ImageFileExportMode = "copy" | "hardlink";

export interface ImageFilesExportResult {
  directory: string;
  exported: number;
  hardlinkFallbacks: number;
  missing: number;
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
  useNumericNamesForEmpty: boolean,
): Promise<JsonExportResult> {
  return invoke<JsonExportResult>("export_zhihuiji_json", {
    selection,
    path,
    useNumericNamesForEmpty,
  });
}

export function inspectZhihuijiExportNotes(
  selection: RowSelection,
): Promise<JsonExportNoteInspection> {
  return invoke<JsonExportNoteInspection>("inspect_zhihuiji_export_notes", { selection });
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

export function inspectZhihuijiJson(path: string): Promise<JsonDedupeInspection> {
  return invoke<JsonDedupeInspection>("inspect_zhihuiji_json", { path });
}

export function dedupeZhihuijiJson(
  inputPath: string,
  outputPath: string,
): Promise<JsonDedupeSummary> {
  return invoke<JsonDedupeSummary>("dedupe_zhihuiji_json", { inputPath, outputPath });
}
