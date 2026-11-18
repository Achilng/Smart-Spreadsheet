import { create } from "zustand";
import { getRowsByIds, type RowRecord } from "../../lib/api";
import { customPage, queryCompareCustom, startingConditions, type CompareOptions, type CompareGroups, type CustomPage } from "../../lib/api/compare-conditions";
import { notify } from "./notices";
import { getCompareSample, queryCompareSameArtists, queryCompareSameVibeDiffStyle, queryCompareSameStyleDiffVibe, queryCompareSameStyleAllModels, type CompareSample, type CompareModelSection } from "../../lib/api/compare";
import { createRequestQueue } from "../../lib/utils/request-queue";
import { modelComparisonTier, modelVersionBadge } from "../../lib/utils/model-version";

export type CompareSectionKey = "artists" | "vibeDiffStyle" | "styleDiffVibe" | "custom";
export type CompareTab = CompareSectionKey | "models";
export interface SectionState { items: RowRecord[]; total: number; loading: boolean; error: string | null; loaded: boolean; offset: number }
const emptySection = (): SectionState => ({ items: [], total: 0, loading: false, error: null, loaded: false, offset: 0 });
const sections = () => ({ groups: null as CompareGroups | null, memberGroup: null as number[] | null, custom: emptySection(), artists: emptySection(), vibeDiffStyle: emptySection(), styleDiffVibe: emptySection(), models: { rows: [], totalCount: 0, truncated: false } as CompareModelSection, modelsLoading: false, modelsError: null as string | null, modelsLoaded: false, modelTier: "", modelOffset: 0, manualTarget: false });
interface CompareState { groups: CompareGroups | null; memberGroup: number[] | null; custom: SectionState; options: CompareOptions; manualTarget: boolean; rowId: number | null; sample: CompareSample | null; sampleLoading: boolean; sampleError: string | null; activeTab: CompareTab; artists: SectionState; vibeDiffStyle: SectionState; styleDiffVibe: SectionState; models: CompareModelSection; modelsLoading: boolean; modelsError: string | null; modelsLoaded: boolean; modelTier: string; modelOffset: number; target: RowRecord | null }
export const useCompare = create<CompareState>(() => ({ options: startingConditions("blank"), rowId: null, sample: null, sampleLoading: false, sampleError: null, activeTab: "artists", target: null, ...sections() }));
const enqueue = createRequestQueue();
export const COMPARE_PAGE_SIZE = 24;
let generation = 0;
const errorText = (error: unknown) => error instanceof Error ? error.message : String(error);

export function applyCompareOptions(options: CompareOptions, page: CustomPage) {
  useCompare.setState({ options: { ...options }, groups: page.groups, memberGroup: null, activeTab: "custom", target: null, manualTarget: false,
    custom: { items: page.rows, total: page.totalCount, offset: page.offset, loading: false, error: null, loaded: true } });
}

export function resetCompare(): void {
  generation++;
  useCompare.setState({ options: startingConditions("blank"), activeTab: "artists", rowId: null, sample: null, sampleError: null, sampleLoading: false, target: null, ...sections() });
}

export async function setCompareSample(rowId: number, force = false, targetId?: number): Promise<void> {
  if (!Number.isInteger(rowId) || rowId <= 0) return;
  const state = useCompare.getState();
  if (!force && state.rowId === rowId && (state.sample || state.sampleLoading)) return;
  const requestGeneration = ++generation;
  const pair = targetId !== undefined && targetId > 0 && targetId !== rowId;
  useCompare.setState({ rowId, sample: null, sampleError: null, sampleLoading: true, target: null, ...sections() });
  try {
    const sample = await enqueue(() => generation === requestGeneration, () => getCompareSample(rowId));
    if (!sample || generation !== requestGeneration) return;
    useCompare.setState({ sample });
    if (pair) {
      try {
        const [target] = await getRowsByIds([targetId]);
        if (generation !== requestGeneration) return;
        if (target) openSideBySide(target, true);
        else notify("找不到目标 B，请重新选择。", "error");
      } catch (error) {
        if (generation === requestGeneration) notify(`读取目标 B 失败：${errorText(error)}`, "error");
      }
    }
    void selectCompareTab(useCompare.getState().activeTab);
  } catch (error) {
    if (generation === requestGeneration) useCompare.setState({ sampleError: errorText(error) });
  } finally {
    if (generation === requestGeneration) useCompare.setState({ sampleLoading: false });
  }
}
export async function refreshCompare(): Promise<void> {
  const rowId = useCompare.getState().rowId;
  if (rowId !== null) await setCompareSample(rowId, true);
}
export async function selectCompareTab(tab: CompareTab): Promise<void> {
  useCompare.setState({ activeTab: tab });
  const state = useCompare.getState();
  if (!state.sample) return;
  if (tab === "models") { if (!state.modelsLoaded) await loadCompareModels(); }
  else if (!state[tab].loaded) await loadCompareSectionPage(tab, true);
}
export async function loadCompareSectionPage(section: CompareSectionKey, first = false, direction: 1 | -1 = 1): Promise<void> {
  const state = useCompare.getState();
  if (state.rowId === null || state[section].loading) return;
  const before = state[section];
  if (!first && (direction === 1 ? before.offset + before.items.length >= before.total : before.offset === 0)) return;
  const offset = first ? 0 : Math.max(0, before.offset + direction * COMPARE_PAGE_SIZE);
  const requestGeneration = generation;
  const current = () => generation === requestGeneration && (section !== "custom" || useCompare.getState().options === state.options);
  const update = (patch: Partial<SectionState>) => { if (current()) useCompare.setState(current => ({ [section]: { ...current[section], ...patch } })); };
  update({ loading: true, error: null });
  try {
    const query = section === "artists" ? queryCompareSameArtists : section === "vibeDiffStyle" ? queryCompareSameVibeDiffStyle : queryCompareSameStyleDiffVibe;
    const page = await enqueue(() => current() && useCompare.getState().activeTab === section,
      async () => {
        if (section !== "custom") return query(state.rowId!, offset, COMPARE_PAGE_SIZE);
        const groups = state.groups ?? await queryCompareCustom(state.sample!, state.options);
        if (!current()) return;
        useCompare.setState({ groups });
        return customPage(groups, offset, COMPARE_PAGE_SIZE);
      });
    if (page) update({ items: page.rows, total: page.totalCount, offset: page.offset, loaded: true });
  } catch (error) { update({ error: errorText(error) }); }
  finally { update({ loading: false }); }
}
export async function loadCompareModels(): Promise<void> {
  const state = useCompare.getState();
  if (state.rowId === null || state.modelsLoading) return;
  const requestGeneration = generation;
  useCompare.setState({ modelsLoading: true, modelsError: null });
  try {
    const models = await enqueue(() => generation === requestGeneration && useCompare.getState().activeTab === "models", () => queryCompareSameStyleAllModels(state.rowId!));
    if (models && generation === requestGeneration) useCompare.setState({ models, modelsLoaded: true });
  } catch (error) {
    if (generation === requestGeneration) useCompare.setState({ modelsError: errorText(error) });
  } finally { if (generation === requestGeneration) useCompare.setState({ modelsLoading: false }); }
}
export function openSideBySide(target: RowRecord, manualTarget = false, memberGroup: number[] | null = null): void {
  useCompare.setState(state => state.activeTab === "models" ? { target, manualTarget, memberGroup } : { target, manualTarget, memberGroup, [state.activeTab]: { ...state[state.activeTab], error: null } });
}
export function closeSideBySide(): void { useCompare.setState({ target: null, memberGroup: null }); }

export interface CompareModelGroup { tier: string; title: string; rank: number; rows: RowRecord[] }
export function compareModelGroups(models: CompareModelSection, sample: RowRecord | undefined): CompareModelGroup[] {
  const groups = new Map<string, CompareModelGroup>();
  const sampleTier = modelComparisonTier(sample?.generationModel ?? null);
  for (const row of models.rows) {
    const tier = modelComparisonTier(row.generationModel);
    if (tier === sampleTier) continue;
    const badge = modelVersionBadge(row.generationModel);
    let group = groups.get(tier);
    if (!group) {
      group = { tier, title: badge?.label || row.generationModel?.trim() || "未知模型", rank: badge ? Number(/^v(\d+(?:\.\d+)?)/.exec(badge.label)?.[1] ?? 0) : -1, rows: [] };
      groups.set(tier, group);
    }
    group.rows.push(row);
  }
  return [...groups.values()].sort((a, b) => b.rank - a.rank || a.title.localeCompare(b.title));
}
export function selectCompareModel(tier: string): void { useCompare.setState({ modelTier: tier, modelOffset: 0 }); }
export function setCompareModelOffset(offset: number): void { useCompare.setState({ modelOffset: Math.max(0, offset) }); }

export function compareNavigation(state: CompareState) {
  if (state.manualTarget) return null;
  if (state.memberGroup && state.target) {
    const position = state.memberGroup.indexOf(state.target.id) + 1;
    const total = state.memberGroup.length;
    return { position, total, previous: position > 1, next: position > 0 && position < total, loading: false, error: null, group: "同组图片" };
  }
  const group = state.activeTab === "models"
    ? compareModelGroups(state.models, state.sample?.row).find(item => item.rows.some(row => row.id === state.target?.id))
    : undefined;
  const section = state.activeTab === "models" ? null : state[state.activeTab];
  const rows = group?.rows ?? section?.items ?? [];
  const index = rows.findIndex(row => row.id === state.target?.id);
  const position = index < 0 ? 0 : (section?.offset ?? 0) + index + 1;
  const total = group?.rows.length ?? section?.total ?? 0;
  return { position, total, previous: position > 1, next: position > 0 && position < total, loading: section ? section.loading : state.modelsLoading, error: section ? section.error : state.modelsError, group: group?.title };
}

/** Keep A fixed, advancing B within the current relation (and model group). */
export async function moveCompareTarget(direction: 1 | -1): Promise<void> {
  const state = useCompare.getState();
  if (!state.target || state.manualTarget) return;
  const navigation = compareNavigation(state);
  if (!navigation || navigation.loading || !(direction === 1 ? navigation.next : navigation.previous)) return;
  if (state.memberGroup) {
    const id = state.memberGroup[navigation.position - 1 + direction];
    try {
      const [target] = await getRowsByIds([id]);
      const latest = useCompare.getState();
      if (latest.memberGroup === state.memberGroup && latest.target === state.target) {
        if (target) useCompare.setState({ target });
        else notify("这张图片已移除，请返回刷新结果。", "error");
      }
    } catch (error) { notify(errorText(error), "error"); }
    return;
  }
  if (state.activeTab === "models") {
    const group = compareModelGroups(state.models, state.sample?.row).find(item => item.rows.some(row => row.id === state.target!.id));
    const index = (group?.rows.findIndex(row => row.id === state.target!.id) ?? -1) + direction;
    const target = group?.rows[index];
    if (target && group) useCompare.setState({ target, modelTier: group.tier, modelOffset: Math.floor(index / COMPARE_PAGE_SIZE) * COMPARE_PAGE_SIZE });
    return;
  }
  const sectionKey = state.activeTab;
  const before = state[sectionKey];
  const index = before.items.findIndex(row => row.id === state.target!.id);
  const adjacent = before.items[index + direction];
  if (adjacent) { useCompare.setState({ target: adjacent, [sectionKey]: { ...before, error: null } }); return; }
  const requestGeneration = generation;
  await loadCompareSectionPage(sectionKey, false, direction);
  const after = useCompare.getState();
  // A sample switch, back action, or another selection invalidates this navigation.
  if (generation !== requestGeneration || after.manualTarget || (sectionKey === "custom" && after.options !== state.options) || after.activeTab !== sectionKey || after.target?.id !== state.target.id) return;
  const page = after[sectionKey];
  if (page.error || page.offset === before.offset) return;
  const target = direction === 1 ? page.items[0] : page.items.at(-1);
  if (target) useCompare.setState({ target });
}
