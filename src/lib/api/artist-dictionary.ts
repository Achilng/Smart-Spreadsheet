import { invoke } from "@tauri-apps/api/core";

export type ArtistDictionarySyncStage = "tags" | "artists" | "aliases" | "saving";

export interface ArtistDictionarySyncProgress {
  stage: ArtistDictionarySyncStage;
  pagesFetched: number;
  itemsFetched: number;
}

export interface ArtistDictionaryStatus {
  syncedAt: string;
  tagCount: number;
  artistCount: number;
  aliasCount: number;
  nameCount: number;
}

export interface AutoArtistCandidate {
  matchName: string;
  displayName: string;
  canonicalName: string;
  postCount: number;
  matchedRows: number;
  matchedFields: number;
  sampleRowIds: number[];
  isBanned: boolean;
  isDeprecated: boolean;
  isAmbiguous: boolean;
  isLowUsage: boolean;
  isShortName: boolean;
  isCommonWord: boolean;
  needsConfirmation: boolean;
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

export function getArtistDictionaryStatus(): Promise<ArtistDictionaryStatus | null> {
  return invoke<ArtistDictionaryStatus | null>("get_artist_dictionary_status");
}

export function syncArtistDictionary(): Promise<ArtistDictionaryStatus> {
  return invoke<ArtistDictionaryStatus>("sync_artist_dictionary");
}

export function previewAutoArtistPrefix(): Promise<AutoArtistPrefixPreview> {
  return invoke<AutoArtistPrefixPreview>("preview_auto_artist_prefix");
}

export function applyAutoArtistPrefix(
  selectedNames: string[],
): Promise<AutoArtistPrefixApplyResult> {
  return invoke<AutoArtistPrefixApplyResult>("apply_auto_artist_prefix", { selectedNames });
}
