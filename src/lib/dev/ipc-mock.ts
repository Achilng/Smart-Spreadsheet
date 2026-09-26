/**
 * 开发专用的 Tauri IPC 模拟（`?mock=1` 查询参数激活，仅 DEV 构建引用）。
 * 用于 Playwright/浏览器冒烟：不启动 Tauri 也能渲染对比窗口并核对
 * 分区、空态、分页与 set-sample 事件切换。生产构建会把它摇树剔除。
 */

import type { FilterNumericComparison, LibraryFilter, MutableRowState, RowQuery, RowRecord, RowSelection } from "../api";
import type { Material, MaterialDraft, MaterialInspection } from "../api/materials";
import type { PromptDocDetail } from "../api/prompt-docs";
import type { GroupSummary } from "../api/groups";
import { emptyAutomationRuleDraft, type AutomationRule, type AutomationRuleDraft, type AutomationRuleImportInspection, type RuleCondition } from "../api/automation-rules";
import { splitListText } from "../utils/list-text";

interface MockRow {
  id: number;
  artists?: string;
  positivePrompt?: string | null;
  characterPrompt?: string | null;
  generationModel?: string | null;
  imagePath?: string;
  imageWidth?: number;
  imageHeight?: number;
  vibeReferenceCount?: number;
  time?: string;
  tags?: string[];
}

function rowDto(row: MockRow): RowRecord {
  return {
    id: row.id,
    batchId: 1,
    sourceOrdinal: row.id,
    favorite: false,
    time: row.time ?? "2026-08-01 12:00",
    positivePrompt: row.positivePrompt ?? null,
    characterPrompt: row.characterPrompt ?? null,
    negativePrompt: "lowres, worst quality",
    note: null,
    artists: row.artists ?? null,
    imageFolder: null,
    imagePath: row.imagePath ?? `D:\\mock\\image${row.id}.png`,
    storedImagePath: null,
    imageWidth: row.imageWidth ?? 832,
    imageHeight: row.imageHeight ?? 1216,
    generationModel: row.generationModel ?? null,
    generationSampler: "k_euler_ancestral",
    generationSteps: 28,
    generationSeed: String(1000 + row.id),
    generationScale: "5",
    generationCfgRescale: "0.18",
    generationNoiseSchedule: "karras",
    metadataFailed: false,
    vibeReferenceCount: row.vibeReferenceCount ?? 0,
    groupId: null,
    groupName: null,
    tags: row.tags ?? [],
  };
}

const ARTIST_ROWS: MockRow[] = Array.from({ length: 30 }, (_, index) => ({
  id: 101 + index,
  artists: "artist:alpha",
  positivePrompt: `artist:alpha, hair style ${index}`,
}));

const VIBE_ROWS: MockRow[] = [
  {
    id: 201,
    positivePrompt: "artist:beta, night city",
    characterPrompt: "1girl, silver hair\ngreen eyes",
    vibeReferenceCount: 3,
  },
  { id: 202, positivePrompt: "artist:gamma, sunset beach", vibeReferenceCount: 3 },
];

const STYLE_ROWS: MockRow[] = [
  { id: 301, positivePrompt: "artist:delta, blue hair, school uniform" },
  { id: 302, positivePrompt: "artist:alpha, blue hair, school uniform" },
];

const MODEL_ROWS: MockRow[] = [
  { id: 401, generationModel: "NovelAI Diffusion V4 Full", positivePrompt: "same prompt" },
  { id: 402, generationModel: "NovelAI Diffusion V4.5 Curated", positivePrompt: "same prompt" },
  { id: 403, generationModel: "NovelAI Diffusion V3", positivePrompt: "same prompt" },
  { id: 404, generationModel: null, positivePrompt: "same prompt" },
  // 与样本同档位：后端会返回，但模型分区必须过滤且不得计入标题数量。
  { id: 405, generationModel: "NovelAI Diffusion V4.5 Full", positivePrompt: "same prompt" },
];

const SAME_MODEL_ROWS: MockRow[] = Array.from({ length: 7 }, (_, index) => ({
  id: 501 + index,
  generationModel: "NovelAI Diffusion V4.5 Full",
  positivePrompt: "same prompt",
}));

type SectionRow = MockRow[];

const SECTIONS: Record<string, SectionRow> = {
  sameArtists: ARTIST_ROWS,
  vibeDiffStyle: VIBE_ROWS,
  styleDiffVibe: STYLE_ROWS,
};

const PAGE_SIZE_DEFAULT = 24;

function sectionPage(rows: SectionRow, offset: number, limit: number) {
  const page = rows.slice(offset, offset + limit);
  return {
    rows: page.map(rowDto),
    totalCount: rows.length,
    offset,
    limit,
  };
}

/** 1×1 透明 PNG，让缩略图/大图管线在浏览器里有真实字节可解码。 */
function tinyPng(): ArrayBuffer {
  const bytes = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
    0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00,
    0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
  ];
  return new Uint8Array(bytes).buffer;
}

/** Browser-only generated scenery, so visual QA can inspect actual image framing. */
async function previewPng(rowId: number): Promise<ArrayBuffer> {
  const canvas = document.createElement("canvas");
  canvas.width = 320;
  canvas.height = 400;
  const context = canvas.getContext("2d");
  if (!context) return tinyPng();
  const palettes = [["#dedfcf", "#81969e", "#4d6d78"], ["#f5dcbf", "#c19487", "#816f80"], ["#d6e8df", "#83a5a1", "#496f78"]];
  const colors = palettes[rowId % palettes.length];
  const gradient = context.createLinearGradient(0, 0, 0, 400);
  gradient.addColorStop(0, colors[0]); gradient.addColorStop(1, colors[1]);
  context.fillStyle = gradient; context.fillRect(0, 0, 320, 400);
  context.fillStyle = "#fff9e5"; context.beginPath(); context.arc(220, 93, 30, 0, Math.PI * 2); context.fill();
  for (let layer = 0; layer < 3; layer++) {
    context.fillStyle = colors[2]; context.globalAlpha = .2 + layer * .2;
    context.beginPath(); context.moveTo(0, 215 + layer * 50);
    context.bezierCurveTo(80, 110 + layer * 60, 160, 320 + layer * 10, 320, 180 + layer * 50);
    context.lineTo(320, 400); context.lineTo(0, 400); context.closePath(); context.fill();
  }
  context.globalAlpha = .7; context.fillStyle = "white"; context.font = "11px sans-serif"; context.fillText("PREVIEW  /  " + rowId, 20, 375);
  const blob = await new Promise<Blob | null>(resolve => canvas.toBlob(resolve, "image/png"));
  return blob ? blob.arrayBuffer() : tinyPng();
}

const eventListeners = new Map<number, { event: string; handler: (payload: unknown) => void }>();
let callbackCounter = 0;

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    /** 冒烟测试辅助：模拟后端向本窗口推送事件。 */
    __mockEmit?: (event: string, payload: unknown) => void;
    __mockCalls?: { command: string; payload: Record<string, unknown> }[];
    __mockDelayMs?: number;
    __mockFailNext?: string;
  }
}

export function installIpcMock(): void {
  eventListeners.clear();
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_event: string, id: number) => { eventListeners.delete(id); } };
  const params = new URLSearchParams(location.search);
  const windowLabel = params.get("window") ?? "main";
  const libraryRows = Array.from({ length: params.has("large") ? 60_000 : 64 }, (_, index) => rowDto({
    id: index + 1,
    imagePath: `D:\\mock\\${index % 7 === 0 ? "清晨的山谷与远处的群山_长文件名显示检查_" : "风景_"}${String(index + 1).padStart(3, "0")}.png`,
    artists: index % 2 === 0 ? "artist:alpha" : "artist:beta",
    positivePrompt: `masterpiece, scenery, mountains, soft light, ${index % 2 ? "sunset" : "morning"}, artist:${index % 2 ? "beta" : "alpha"}`,
    generationModel: "NovelAI Diffusion V4.5 Full",
    imageWidth: index % 3 === 0 ? 1216 : 832,
    imageHeight: index % 3 === 0 ? 832 : 1216,
    tags: index % 4 === 0 ? ["收藏", "风景", "柔和光线"] : index % 3 === 0 ? [] : ["风景"],
    vibeReferenceCount: index % 4 === 0 ? 2 : 0,
  }));
  const snapshot = {
    dataDirectory: params.has("setup") ? null : "D:\\mock",
    rejectedImagesDirectory: "D:\\mock\\rejected",
    library: { rowCount: libraryRows.length, batchCount: 1, lastBatch: null },
    autoArtistPrefixOnImport: false,
    startupError: null,
  };
  const groups: GroupSummary[] = [
    { id: 1, name: "清晨风景", memberCount: 0, createdAt: "2026-09-20T08:00:00Z" },
    { id: 2, name: "日落参考", memberCount: 0, createdAt: "2026-09-20T09:00:00Z" },
    { id: 3, name: "待整理（空分组）", memberCount: 0, createdAt: "2026-09-20T10:00:00Z" },
  ];
  for (const row of libraryRows.slice(0, 20)) { const group = groups[row.id <= 12 ? 0 : 1]; row.groupId = group.id; row.groupName = group.name; }
  let nextGroupId = 4;
  const aliases = new Map<string, string>();
  const now = () => new Date().toISOString();
  const docs: PromptDocDetail[] = ["风景提示词笔记", "角色与服装参考", "空白文档"].map((title, index) => ({
    id: `mock-doc-${index + 1}`, title, createdAt: "2026-09-20T08:00:00Z", updatedAt: `2026-09-20T0${9 - index}:00:00Z`,
    plainText: index === 2 ? "" : index === 0 ? "清晨山谷\nmasterpiece, scenery, mountains, soft light" : "角色参考\n1girl, silver hair, green eyes",
    content: { type: "doc", content: index === 2 ? [{ type: "paragraph" }] : [
      { type: "heading", attrs: { level: 2 }, content: [{ type: "text", text: index === 0 ? "清晨山谷" : "角色参考" }] },
      { type: "paragraph", content: [{ type: "text", text: index === 0 ? "masterpiece, scenery, mountains, soft light" : "1girl, silver hair, green eyes" }] },
    ] }, assets: [],
  }));
  let nextDocId = 4;
  const assetUrls = new Map<string, string>();
  let nextAssetId = 1;
  const materials: Material[] = Array.from({ length: 52 }, (_, index) => ({
    id: index + 1, title: `${index % 2 ? "角色服装" : "风景光影"}参考 ${index + 1}`, text: index % 2 ? "1girl, silver hair, blue dress" : "scenery, mountains, soft light",
    tags: index % 5 === 0 ? [] : [index % 2 ? "角色" : "风景"], updatedAt: "2026-09-20T08:00:00Z",
    versions: [
      { id: index * 2 + 1, name: "默认版本", text: index % 2 ? "1girl, silver hair, blue dress" : "scenery, mountains, soft light", hasImage: false },
      { id: index * 2 + 2, name: "暖色变体", text: "warm light, sunset, orange sky", hasImage: true },
    ],
  }));
  let nextMaterialId = 53, nextVersionId = 105;
  const materialCovers = new Map(materials.map(material => [material.id, material.id]));
  const versionCovers = new Map(materials.flatMap(material => material.versions.filter(version => version.hasImage).map(version => [version.id, material.id + 1] as const)));
  const requiredDoc = (id: unknown) => { const doc = docs.find(item => item.id === id); if (!doc) throw new Error("文档不存在"); return doc; };
  const requiredMaterial = (id: unknown) => { const item = materials.find(material => material.id === id); if (!item) throw new Error("素材不存在"); return item; };
  const normalizeTags = (values: string[]) => [...new Set(values.map(value => value.trim()).filter(Boolean))];
  const rowPage = (rows: RowRecord[], offset: number, limit: number) => ({ rows: rows.slice(offset, offset + limit), totalCount: rows.length, offset, limit, hasMore: offset + limit < rows.length });
  window.__mockCalls = [];
  window.__mockDelayMs = Number(new URLSearchParams(location.search).get("mockDelay") ?? 0);
  window.__mockFailNext = params.get("failNext") ?? undefined;
  const numeric = (value: number | null, comparison: FilterNumericComparison): boolean => {
    if (value === null) return false;
    const { operator, value: other, secondValue } = comparison;
    return operator === "equal" ? value === other : operator === "notEqual" ? value !== other : operator === "greaterThan" ? value > other : operator === "greaterOrEqual" ? value >= other : operator === "lessThan" ? value < other : operator === "lessOrEqual" ? value <= other : value >= other && value <= (secondValue ?? other);
  };
  const matches = (row: RowRecord, filter: LibraryFilter): boolean => {
    const textMatches = (text: string, values: string[], operator: string, sensitive = false) => {
      const textValue = sensitive ? text : text.toLocaleLowerCase();
      const hits = values.map(value => textValue.includes(sensitive ? value : value.toLocaleLowerCase()));
      return operator === "isEmpty" ? !text.trim() : operator === "containsAll" ? hits.every(Boolean) : operator === "containsNone" ? !hits.some(Boolean) : hits.some(Boolean);
    };
    switch (filter.type) {
      case "tag": return filter.operator === "isEmpty" ? !row.tags.length : filter.operator === "hasAll" ? filter.values.every(tag => row.tags.includes(tag)) : filter.operator === "hasNone" ? filter.values.every(tag => !row.tags.includes(tag)) : filter.values.some(tag => row.tags.includes(tag));
      case "group": return filter.operator === "isEmpty" ? row.groupId === null : filter.operator === "is" ? row.groupId === filter.groupId : row.groupId !== filter.groupId;
      case "artist": {
        const artists = (row.artists ?? "").split("\n").filter(Boolean);
        return filter.operator === "isSingle" ? artists.length === 1 : filter.operator === "isMultiple" ? artists.length > 1 : textMatches(row.artists ?? "", filter.values, filter.operator);
      }
      case "prompt": return textMatches([row.positivePrompt, row.characterPrompt, row.negativePrompt].filter(Boolean).join("\n"), filter.values, filter.operator, filter.caseSensitive);
      case "vibe": return filter.operator === "hasAny" ? (row.vibeReferenceCount ?? 0) > 0 : filter.operator === "hasNone" ? row.vibeReferenceCount === 0 : !!filter.comparison && numeric(row.vibeReferenceCount, filter.comparison);
      case "favorite": return row.favorite;
      case "note": return filter.operator === "isNotEmpty" ? !!row.note?.trim() : textMatches(row.note ?? "", [filter.value], filter.operator, filter.caseSensitive);
      case "metadata": return filter.parsed !== row.metadataFailed;
      case "orientation": return filter.orientation === "landscape" ? row.imageWidth! > row.imageHeight! : filter.orientation === "portrait" ? row.imageWidth! < row.imageHeight! : row.imageWidth === row.imageHeight;
      case "imageDimension": return numeric(filter.field === "width" ? row.imageWidth : filter.field === "height" ? row.imageHeight : row.imageWidth! / row.imageHeight!, filter.comparison);
      case "generationText": {
        const value = ({ model: row.generationModel, sampler: row.generationSampler, noiseSchedule: row.generationNoiseSchedule, seed: row.generationSeed })[filter.field] ?? "";
        return filter.operator === "equals" ? (filter.caseSensitive ? value === filter.value : value.toLocaleLowerCase() === filter.value.toLocaleLowerCase()) : textMatches(value, [filter.value], "containsAny", filter.caseSensitive);
      }
      case "generationNumber": { const value = ({ steps: row.generationSteps, scale: row.generationScale, cfgRescale: row.generationCfgRescale })[filter.field]; return numeric(value === null ? null : Number(value), filter.comparison); }
    }
  };
  const filteredRows = (query: Partial<RowQuery>) => {
    const search = (query.search ?? "").toLowerCase();
    return libraryRows.filter(row => (!search || `${row.imagePath} ${row.positivePrompt} ${row.characterPrompt} ${row.artists}`.toLowerCase().includes(search))
      && (!query.untaggedOnly || row.tags.length === 0)
      && (!query.artistFilter || row.artists === query.artistFilter)
      && (!query.singleArtistOnly || (row.artists ?? "").split("\n").filter(Boolean).length === 1)
      && (query.groupView || !query.hideGrouped || row.groupId === null)
      && (!query.hasVibe || (row.vibeReferenceCount ?? 0) > 0)
      && (!query.filters?.length || query.filters.every(filter => matches(row, filter)))
      && (!query.tags?.length || (query.tagMode === "or" ? query.tags.some(tag => row.tags.includes(tag)) : query.tags.every(tag => row.tags.includes(tag)))));
  };
  const selectedRows = (selection: RowSelection) => selection.kind === "explicit"
    ? libraryRows.filter(row => selection.rowIds.includes(row.id))
    : representativeRows(selection).filter(row => !selection.excludedRowIds.includes(row.id));
  let recentTags: string[] = [];
  const knownTags = new Set([...libraryRows.flatMap(row => row.tags), ...materials.flatMap(material => material.tags)]);
  // Browser automation-rule fixtures exercise real state changes. Files stay in memory;
  // the Rust engine and filesystem remain the authority for native integration tests.
  const automationRules: AutomationRule[] = [{ ...emptyAutomationRuleDraft(), id: 1, position: 0, name: "清晨风景自动收藏", description: "开发验证规则", enabled: false, createdAt: now(), updatedAt: now(), conditions: { mode: "any", negate: false, groups: [{ mode: "all", conditions: [{ type: "prompt", scope: "positiveAndCharacter", operator: "containsAll", value: "morning", caseSensitive: false }] }] }, actions: [{ type: "addTags", tags: ["收藏"] }] }];
  let nextRuleId = 2;
  const ruleFiles = new Map<string, string>();
  const ruleRequired = (id: unknown) => { const rule = automationRules.find(item => item.id === id); if (!rule) throw new Error("规则不存在"); return rule; };
  const validateRule = (draft: AutomationRuleDraft) => {
    if (!draft.name?.trim()) throw new Error("请填写规则名称");
    if (!draft.conditions?.groups?.length || draft.conditions.groups.some(group => !group.conditions.length)) throw new Error("每条规则及条件组至少需要一个条件");
    if (!draft.actions?.length) throw new Error("至少需要一个执行任务");
    for (const condition of draft.conditions.groups.flatMap(group => group.conditions)) {
      if ((condition.type === "prompt" || condition.type === "fileText" || condition.type === "generationText" || condition.type === "note" && condition.operator === "contains") && !condition.value.trim()) throw new Error("条件内容不能为空");
      if (condition.type === "tag" && condition.operator !== "isEmpty" && !normalizeTags(condition.tags).length) throw new Error("请选择条件 Tag");
      if (condition.type === "group" && condition.operator !== "isEmpty" && !condition.groupId) throw new Error("请选择条件分组");
      if (condition.type === "artist" && ["containsAny", "containsNone"].includes(condition.operator) && !normalizeTags(condition.artists).length) throw new Error("请填写画师名");
      if ("operator" in condition && condition.operator === "regex" && "value" in condition) new RegExp(condition.value);
      if ("comparison" in condition && condition.comparison) { const comparison = condition.comparison; if (!Number.isFinite(comparison.value) || comparison.operator === "between" && (comparison.secondValue === null || comparison.secondValue < comparison.value)) throw new Error("数值范围无效"); }
    }
    for (const action of draft.actions) {
      if ((action.type === "addTags" || action.type === "removeTags") && !normalizeTags(action.tags).length) throw new Error("请选择任务 Tag");
      if (action.type === "setGroup" && !groups.some(group => group.id === action.groupId)) throw new Error("目标分组不存在");
      if ((action.type === "appendPrompt" || action.type === "deletePromptTags") && !action.value.trim()) throw new Error("请填写提示词任务内容");
      if (action.type === "replacePrompt" && !action.find) throw new Error("查找内容不能为空");
      if (action.type === "prefixArtist" && !action.artists.length) throw new Error("请填写需要修正的画师名");
    }
  };
  const ruleTextMatches = (text: string, expected: string, operator: string, sensitive: boolean) => {
    if (operator === "regex") return new RegExp(expected, sensitive ? "u" : "iu").test(text);
    const available = sensitive ? text : text.toLocaleLowerCase(), value = sensitive ? expected : expected.toLocaleLowerCase();
    return operator === "equals" || operator === "textEquals" ? available === value : available.includes(value);
  };
  const ruleConditionMatches = (row: RowRecord, condition: RuleCondition): boolean => {
    switch (condition.type) {
      case "prompt": {
        const text = (condition.scope === "positive" ? [row.positivePrompt] : condition.scope === "character" ? [row.characterPrompt] : condition.scope === "negative" ? [row.negativePrompt] : condition.scope === "all" ? [row.positivePrompt, row.characterPrompt, row.negativePrompt] : [row.positivePrompt, row.characterPrompt]).filter(Boolean).join("\n");
        if (!condition.operator.startsWith("contains")) return ruleTextMatches(text, condition.value, condition.operator, condition.caseSensitive);
        const tokens = new Set(splitListText(text).map(token => token.toLocaleLowerCase()));
        const hits = splitListText(condition.value).map(token => tokens.has(token.toLocaleLowerCase()));
        return condition.operator === "containsAll" ? hits.every(Boolean) : condition.operator === "containsNone" ? !hits.some(Boolean) : hits.some(Boolean);
      }
      case "tag": return condition.operator === "isEmpty" ? !row.tags.length : condition.operator === "hasAll" ? condition.tags.every(tag => row.tags.includes(tag)) : condition.operator === "hasNone" ? condition.tags.every(tag => !row.tags.includes(tag)) : condition.tags.some(tag => row.tags.includes(tag));
      case "group": return condition.operator === "isEmpty" ? row.groupId === null : condition.operator === "is" ? row.groupId === condition.groupId : row.groupId !== condition.groupId;
      case "artist": { const names = splitListText(row.artists ?? "").map(name => name.replace(/^artist:/i, "").trim().toLowerCase()); const hits = condition.artists.map(name => names.includes(name.replace(/^artist:/i, "").trim().toLowerCase())); return condition.operator === "isEmpty" ? !names.length : condition.operator === "isSingle" ? names.length === 1 : condition.operator === "isMultiple" ? names.length > 1 : condition.operator === "containsNone" ? !hits.some(Boolean) : hits.some(Boolean); }
      case "note": return condition.operator === "isEmpty" ? !row.note?.trim() : ruleTextMatches(row.note ?? "", condition.value, "contains", condition.caseSensitive);
      case "fileText": return ruleTextMatches(condition.field === "fileName" ? (row.imagePath ?? "").split(/[\\/]/).pop() ?? "" : condition.field === "originalPath" ? row.imagePath ?? "" : "D:\\mock", condition.value, condition.operator, condition.caseSensitive);
      case "fileSize": return numeric(1_048_576, condition.comparison);
      case "sourceType": return (condition.sourceType === "folder") !== condition.negate;
      case "vibe": return condition.operator === "hasAny" ? (row.vibeReferenceCount ?? 0) > 0 : condition.operator === "hasNone" ? !row.vibeReferenceCount : !!condition.comparison && numeric(row.vibeReferenceCount, condition.comparison);
      case "metadata": return condition.parsed !== row.metadataFailed;
      case "orientation": return (condition.orientation === "landscape" ? row.imageWidth! > row.imageHeight! : condition.orientation === "portrait" ? row.imageWidth! < row.imageHeight! : row.imageWidth === row.imageHeight) !== condition.negate;
      case "imageDimension": return numeric(condition.field === "width" ? row.imageWidth : condition.field === "height" ? row.imageHeight : row.imageWidth! / row.imageHeight!, condition.comparison);
      case "generationText": return ruleTextMatches(({ model: row.generationModel, sampler: row.generationSampler, noiseSchedule: row.generationNoiseSchedule, seed: row.generationSeed })[condition.field] ?? "", condition.value, condition.operator, condition.caseSensitive);
      case "generationNumber": { const value = ({ steps: row.generationSteps, scale: row.generationScale, cfgRescale: row.generationCfgRescale })[condition.field]; return numeric(value === null ? null : Number(value), condition.comparison); }
    }
  };
  const runMockRule = (draft: AutomationRuleDraft, mutate: boolean) => {
    validateRule(draft);
    let matchedRows = 0, changedRows = 0, stoppedRows = 0;
    const sampleRowIds: number[] = [], sequence = new Map<string, number>();
    for (const row of libraryRows) {
      const results = draft.conditions.groups.map(group => { const matches = group.conditions.map(condition => ruleConditionMatches(row, condition)); return group.mode === "all" ? matches.every(Boolean) : matches.some(Boolean); });
      if ((draft.conditions.mode === "all" ? results.every(Boolean) : results.some(Boolean)) === draft.conditions.negate) continue;
      matchedRows++; if (sampleRowIds.length < 16) sampleRowIds.push(row.id);
      const edited = structuredClone(row);
      for (const action of draft.actions) {
        if (action.type === "addTags") edited.tags = normalizeTags([...edited.tags, ...action.tags]);
        if (action.type === "removeTags") edited.tags = edited.tags.filter(tag => !action.tags.includes(tag));
        if (action.type === "setGroup" && (!action.onlyIfUngrouped || edited.groupId === null)) { const group = groups.find(group => group.id === action.groupId)!; edited.groupId = group.id; edited.groupName = group.name; }
        if (action.type === "clearGroup") { edited.groupId = null; edited.groupName = null; }
        if (action.type === "setNote") edited.note = action.value || null;
        if (action.type === "clearNote") edited.note = null;
        if (action.type === "appendNote") edited.note = [edited.note, action.value].filter(Boolean).join(action.separator) || null;
        if (action.type === "setNoteSequence") {
          const numbers = libraryRows.map(item => item.note?.startsWith(action.prefix) ? Number(item.note.slice(action.prefix.length)) : NaN).filter(Number.isFinite);
          if (!edited.note?.startsWith(action.prefix) || !Number.isFinite(Number(edited.note.slice(action.prefix.length))) || !edited.note.slice(action.prefix.length)) { const next = (sequence.get(action.prefix) ?? Math.max(0, ...numbers)) + 1; sequence.set(action.prefix, next); edited.note = `${action.prefix}${next}`; }
        }
        if (action.type === "appendPrompt" || action.type === "deletePromptTags" || action.type === "replacePrompt") {
          const field = ({ positive: "positivePrompt", character: "characterPrompt", negative: "negativePrompt" } as const)[action.field];
          const text = edited[field] ?? "";
          if (action.type === "appendPrompt") edited[field] = normalizeTags([...splitListText(text), ...splitListText(action.value)]).join(", ");
          if (action.type === "deletePromptTags") { const remove = new Set(splitListText(action.value).map(value => value.toLowerCase())); edited[field] = splitListText(text).filter(value => !remove.has(value.toLowerCase())).join(", ") || null; }
          if (action.type === "replacePrompt") edited[field] = text.replace(new RegExp(action.find.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), action.caseSensitive ? "g" : "gi"), () => action.replace) || null;
        }
        if (action.type === "prefixArtist") for (const field of ["positivePrompt", "characterPrompt", "negativePrompt"] as const) edited[field] = splitListText(edited[field] ?? "").map(token => action.artists.some(artist => artist.replace(/^artist:/i, "").trim().toLowerCase() === token.toLowerCase()) ? `artist:${token}` : token).join(", ") || null;
        if (action.type === "stopProcessing") stoppedRows++;
      }
      if (JSON.stringify(edited) !== JSON.stringify(row)) { changedRows++; if (mutate) { Object.assign(row, edited); edited.tags.forEach(tag => knownTags.add(tag)); } }
    }
    return { scannedRows: libraryRows.length, matchedRows, rowsNeedingChanges: changedRows, stoppedRows, sampleRowIds };
  };
  const prepareRuleImport = async (text: string) => {
    if (new TextEncoder().encode(text).length > 2 * 1024 * 1024) throw new Error("粘贴内容超过 2 MB 上限");
    const blocks = [...text.matchAll(/```(?:json)?\s*\n([\s\S]*?)\n```/gi)];
    if (blocks.length > 1) throw new Error("一次只能粘贴一份 JSON 代码块");
    const document = JSON.parse(blocks.length ? blocks[0][1] : text.trim());
    if (document.format !== "smart-spreadsheet.automation-rules" || document.version !== 1 || !Array.isArray(document.rules) || !document.rules.length || document.rules.length > 200) throw new Error("这不是支持的智能表格规则文件");
    const missingTags = new Set<string>(), missingGroups = new Set<string>(), usedNames = new Set(automationRules.map(rule => rule.name));
    const drafts: AutomationRuleDraft[] = [], previews: AutomationRuleImportInspection["rules"] = [];
    const portableRules = structuredClone(document.rules) as Array<AutomationRuleDraft>;
    for (const value of portableRules) {
      const draft = structuredClone(value);
      const refs = [...draft.conditions.groups.flatMap(group => group.conditions), ...draft.actions];
      for (const ref of refs) {
        if ("tags" in ref) ref.tags.forEach(tag => { if (!knownTags.has(tag)) missingTags.add(tag); });
        if ("groupName" in ref && typeof ref.groupName === "string") { const name = ref.groupName; if (!groups.some(group => group.name === name)) missingGroups.add(name); Object.assign(ref, { groupId: groups.find(group => group.name === name)?.id ?? nextGroupId + [...missingGroups].indexOf(name) }); delete ref.groupName; }
      }
      const original = draft.name.trim(); let name = original, suffix = 2;
      while (usedNames.has(name)) name = `${original}（${suffix++}）`;
      usedNames.add(name); draft.name = name; draft.enabled = false;
      // Missing group targets are created only when the user confirms importing.
      if (!draft.name || !draft.conditions.groups.length || !draft.actions.length) throw new Error("规则内容不完整");
      drafts.push(draft);
      previews.push({ name: original, importedName: name, conditionCount: draft.conditions.groups.reduce((sum, group) => sum + group.conditions.length, 0), actionCount: draft.actions.length, runOnImport: draft.runOnImport, runOnUpdate: draft.runOnUpdate });
    }
    const hash = [...new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)))].map(byte => byte.toString(16).padStart(2, "0")).join("");
    return { drafts, inspection: { contentHash: hash, version: 1, ruleCount: drafts.length, rules: previews, missingTags: [...missingTags], missingGroups: [...missingGroups], renamedRules: previews.filter(rule => rule.name !== rule.importedName).length } satisfies AutomationRuleImportInspection };
  };
  const clusterKey = (row: RowRecord, mode: unknown) => mode === "artists" ? row.artists?.trim() || null : mode === "positivePrompt" ? row.positivePrompt?.trim() || null : mode === "vibes" && row.vibeReferenceCount ? `mock-vibe-${row.vibeReferenceCount}` : null;
  const representativeRows = (query: Partial<RowQuery>) => {
    const seen = new Set<string>();
    return filteredRows(query).filter(row => {
      const key = query.groupView ? row.groupId === null ? null : `group:${row.groupId}` : clusterKey(row, query.dedupe);
      if (key === null) return true;
      if (seen.has(key)) return false;
      seen.add(key); return true;
    });
  };
  const clustersFor = (query: Partial<RowQuery>) => {
    const clusters = new Map<string, RowRecord[]>();
    for (const row of filteredRows(query)) { const key = clusterKey(row, query.dedupe); if (key) clusters.set(key, [...clusters.get(key) ?? [], row]); }
    return new Map([...clusters].filter(([, rows]) => rows.length > 1));
  };
  const inspection = async (title: string, id: number): Promise<MaterialInspection> => ({ title, preview: [...new Uint8Array(await previewPng(id))], sections: [
    { id: "positive", label: "正向提示词", text: libraryRows.find(row => row.id === id)?.positivePrompt ?? "scenery, mountains, soft light" },
    { id: "negative", label: "负向提示词", text: "lowres, worst quality" },
  ], warning: null });
  const internals = {
    metadata: { currentWindow: { label: windowLabel }, currentWebview: { label: windowLabel } },
    convertFileSrc(path: string): string { return assetUrls.get(path) ?? path; },
    transformCallback(callback: (payload: unknown) => void): number {
      callbackCounter += 1;
      const id = callbackCounter;
      eventListeners.set(id, { event: "", handler: callback });
      // listen 命令随后会携带 event 名重新登记；这里先占位。
      void callback;
      return id;
    },
    async invoke(command: string, args: Record<string, unknown> | undefined): Promise<unknown> {
      const payload = args ?? {};
      window.__mockCalls?.push({ command, payload: structuredClone(payload) });
      if (!command.startsWith("plugin:")) {
        if (window.__mockDelayMs) await new Promise(resolve => setTimeout(resolve, window.__mockDelayMs));
        if (window.__mockFailNext === command) {
          window.__mockFailNext = undefined;
          throw new Error("模拟查询失败，请重试");
        }
      }
      switch (command) {
        case "list_automation_rules": return automationRules;
        case "create_automation_rule": {
          const draft = payload.draft as AutomationRuleDraft; validateRule(draft);
          const rule = { ...structuredClone(draft), name: draft.name.trim(), id: nextRuleId++, position: automationRules.length, createdAt: now(), updatedAt: now() };
          automationRules.push(rule); return rule;
        }
        case "update_automation_rule": {
          const draft = payload.draft as AutomationRuleDraft; validateRule(draft);
          const rule = ruleRequired(payload.id); Object.assign(rule, structuredClone(draft), { name: draft.name.trim(), updatedAt: now() }); return rule;
        }
        case "set_automation_rule_enabled": { const rule = ruleRequired(payload.id); rule.enabled = Boolean(payload.enabled); rule.updatedAt = now(); return; }
        case "delete_automation_rule": { const rule = ruleRequired(payload.id); automationRules.splice(automationRules.indexOf(rule), 1); return true; }
        case "reorder_automation_rules": {
          const ids = payload.ids as number[];
          if (ids.length !== automationRules.length || new Set(ids).size !== ids.length) throw new Error("规则排序范围无效");
          const ordered = ids.map(id => ruleRequired(id)); ordered.forEach((rule, index) => { rule.position = index; }); automationRules.splice(0, automationRules.length, ...ordered); return;
        }
        case "preview_automation_rule": return runMockRule(ruleRequired(payload.id), false);
        case "preview_automation_rule_draft": return runMockRule(payload.draft as AutomationRuleDraft, false);
        case "run_automation_rule_on_library": {
          const rule = ruleRequired(payload.id), result = runMockRule(rule, true);
          return { trigger: "manual", inputRows: result.scannedRows, changedRows: result.rowsNeedingChanges, reports: [{ ruleId: rule.id, ruleName: rule.name, scannedRows: result.scannedRows, matchedRows: result.matchedRows, changedRows: result.rowsNeedingChanges, actionsChanged: result.rowsNeedingChanges, stoppedRows: result.stoppedRows, error: null }], engineError: null };
        }
        case "inspect_automation_rule_file":
        case "inspect_automation_rule_text": {
          const text = command.endsWith("_text") ? String(payload.text) : ruleFiles.get(String(payload.path)) ?? JSON.stringify({ format: "smart-spreadsheet.automation-rules", version: 1, rules: [{ ...emptyAutomationRuleDraft(), name: "导入规则示例", conditions: { mode: "any", negate: false, groups: [{ mode: "all", conditions: [{ type: "tag", operator: "hasAny", tags: ["风景"] }] }] }, actions: [{ type: "addTags", tags: ["导入规则创建的Tag"] }] }] });
          if (command.endsWith("_file")) ruleFiles.set(String(payload.path), text);
          return (await prepareRuleImport(text)).inspection;
        }
        case "import_automation_rule_file":
        case "import_automation_rule_text": {
          const text = command.endsWith("_text") ? String(payload.text) : ruleFiles.get(String(payload.path));
          if (!text) throw new Error("规则文件不存在，请重新检查");
          const { drafts, inspection } = await prepareRuleImport(text);
          if (inspection.contentHash !== payload.expectedHash) throw new Error("规则内容发生变化，请重新检查后导入");
          inspection.missingTags.forEach(tag => knownTags.add(tag));
          inspection.missingGroups.forEach(name => groups.push({ id: nextGroupId++, name, memberCount: 0, createdAt: now() }));
          const importedRuleIds = drafts.map(draft => { const rule = { ...draft, enabled: false, id: nextRuleId++, position: automationRules.length, createdAt: now(), updatedAt: now() }; automationRules.push(rule); return rule.id; });
          return { importedRules: drafts.length, createdTags: inspection.missingTags.length, createdGroups: inspection.missingGroups.length, renamedRules: inspection.renamedRules, importedRuleIds };
        }
        case "export_automation_rules": {
          const ids = payload.ids as number[]; if (!ids.length) throw new Error("请选择要导出的规则");
          const drafts = ids.map(id => { const rule = ruleRequired(id); const { name, description, enabled, runOnImport, runOnUpdate, conditions, actions } = rule; return structuredClone({ name, description, enabled, runOnImport, runOnUpdate, conditions, actions }); });
          for (const draft of drafts) for (const ref of [...draft.conditions.groups.flatMap(group => group.conditions), ...draft.actions]) if ("groupId" in ref && ref.groupId) { Object.assign(ref, { groupName: groups.find(group => group.id === ref.groupId)?.name }); Reflect.deleteProperty(ref, "groupId"); }
          ruleFiles.set(String(payload.path), JSON.stringify({ format: "smart-spreadsheet.automation-rules", version: 1, rules: drafts }));
          return { path: String(payload.path), exportedRules: ids.length };
        }
        case "get_app_snapshot": return structuredClone(snapshot);
        case "plugin:dialog|save": return params.has("mockPaths") ? "D:\\Agent\\Agent_temp\\mock-export.json" : null;
        case "plugin:dialog|open": return params.has("mockPaths") ? "D:\\Agent\\Agent_temp\\mock-images" : null;
        case "set_auto_artist_prefix_on_import":
          snapshot.autoArtistPrefixOnImport = Boolean(payload.enabled);
          return { ...snapshot };
        case "query_rows": {
          const query = payload.query as RowQuery;
          let rows = representativeRows(query);
          if (payload.sort === "timeDesc") rows = [...rows].reverse();
          return { rows: structuredClone(rows.slice(query.offset, query.offset + query.limit)), totalCount: rows.length, offset: query.offset, limit: query.limit, hasMore: query.offset + query.limit < rows.length };
        }
        case "get_rows_by_ids": return structuredClone(libraryRows.filter(row => (payload.rowIds as number[]).includes(row.id)));
        case "count_selected_rows": return selectedRows(payload.selection as RowSelection).length;
        case "selected_row_ids": return selectedRows(payload.selection as RowSelection).map(row => row.id);
        case "update_positive_prompt":
        case "update_character_prompt":
        case "update_negative_prompt":
        case "set_favorite":
        case "update_note": {
          const row = libraryRows.find(item => item.id === payload.rowId);
          if (!row) throw new Error("图片不存在");
          if (command === "update_positive_prompt") row.positivePrompt = String(payload.newPrompt);
          if (command === "update_character_prompt") row.characterPrompt = String(payload.newPrompt);
          if (command === "update_negative_prompt") row.negativePrompt = String(payload.newPrompt);
          if (command === "update_note") row.note = String(payload.note).trim() || null;
          if (command === "set_favorite") row.favorite = Boolean(payload.favorite);
          return command === "update_positive_prompt" || command === "update_character_prompt" ? { affectedRows: 1, newArtists: row.artists } : 1;
        }
        case "restore_mutable_row_states": {
          const states = payload.states as MutableRowState[];
          for (const state of states) {
            const row = libraryRows.find(item => item.id === state.rowId);
            if (row) { const { rowId: _id, ...fields } = state; Object.assign(row, structuredClone(fields)); row.groupName = groups.find(group => group.id === row.groupId)?.name ?? null; for (const tag of row.tags) knownTags.add(tag); }
          }
          return states.length;
        }
        case "get_row_index": return libraryRows.findIndex(row => row.id === payload.rowId);
        case "list_tags": return [...knownTags].map(name => ({ name, rowCount: libraryRows.filter(row => row.tags.includes(name)).length }));
        case "create_tag": { const name = String(payload.name).trim(); if (!name) throw new Error("Tag 不能为空"); const added = !knownTags.has(name); knownTags.add(name); return added; }
        case "delete_tag": { const name = String(payload.name); const removed = knownTags.delete(name); for (const item of [...libraryRows, ...materials]) item.tags = item.tags.filter(tag => tag !== name); return removed; }
        case "rename_tag": {
          const oldName = String(payload.oldName), newName = String(payload.newName).trim(); if (!newName) throw new Error("Tag 不能为空"); if (!knownTags.has(oldName)) return false;
          knownTags.delete(oldName); knownTags.add(newName); for (const item of [...libraryRows, ...materials]) item.tags = normalizeTags(item.tags.map(tag => tag === oldName ? newName : tag)); return true;
        }
        case "set_tags_for_row": {
          const row = libraryRows.find(item => item.id === payload.rowId); if (!row) throw new Error("图片不存在");
          const tags = normalizeTags(payload.tags as string[]); const associationsChanged = row.tags.filter(tag => !tags.includes(tag)).length + tags.filter(tag => !row.tags.includes(tag)).length;
          row.tags = tags; for (const tag of tags) knownTags.add(tag); return { affectedRows: 1, normalizedTags: tags, associationsChanged };
        }
        case "get_recent_tags": return JSON.stringify(recentTags);
        case "set_recent_tags": recentTags = JSON.parse(String(payload.json)) as string[]; return null;
        case "list_selection_tags": {
          const selected = selectedRows(payload.selection as RowSelection);
          return [...knownTags].map(name => ({ name, selectedRows: selected.filter(row => row.tags.includes(name)).length }));
        }
        case "add_tags_to_selection":
        case "remove_tags_from_selection": {
          const selected = selectedRows(payload.selection as RowSelection);
          const tags = (payload.tags as string[]).map(tag => tag.trim()).filter(Boolean);
          let associationsChanged = 0;
          for (const row of selected) {
            const next = command === "add_tags_to_selection" ? [...new Set([...row.tags, ...tags])] : row.tags.filter(tag => !tags.includes(tag));
            associationsChanged += Math.abs(next.length - row.tags.length); row.tags = next;
          }
          if (command === "add_tags_to_selection") for (const tag of tags) knownTags.add(tag);
          return { affectedRows: selected.length, normalizedTags: tags, associationsChanged };
        }
        case "export_zhihuiji_json": return { path: payload.path, exported: selectedRows(payload.selection as RowSelection).length, duplicatesRemoved: 0, artistsAdded: 0 };
        case "export_xlsx": return { path: payload.path, rowCount: selectedRows(payload.selection as RowSelection).length, imagesEmbedded: 0, imageFailures: 0 };
        case "export_prompt_rotation_json": return { path: payload.path, exported: selectedRows(payload.selection as RowSelection).length };
        case "export_image_files": return { directory: payload.parentDir, exported: selectedRows(payload.selection as RowSelection).length, missing: 0, hardlinkFallbacks: 0 };
        case "list_prompt_docs": return docs.map(({ content: _content, assets: _assets, ...summary }) => summary).sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
        case "create_prompt_doc": {
          const doc: PromptDocDetail = { id: `mock-doc-${nextDocId++}`, title: String(payload.title).trim() || "未命名文档", createdAt: now(), updatedAt: now(), plainText: "", content: { type: "doc", content: [{ type: "paragraph" }] }, assets: [] };
          docs.push(doc); return doc;
        }
        case "load_prompt_doc": return requiredDoc(payload.docId);
        case "save_prompt_doc": {
          const doc = requiredDoc(payload.docId);
          Object.assign(doc, { title: String(payload.title).trim() || "未命名文档", content: structuredClone(payload.content), plainText: String(payload.plainText), updatedAt: now() });
          return doc;
        }
        case "delete_prompt_doc": { const doc = requiredDoc(payload.docId); docs.splice(docs.indexOf(doc), 1); return null; }
        case "import_prompt_doc_image_from_path":
        case "import_prompt_doc_image_bytes": {
          const doc = requiredDoc(payload.docId);
          const asset = { src: `assets/mock-image-${nextAssetId++}.png`, path: `D:\\mock\\prompt-docs\\${doc.id}\\image-${nextAssetId}.png` };
          const bytes = command === "import_prompt_doc_image_bytes" ? new Uint8Array(payload.bytes as number[]) : new Uint8Array(await previewPng(nextAssetId));
          if (!bytes.length) throw new Error("图片内容为空");
          let binary = ""; for (const byte of bytes) binary += String.fromCharCode(byte);
          assetUrls.set(asset.path, `data:image/png;base64,${btoa(binary)}`); doc.assets.push(asset); return asset;
        }
        case "list_materials": {
          const search = String(payload.search ?? "").trim().toLocaleLowerCase();
          const tags = normalizeTags(payload.tags as string[] ?? []);
          const items = materials.filter(item => (!payload.untagged || !item.tags.length) && tags.every(tag => item.tags.includes(tag)) && (!search || [item.title, ...item.versions.flatMap(version => [version.name, version.text])].some(value => value.toLocaleLowerCase().includes(search)))).sort((a, b) => b.updatedAt.localeCompare(a.updatedAt) || b.id - a.id);
          return { items: items.slice(Number(payload.offset), Number(payload.offset) + 48), total: items.length };
        }
        case "material_tag_counts": return [...knownTags].sort().map(name => ({ name, rowCount: materials.filter(item => item.tags.includes(name)).length }));
        case "material_ids_for_tag": return materials.filter(item => item.tags.includes(String(payload.name))).map(item => item.id);
        case "restore_material_tag": {
          const name = String(payload.name); if (knownTags.has(name)) for (const item of materials) if ((payload.ids as number[]).includes(item.id)) item.tags = normalizeTags([...item.tags, name]); return null;
        }
        case "inspect_material_image": return inspection(String(payload.path).split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "图片素材", 1);
        case "inspect_material_library_image": {
          const row = libraryRows.find(item => item.id === payload.rowId); if (!row) throw new Error("图片不存在");
          return { path: row.imagePath, inspection: await inspection(`图片 ${row.id}`, row.id) };
        }
        case "save_material": {
          const draft = payload.draft as MaterialDraft;
          const previous = draft.id === null ? undefined : requiredMaterial(draft.id);
          if (!previous && !draft.imagePath) throw new Error("新素材需要图片");
          if (!draft.versions.length || draft.versions.length > 128) throw new Error("素材必须包含 1 至 128 个版本");
          const seen = new Set<number>();
          for (const version of draft.versions) {
            if (version.id !== null && (!previous?.versions.some(item => item.id === version.id) || seen.has(version.id))) throw new Error("版本不属于当前素材或重复");
            if (version.id !== null) seen.add(version.id);
            if (version.imageSourceId !== null && !previous?.versions.some(item => item.id === version.imageSourceId)) throw new Error("图片来源版本不存在");
          }
          const id = previous?.id ?? nextMaterialId++;
          const images = draft.versions.map(version => version.imagePath ? id + 1 : version.imageSourceId === null ? undefined : versionCovers.get(version.imageSourceId));
          const versions = draft.versions.map((version, index) => ({ id: version.id ?? nextVersionId++, name: version.name.trim() || `版本 ${index + 1}`, text: version.text, hasImage: images[index] !== undefined }));
          for (const version of previous?.versions ?? []) versionCovers.delete(version.id);
          versions.forEach((version, index) => { if (images[index] !== undefined) versionCovers.set(version.id, images[index]!); });
          const item: Material = { id, title: draft.title.trim() || "未命名素材", text: versions[0].text, tags: normalizeTags(draft.tags), updatedAt: now(), versions };
          if (draft.imagePath || !previous) materialCovers.set(id, id);
          for (const tag of item.tags) knownTags.add(tag);
          if (previous) materials.splice(materials.indexOf(previous), 1, item); else materials.push(item);
          return item;
        }
        case "delete_material": { const item = requiredMaterial(payload.id); materials.splice(materials.indexOf(item), 1); materialCovers.delete(item.id); for (const version of item.versions) versionCovers.delete(version.id); return null; }
        case "material_image": { const item = requiredMaterial(payload.id); return previewPng(materialCovers.get(item.id)!); }
        case "material_version_image": {
          const owner = materials.find(item => item.versions.some(version => version.id === payload.id)); if (!owner) throw new Error("素材版本不存在");
          return previewPng(versionCovers.get(Number(payload.id)) ?? materialCovers.get(owner.id)!);
        }
        case "list_groups": return groups.map(group => ({ ...group, memberCount: libraryRows.filter(row => row.groupId === group.id).length }));
        case "create_group":
        case "restore_group": {
          const restored = command === "restore_group" ? payload.group as GroupSummary : null;
          const name = (restored?.name ?? String(payload.name)).trim();
          if (!name) throw new Error("分组名称不能为空");
          if (groups.some(group => group.name === name || group.id === restored?.id)) throw new Error("分组已存在");
          const group = { id: restored?.id ?? nextGroupId++, name, memberCount: 0, createdAt: restored?.createdAt ?? now() }; nextGroupId = Math.max(nextGroupId, group.id + 1); groups.push(group); return group;
        }
        case "rename_group": {
          const group = groups.find(item => item.id === payload.groupId); if (!group) throw new Error("分组不存在");
          const name = String(payload.newName).trim(); if (!name || groups.some(item => item.id !== group.id && item.name === name)) throw new Error("分组名称为空或已存在");
          group.name = name; for (const row of libraryRows) if (row.groupId === group.id) row.groupName = name;
          return { ...group, memberCount: libraryRows.filter(row => row.groupId === group.id).length };
        }
        case "delete_group": { const index = groups.findIndex(item => item.id === payload.groupId); if (index < 0) return false; groups.splice(index, 1); for (const row of libraryRows) if (row.groupId === payload.groupId) { row.groupId = null; row.groupName = null; } return true; }
        case "delete_empty_groups": { const empty = groups.filter(group => !libraryRows.some(row => row.groupId === group.id)); for (const group of empty) groups.splice(groups.indexOf(group), 1); return empty.length; }
        case "assign_rows_to_group":
        case "ungroup_rows": {
          const group = command === "assign_rows_to_group" ? groups.find(item => item.id === payload.groupId) : undefined;
          if (command === "assign_rows_to_group" && !group) throw new Error("分组不存在");
          const rows = selectedRows(payload.selection as RowSelection); for (const row of rows) { row.groupId = group?.id ?? null; row.groupName = group?.name ?? null; } return rows.length;
        }
        case "get_group_members": return rowPage(libraryRows.filter(row => row.groupId === payload.groupId), Number(payload.offset), Number(payload.limit));
        case "list_dedupe_clusters": return [...clustersFor(payload as Partial<RowQuery>)].map(([key, rows]) => ({ key, memberCount: rows.length, alias: aliases.get(`${payload.dedupe}:${key}`) ?? null }));
        case "get_dedupe_cluster_members": return rowPage(clustersFor(payload as Partial<RowQuery>).get(String(payload.key)) ?? [], Number(payload.offset), Number(payload.limit));
        case "set_dedupe_alias": { const key = `${payload.dedupe}:${payload.key}`; const alias = String(payload.alias).trim(); if (alias) aliases.set(key, alias); else aliases.delete(key); return null; }
        case "list_distinct_artists": return ["artist:alpha", "artist:beta"];
        case "backfill_vibe_statuses":
        case "backfill_style_signatures": return { total: 0, processed: 0, updated: 0, unreadable: 0 };
        case "plugin:window|is_maximized": return false;
        case "plugin:window|is_focused": return true;
        case "plugin:event|listen": {
          const id = Number(payload.handler);
          const entry = eventListeners.get(id);
          if (entry) {
            entry.event = String(payload.event);
          }
          return id;
        }
        case "plugin:event|unlisten": {
          eventListeners.delete(Number(payload.eventId ?? -1));
          return null;
        }
        case "get_compare_sample": {
          // 按请求的 rowId 选样本：set-sample 事件切换后界面可见变化。
          const requested = Number(payload.rowId);
          const sample: MockRow = requested === 2
            ? { id: requested, positivePrompt: null, generationModel: null }
            : {
                id: requested,
                artists: "artist:alpha",
                positivePrompt: "artist:alpha, blue hair, school uniform, masterpiece",
                characterPrompt: "1girl, silver hair\nblue eyes",
                generationModel: "NovelAI Diffusion V4.5 Full",
                vibeReferenceCount: 3,
                tags: ["样本"],
              };
          const flags = requested === 2
            ? { hasStyleSignature: false, hasVibeSignature: false, vibeSignatureUnreadable: false }
            : { hasStyleSignature: true, hasVibeSignature: true, vibeSignatureUnreadable: false };
          return { row: rowDto(sample), ...flags };
        }
        case "query_compare_same_artists":
        case "query_compare_same_vibe_diff_style":
        case "query_compare_same_style_diff_vibe": {
          // 样本 2（空态样本）下分区如实返回空。
          if (Number(payload.rowId) === 2) {
            return sectionPage([], 0, Number(payload.limit));
          }
          if (new URLSearchParams(location.search).has("large")) {
            const offset = Number(payload.offset);
            const limit = Number(payload.limit);
            return { rows: Array.from({ length: Math.min(limit, 60_000 - offset) }, (_, index) => rowDto({ id: 101 + offset + index, artists: "artist:alpha", positivePrompt: "artist:alpha, blue hair, sunlight", generationModel: "NovelAI Diffusion V4.5 Full" })), totalCount: 60_000, offset, limit };
          }
          const rows = command === "query_compare_same_artists"
            ? SECTIONS.sameArtists
            : command === "query_compare_same_vibe_diff_style"
              ? SECTIONS.vibeDiffStyle
              : SECTIONS.styleDiffVibe;
          return sectionPage(rows, Number(payload.offset), Number(payload.limit));
        }
        case "query_compare_same_style_all_models":
          if (Number(payload.rowId) === 2) {
            return { rows: [], totalCount: 0, truncated: false };
          }
          if (Number(payload.rowId) === 3) {
            return {
              rows: SAME_MODEL_ROWS.map(rowDto),
              totalCount: SAME_MODEL_ROWS.length,
              truncated: false,
            };
          }
          if (new URLSearchParams(location.search).has("large")) {
            return { rows: Array.from({ length: 500 }, (_, index) => rowDto({ id: 1001 + index, generationModel: index % 2 ? "NovelAI Diffusion V4 Full" : "NovelAI Diffusion V3" })), totalCount: 60_000, truncated: true };
          }
          return {
            rows: MODEL_ROWS.map(rowDto),
            totalCount: MODEL_ROWS.length,
            truncated: false,
          };
        case "get_row_thumbnail":
        case "get_row_gallery_preview":
        case "get_row_preview":
        case "get_row_original":
          return previewPng(Number(payload.rowId));
        case "get_row_vibe_status":
          return 2;
        case "plugin:window|destroy":
          window.close();
          return null;
        case "plugin:event|emit":
          window.__mockEmit?.(String(payload.event), structuredClone(payload.payload));
          return null;
        // Cross-window/native window chrome have no host window in browser QA.
        case "plugin:event|emit_to":
        case "plugin:window|minimize":
        case "plugin:window|maximize":
        case "plugin:window|unmaximize":
        case "plugin:window|toggle_maximize":
        case "plugin:window|set_focus":
        case "plugin:window|show":
        case "plugin:window|hide":
        case "plugin:window|start_dragging":
        case "plugin:window|close": return null;
        default:
          throw new Error(`开发模拟尚未实现命令：${command}`);
      }
    },
  };
  // Match IPC ownership: callers never receive references to mutable mock storage.
  const invoke = internals.invoke.bind(internals);
  internals.invoke = async (command, payload) => structuredClone(await invoke(command, payload));
  window.__TAURI_INTERNALS__ = internals;
  window.__mockEmit = (event: string, payload: unknown) => {
    // 真实后端调用回调时传完整事件信封，@tauri-apps/api 再从中取 payload。
    for (const [id, entry] of eventListeners) {
      if (entry.event === event) {
        entry.handler({ event, id, payload });
      }
    }
  };
  // ?switchTo=<id>：装载 800ms 后模拟后端推送 set-sample（复用窗口切换样本路径）。
  const switchTo = new URLSearchParams(window.location.search).get("switchTo");
  if (switchTo) {
    window.setTimeout(() => {
      window.__mockEmit?.("compare://set-sample", Number(switchTo));
    }, 800);
  }
  void PAGE_SIZE_DEFAULT;
}
