import { invoke } from "@tauri-apps/api/core";

import type { RowRecord } from "./rows";

/** 对比样本：行摘要 + 决定各分区空态文案的签名状态。 */
export interface CompareSample {
  row: RowRecord;
  hasStyleSignature: boolean;
  hasVibeSignature: boolean;
  vibeSignatureUnreadable: boolean;
}

/** 分区①②③的分页结果。 */
export interface CompareSectionPage {
  rows: RowRecord[];
  totalCount: number;
  offset: number;
  limit: number;
}

/** 分区④：同画风全部行（上限 500 张）+ 截断标记。 */
export interface CompareModelSection {
  rows: RowRecord[];
  totalCount: number;
  truncated: boolean;
}

export function getCompareSample(rowId: number): Promise<CompareSample> {
  return invoke<CompareSample>("get_compare_sample", { rowId });
}

export function queryCompareSameArtists(
  rowId: number,
  offset: number,
  limit: number,
): Promise<CompareSectionPage> {
  return invoke<CompareSectionPage>("query_compare_same_artists", {
    rowId,
    offset,
    limit,
  });
}

export function queryCompareSameVibeDiffStyle(
  rowId: number,
  offset: number,
  limit: number,
): Promise<CompareSectionPage> {
  return invoke<CompareSectionPage>("query_compare_same_vibe_diff_style", {
    rowId,
    offset,
    limit,
  });
}

export function queryCompareSameStyleDiffVibe(
  rowId: number,
  offset: number,
  limit: number,
): Promise<CompareSectionPage> {
  return invoke<CompareSectionPage>("query_compare_same_style_diff_vibe", {
    rowId,
    offset,
    limit,
  });
}

export function queryCompareSameStyleAllModels(
  rowId: number,
): Promise<CompareModelSection> {
  return invoke<CompareModelSection>("query_compare_same_style_all_models", {
    rowId,
  });
}
