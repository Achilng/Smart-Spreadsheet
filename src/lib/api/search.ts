import { invoke } from "@tauri-apps/api/core";

export interface PerceptualHashProgress {
  processed: number;
  total: number;
  updated: number;
  unreadable: number;
}

export interface SimilarImageMatch {
  rowId: number;
  distance: number;
}

export function backfillPerceptualHashes(): Promise<PerceptualHashProgress> {
  return invoke<PerceptualHashProgress>("backfill_perceptual_hashes");
}

export interface VibeStatusProgress {
  processed: number;
  total: number;
  unreadable: number;
}

/** 升级后首启补齐历史图片的 VIBE 数量与组合签名；无待补行时立即返回 total = 0。 */
export function backfillVibeStatuses(): Promise<VibeStatusProgress> {
  return invoke<VibeStatusProgress>("backfill_vibe_statuses");
}

export function searchSimilarImages(
  path: string,
  threshold: number,
): Promise<SimilarImageMatch[]> {
  return invoke<SimilarImageMatch[]>("search_similar_images", { path, threshold });
}
