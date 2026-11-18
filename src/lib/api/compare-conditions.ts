import { invoke } from "@tauri-apps/api/core";
import { getRowsByIds, type RowRecord } from "./rows";
import type { CompareSample, CompareSectionPage } from "./compare";
import { modelComparisonTier, modelVersionBadge } from "../utils/model-version";
import { diffPromptField } from "../utils/prompt-diff";

export const compareFields = {
  positive: "正向提示词", negative: "负向提示词", character: "角色提示词", artists: "画师串",
  model: "模型", vibe: "VIBE 引用组合", sampler: "采样器", steps: "步数", seed: "种子",
  scale: "提示词引导", cfgRescale: "引导重缩放", noiseSchedule: "噪声调度", dimensions: "图片尺寸",
} as const;
export type CompareField = keyof typeof compareFields;
export type DiffTab = "positive" | "negative" | "character" | "params";
export interface CompareOptions { observe: CompareField; dedupe: boolean; allowed: CompareField[] }
export type CompareGroups = number[][];
export interface CustomPage extends CompareSectionPage { groups: CompareGroups }

export function startingConditions(preset: "artists" | "vibeDiffStyle" | "styleDiffVibe" | "models" | "blank"): CompareOptions {
  const observe = { artists: "artists", vibeDiffStyle: "positive", styleDiffVibe: "vibe", models: "model", blank: "negative" } as const;
  return { observe: observe[preset], dedupe: false, allowed: Object.keys(compareFields) as CompareField[] };
}

export function inspectionTab(field: CompareField): DiffTab {
  if (field === "artists") return "positive";
  return field === "positive" || field === "character" || field === "negative" ? field : "params";
}

export async function queryCompareCustom(sample: CompareSample, options: CompareOptions): Promise<CompareGroups> {
  const names = sample.modelNames ?? (sample.row.generationModel ? [sample.row.generationModel] : []);
  const modelKeys = Object.fromEntries(names.map(name => [name, modelComparisonTier(name)]));
  return invoke("query_compare_custom", { rowId: sample.row.id, conditions: { ...options, modelKeys } });
}

export async function orderedRows(ids: number[]) {
  if (!ids.length) return [];
  const rows = await getRowsByIds(ids);
  const byId = new Map(rows.map(row => [row.id, row]));
  return ids.flatMap(id => byId.get(id) ?? []);
}

export async function customPage(groups: CompareGroups, offset: number, limit: number): Promise<CustomPage> {
  const rows = await orderedRows(groups.slice(offset, offset + limit).map(group => group[0]));
  return { groups, rows, offset, limit, totalCount: groups.length };
}

export function fieldValue(row: RowRecord, field: CompareField): string {
  const values = {
    positive: row.positivePrompt, negative: row.negativePrompt, character: row.characterPrompt, artists: row.artists,
    model: modelVersionBadge(row.generationModel)?.label ?? row.generationModel, sampler: row.generationSampler,
    steps: row.generationSteps, seed: row.generationSeed, scale: row.generationScale, cfgRescale: row.generationCfgRescale,
    noiseSchedule: row.generationNoiseSchedule, dimensions: row.imageWidth && row.imageHeight ? `${row.imageWidth} × ${row.imageHeight}` : null,
    vibe: row.vibeReferenceCount == null ? "引用数量未知" : row.vibeReferenceCount ? `${row.vibeReferenceCount} 个引用` : "无引用",
  };
  return String(values[field] ?? "");
}

export function describeChange(a: RowRecord, b: RowRecord, field: CompareField) {
  const before = fieldValue(a, field), after = fieldValue(b, field);
  if (["positive", "negative", "character", "artists"].includes(field)) {
    const diff = diffPromptField(before, after);
    const added = diff.onlyRight.map(token => `新增 ${token.display}`);
    const removed = diff.onlyLeft.map(token => `移除 ${token.display}`);
    const changes = [...added, ...removed];
    if (changes.length) {
      const lines = added.length && removed.length ? [added[0], removed[0]] : changes.slice(0, 2);
      return { lines, more: changes.length - lines.length, full: changes.join("\n") };
    }
  }
  let line = `${before || "空"} → ${after || "空"}`;
  if (field === "model" && before === after) line = `${a.generationModel} → ${b.generationModel}`;
  if (field === "vibe" && before === after) line = `${after}，引用组合不同`;
  const summary = `${compareFields[field]} ${line}`;
  return { lines: [summary], more: 0, full: summary };
}
