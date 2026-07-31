import { invoke } from "@tauri-apps/api/core";

import type { SourceType } from "./types";

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
  autoArtistPrefixOnImport: boolean;
  startupError: string | null;
}

export interface MigrationResult {
  snapshot: AppSnapshot;
  retiredSource: string | null;
}

export type MigrationStage =
  | "preparing"
  | "copyingFiles"
  | "backingUpDatabase"
  | "verifyingFiles"
  | "verifyingDatabase"
  | "switching";

/** `migration://progress` 事件载荷。completed/total 用于整体进度条。 */
export interface MigrationProgress {
  stage: MigrationStage;
  completed: number;
  total: number;
  stageCompleted: number;
  stageTotal: number;
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

export function setAutoArtistPrefixOnImport(enabled: boolean): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("set_auto_artist_prefix_on_import", { enabled });
}

export function resetData(): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("reset_data");
}

export function listImportBatches(): Promise<BatchSummary[]> {
  return invoke<BatchSummary[]>("list_import_batches");
}

export function migrateDataDirectory(path: string): Promise<MigrationResult> {
  return invoke<MigrationResult>("migrate_data_directory", { path });
}

export function openRejectedImagesDirectory(): Promise<void> {
  return invoke<void>("open_rejected_images_directory");
}
