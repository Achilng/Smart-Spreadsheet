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

export function getArtistDictionaryStatus(): Promise<ArtistDictionaryStatus | null> {
  return invoke<ArtistDictionaryStatus | null>("get_artist_dictionary_status");
}

export function syncArtistDictionary(): Promise<ArtistDictionaryStatus> {
  return invoke<ArtistDictionaryStatus>("sync_artist_dictionary");
}
