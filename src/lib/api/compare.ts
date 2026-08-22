import { invoke } from "@tauri-apps/api/core";

import type { RowPage, RowRecord } from "./rows";

/** 对比页分区类型，与后端 `CompareSectionKind` 一一对应。 */
export type CompareSectionKind =
  | "sameArtists"
  | "artistsByModel"
  | "sameVibePromptDiff"
  | "samePromptVibeDiff"
  | "samePromptModelDiff";

/** 模型分组分区的组：`model` 为空串表示库内未知模型。 */
export interface CompareModelGroup {
  model: string;
  rowCount: number;
}

export interface CompareSectionSummary {
  kind: CompareSectionKind;
  totalCount: number;
  modelGroups: CompareModelGroup[];
}

export interface CompareSample {
  row: RowRecord;
  sections: CompareSectionSummary[];
}

export function compareSample(rowId: number): Promise<CompareSample> {
  return invoke<CompareSample>("compare_sample", { rowId });
}

export function compareSectionRows(
  rowId: number,
  kind: CompareSectionKind,
  model: string | null,
  offset: number,
  limit: number,
): Promise<RowPage> {
  return invoke<RowPage>("compare_section_rows", {
    rowId,
    kind,
    model,
    offset,
    limit,
  });
}
