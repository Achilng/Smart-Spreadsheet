import { invoke } from "@tauri-apps/api/core";

export interface AutoArtistCandidate {
  matchName: string;
  displayName: string;
  matchedRows: number;
  matchedFields: number;
  sampleRowIds: number[];
}

export interface AutoArtistPrefixPreview {
  scannedRows: number;
  matchedRows: number;
  promptFieldsNeedingChanges: number;
  candidates: AutoArtistCandidate[];
}

export interface AutoArtistPrefixApplyResult {
  scannedRows: number;
  matchedRows: number;
  changedRows: number;
  promptFieldsChanged: number;
  changes: import("./quick-edit").QuickArtistPrefixChange[];
}

export function previewAutoArtistPrefix(): Promise<AutoArtistPrefixPreview> {
  return invoke<AutoArtistPrefixPreview>("preview_auto_artist_prefix");
}

export function applyAutoArtistPrefix(
  selectedNames: string[],
): Promise<AutoArtistPrefixApplyResult> {
  return invoke<AutoArtistPrefixApplyResult>("apply_auto_artist_prefix", { selectedNames });
}
