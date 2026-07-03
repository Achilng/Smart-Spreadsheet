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

export function searchSimilarImages(
  path: string,
  threshold: number,
): Promise<SimilarImageMatch[]> {
  return invoke<SimilarImageMatch[]>("search_similar_images", { path, threshold });
}
