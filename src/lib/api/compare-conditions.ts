import { invoke } from "@tauri-apps/api/core";
import type { LibraryFilter } from "./types";
import type { CompareSample, CompareSectionPage } from "./compare";
import { modelComparisonTier, modelVersionBadge } from "../utils/model-version";

export const compareFields = {
  artists: "画师串", positive: "正向提示词", character: "角色提示词", negative: "负向提示词",
  style: "正向提示词（忽略官方质量词）", vibe: "VIBE 引用组合", model: "模型", sampler: "采样器",
  steps: "步数", seed: "种子", scale: "提示词引导", cfgRescale: "引导重缩放", noiseSchedule: "噪声调度", dimensions: "图片尺寸",
} as const;
export const diffTabs = { positive: "正向提示词", character: "角色提示词", negative: "负向提示词", params: "生成参数" } as const;
export type DiffTab = keyof typeof diffTabs;
export type CompareField = keyof typeof compareFields;
export interface CompareRule {
  field: CompareField;
  relation: "same" | "different" | "onlyWords";
  enabled: boolean;
  words: string;
}
export interface CompareOptions { rules: CompareRule[]; filters: LibraryFilter[]; initialTab: DiffTab }
export const newRule = (field: CompareField, relation: CompareRule["relation"] = "same"): CompareRule => ({ field, relation, enabled: true, words: "" });

export function startingConditions(preset: "artists" | "vibeDiffStyle" | "styleDiffVibe" | "models" | "blank"): CompareOptions {
  const rules = {
    artists: [newRule("artists")], vibeDiffStyle: [newRule("vibe"), newRule("style", "different")],
    styleDiffVibe: [newRule("style"), newRule("vibe", "different")], models: [newRule("style"), newRule("model", "different")], blank: [],
  };
  return { rules: rules[preset], filters: [], initialTab: preset === "models" ? "params" : "positive" };
}

export function sampleValue(sample: CompareSample, field: CompareField): string {
  const row = sample.row;
  const badge = modelVersionBadge(row.generationModel);
  const emptyPrompt = row.metadataFailed ? "未记录" : "空（0 项）";
  const values = {
    artists: row.artists, positive: row.positivePrompt || emptyPrompt, style: row.positivePrompt || emptyPrompt, character: row.characterPrompt || emptyPrompt, negative: row.negativePrompt || emptyPrompt,
    vibe: sample.vibeSignatureUnreadable ? "引用不可读" : sample.hasVibeSignature ? `已引用 VIBE${row.vibeReferenceCount ? ` × ${row.vibeReferenceCount}` : ""}` : "无 VIBE",
    model: badge ? `${badge.label} · ${row.generationModel}` : row.generationModel, sampler: row.generationSampler,
    steps: row.generationSteps, seed: row.generationSeed, scale: row.generationScale, cfgRescale: row.generationCfgRescale,
    noiseSchedule: row.generationNoiseSchedule, dimensions: row.imageWidth && row.imageHeight ? `${row.imageWidth} × ${row.imageHeight}` : null,
  };
  return String(values[field] ?? "未记录");
}

export function queryCompareCustom(sample: CompareSample, options: Pick<CompareOptions, "rules" | "filters">, offset: number, limit: number): Promise<CompareSectionPage> {
  const tier = modelComparisonTier(sample.row.generationModel);
  const names = sample.modelNames ?? (sample.row.generationModel ? [sample.row.generationModel] : []);
  const conditions = {
    rules: options.rules.filter(rule => rule.enabled).map(({ field, relation, words }) => ({ field, relation, words })),
    filters: options.filters, equivalentModels: names.filter(name => modelComparisonTier(name) === tier),
  };
  return invoke("query_compare_custom", { rowId: sample.row.id, conditions, offset, limit });
}
