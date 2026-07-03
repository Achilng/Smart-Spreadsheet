import { invoke } from "@tauri-apps/api/core";

import type { DedupeCluster } from "./types";

// ── Artists / Albums ────────────────────────────────────────────

/** 全库去重后的画师片段列表（按换行拆分、trim、去重、排序）。 */
export function listDistinctArtists(): Promise<string[]> {
  return invoke<string[]>("list_distinct_artists");
}

/** 用户自定义画师名单（settings 表持久化，原始多行文本）。 */
export function getCustomArtists(): Promise<string> {
  return invoke<string>("get_custom_artists");
}

export function setCustomArtists(text: string): Promise<void> {
  return invoke<void>("set_custom_artists", { text });
}

/** 画师串与给定值完全相同的所有行 ID（全库，忽略 Tag 筛选）。 */
export function rowIdsWithArtists(artists: string): Promise<number[]> {
  return invoke<number[]>("row_ids_with_artists", { artists });
}

/** 画册集：每个画师串成一册（含单张），复用 DedupeCluster 结构。 */
export function listArtistAlbums(): Promise<DedupeCluster[]> {
  return invoke<DedupeCluster[]>("list_artist_albums");
}
