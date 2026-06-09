import { invoke } from "@tauri-apps/api/core";

export interface WorkbookSummary {
  importedName: string;
  importedAt: string;
  sheetName: string;
  rowCount: number;
}

export interface AppSnapshot {
  dataDirectory: string | null;
  workbook: WorkbookSummary | null;
  startupError: string | null;
}

export interface ImportResult {
  snapshot: AppSnapshot;
  importedRows: number;
  embeddedImages: number;
  previousCopyCleanup: string | null;
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
