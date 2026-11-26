import { create } from "zustand";
import { getRowsByIds, type RowRecord } from "../../lib/api";
import { compareFields, customPage, queryCompareCustom, startingConditions, type CompareField, type CompareOptions, type CompareGroups } from "../../lib/api/compare-conditions";
import type { CompareWindowView } from "../../lib/windows/compare";
import { notify } from "./notices";
import { getCompareSample, queryCompareSameArtists, queryCompareSameVibeDiffStyle, queryCompareSameStyleDiffVibe, queryCompareSameStyleAllModels, type CompareSample, type CompareModelSection } from "../../lib/api/compare";
import { createRequestQueue } from "../../lib/utils/request-queue";
import { modelComparisonTier, modelVersionBadge } from "../../lib/utils/model-version";

export type CompareSectionKey = "artists" | "vibeDiffStyle" | "styleDiffVibe" | "custom";
export type CompareTab = CompareSectionKey | "models";
/** Results load in batches as the list scrolls; `items` is everything loaded so far. */
export interface SectionState { items: RowRecord[]; total: number; loading: boolean; error: string | null; loaded: boolean }
const emptySection = (): SectionState => ({ items: [], total: 0, loading: false, error: null, loaded: false });
const sections = () => ({
  groups: null as CompareGroups | null, expandedGroup: null as number[] | null, memberRows: null as RowRecord[] | null,
  custom: emptySection(), artists: emptySection(), vibeDiffStyle: emptySection(), styleDiffVibe: emptySection(),
  models: { rows: [], totalCount: 0, truncated: false } as CompareModelSection, modelsLoading: false, modelsError: null as string | null, modelsLoaded: false, manualTarget: false,
});
interface CompareState {
  groups: CompareGroups | null;
  /** Members of the deduplicated group shown in the group dialog. */
  expandedGroup: number[] | null;
  /** Set when B was opened from an expanded group; B then moves within that group. */
  memberRows: RowRecord[] | null;
  custom: SectionState; options: CompareOptions; manualTarget: boolean; rowId: number | null; sample: CompareSample | null; sampleLoading: boolean; sampleError: string | null; activeTab: CompareTab;
  artists: SectionState; vibeDiffStyle: SectionState; styleDiffVibe: SectionState; models: CompareModelSection; modelsLoading: boolean; modelsError: string | null; modelsLoaded: boolean; target: RowRecord | null;
}
export const useCompare = create<CompareState>(() => ({ options: startingConditions("blank"), rowId: null, sample: null, sampleLoading: false, sampleError: null, activeTab: "artists", target: null, ...sections() }));
const enqueue = createRequestQueue();
const BATCH_SIZE = 60;
const OPTIONS_DEBOUNCE_MS = 160;
let generation = 0;
const sectionRequests: Record<CompareSectionKey, number> = { artists: 0, vibeDiffStyle: 0, styleDiffVibe: 0, custom: 0 };
let optionsTimer: ReturnType<typeof setTimeout> | undefined;
const errorText = (error: unknown) => error instanceof Error ? error.message : String(error);

export function resetCompare(): void {
  generation++;
  clearTimeout(optionsTimer);
  useCompare.setState({ options: startingConditions("blank"), activeTab: "artists", rowId: null, sample: null, sampleError: null, sampleLoading: false, target: null, ...sections() });
}

export async function setCompareSample(rowId: number, force = false, targetId?: number, view?: CompareWindowView): Promise<void> {
  if (!Number.isInteger(rowId) || rowId <= 0) return;
  const state = useCompare.getState();
  if (!force && state.rowId === rowId && (state.sample || state.sampleLoading)) return;
  const requestGeneration = ++generation;
  clearTimeout(optionsTimer);
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
    if (view?.custom) {
      const allowed = view.custom.allowed ?? (Object.keys(compareFields) as CompareField[]);
      useCompare.setState({ options: { observe: view.custom.observe, dedupe: view.custom.dedupe === true, allowed: allowed.filter(field => field !== view.custom!.observe) }, groups: null, activeTab: "custom" });
    } else if (view?.tab) useCompare.setState({ activeTab: view.tab });
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
  useCompare.setState({ activeTab: tab, expandedGroup: null });
  const state = useCompare.getState();
  if (!state.sample) return;
  if (tab === "models") { if (!state.modelsLoaded) await loadCompareModels(); }
  else if (!state[tab].loaded) await loadCompareSection(tab, true);
}

/** Changes the custom comparison; results follow shortly after, keeping the old ones (dimmed) until then. */
export function setCompareOptions(options: CompareOptions): void {
  clearTimeout(optionsTimer);
  useCompare.setState(state => ({ options, groups: null, expandedGroup: null, activeTab: "custom", target: null, manualTarget: false, memberRows: null, custom: { ...state.custom, loading: true, error: null } }));
  optionsTimer = setTimeout(() => void loadCompareSection("custom", true), OPTIONS_DEBOUNCE_MS);
}

/** Loads the first batch (`reset`) or the next one after what is already loaded. */
export async function loadCompareSection(section: CompareSectionKey, reset = false): Promise<void> {
  const state = useCompare.getState();
  if (state.rowId === null || !state.sample) return;
  const before = state[section];
  if (!reset && (before.loading || before.error || !before.loaded || before.items.length >= before.total)) return;
  const offset = reset ? 0 : before.items.length;
  const request = ++sectionRequests[section];
  const requestGeneration = generation;
  const current = () => generation === requestGeneration && sectionRequests[section] === request && (section !== "custom" || useCompare.getState().options === state.options);
  const update = (patch: (section: SectionState) => Partial<SectionState>) => {
    if (current()) useCompare.setState(latest => ({ [section]: { ...latest[section], ...patch(latest[section]) } }));
  };
  update(() => ({ loading: true, error: null }));
  try {
    const query = section === "artists" ? queryCompareSameArtists : section === "vibeDiffStyle" ? queryCompareSameVibeDiffStyle : queryCompareSameStyleDiffVibe;
    const page = await enqueue(current, async () => {
      if (section !== "custom") return query(state.rowId!, offset, BATCH_SIZE);
      const groups = state.groups ?? await queryCompareCustom(state.sample!, state.options);
      if (!current()) return;
      useCompare.setState({ groups });
      return customPage(groups, offset, BATCH_SIZE);
    });
    if (page) update(latest => ({ items: reset ? page.rows : [...latest.items, ...page.rows], total: page.totalCount, loaded: true }));
  } catch (error) { update(() => ({ error: errorText(error) })); }
  finally { update(() => ({ loading: false })); }
}
/** Retries a failed batch, or continues after the last one. */
export function retryCompareSection(section: CompareSectionKey): void {
  const state = useCompare.getState()[section];
  if (!state.loaded) { void loadCompareSection(section, true); return; }
  useCompare.setState(latest => ({ [section]: { ...latest[section], error: null } }));
  void loadCompareSection(section);
}
export async function loadCompareModels(): Promise<void> {
  const state = useCompare.getState();
  if (state.rowId === null || state.modelsLoading) return;
  const requestGeneration = generation;
  useCompare.setState({ modelsLoading: true, modelsError: null });
  try {
    const models = await enqueue(() => generation === requestGeneration, () => queryCompareSameStyleAllModels(state.rowId!));
    if (models && generation === requestGeneration) useCompare.setState({ models, modelsLoaded: true });
  } catch (error) {
    if (generation === requestGeneration) useCompare.setState({ modelsError: errorText(error) });
  } finally { if (generation === requestGeneration) useCompare.setState({ modelsLoading: false }); }
}
export function openSideBySide(target: RowRecord, manualTarget = false, memberRows: RowRecord[] | null = null): void {
  // Opened from outside the group dialog, B leaves that group, so the dialog stays closed on return.
  useCompare.setState(state => ({ target, manualTarget, memberRows, expandedGroup: memberRows ? state.expandedGroup : null }));
}
export function closeSideBySide(): void { useCompare.setState({ target: null, memberRows: null }); }
export function pickCompareTarget(target: RowRecord): void { useCompare.setState({ target }); }
export function setExpandedGroup(group: number[] | null): void { useCompare.setState({ expandedGroup: group }); }

export interface CompareModelGroup { tier: string; title: string; rank: number; rows: RowRecord[] }
let modelGroupsCache: { models: CompareModelSection; sample: RowRecord | undefined; groups: CompareModelGroup[] } | null = null;
export function compareModelGroups(models: CompareModelSection, sample: RowRecord | undefined): CompareModelGroup[] {
  if (modelGroupsCache?.models === models && modelGroupsCache.sample === sample) return modelGroupsCache.groups;
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
  const sorted = [...groups.values()].sort((a, b) => b.rank - a.rank || a.title.localeCompare(b.title));
  modelGroupsCache = { models, sample, groups: sorted };
  return sorted;
}

/** The results B moves through: the expanded group, all model groups in order, or the current relation. */
export interface CompareList { rows: RowRecord[]; total: number; more: boolean; loading: boolean; error: string | null; section: CompareSectionKey | null }
export function compareList(state: CompareState): CompareList | null {
  if (state.manualTarget) return null;
  if (state.memberRows) return { rows: state.memberRows, total: state.memberRows.length, more: false, loading: false, error: null, section: null };
  if (state.activeTab === "models") {
    const rows = compareModelGroups(state.models, state.sample?.row).flatMap(group => group.rows);
    return { rows, total: rows.length, more: false, loading: state.modelsLoading, error: state.modelsError, section: null };
  }
  const section = state[state.activeTab];
  return { rows: section.items, total: section.total, more: section.items.length < section.total, loading: section.loading, error: section.error, section: state.activeTab };
}

export function compareNavigation(state: CompareState) {
  const list = compareList(state);
  if (!list) return null;
  const index = list.rows.findIndex(row => row.id === state.target?.id);
  const position = index + 1;
  const group = state.memberRows ? "同组图片"
    : state.activeTab === "models" ? compareModelGroups(state.models, state.sample?.row).find(item => item.rows.some(row => row.id === state.target?.id))?.title
      : undefined;
  // Only waiting matters when B sits on the last loaded result; elsewhere loading runs in the background.
  return { position, total: list.total, previous: position > 1, next: position > 0 && (position < list.rows.length || list.more), loading: list.loading && position >= list.rows.length, error: list.error, group };
}

/** Keep A fixed, advancing B through the current list; loads the next batch at the end of what is loaded. */
export async function moveCompareTarget(direction: 1 | -1): Promise<void> {
  const state = useCompare.getState();
  if (!state.target) return;
  const list = compareList(state);
  if (!list) return;
  const index = list.rows.findIndex(row => row.id === state.target!.id);
  if (index < 0) return;
  const adjacent = list.rows[index + direction];
  if (adjacent) { useCompare.setState({ target: adjacent }); return; }
  if (direction !== 1 || !list.more || !list.section || list.loading) return;
  const section = list.section;
  if (list.error) useCompare.setState(latest => ({ [section]: { ...latest[section], error: null } }));
  await loadCompareSection(section);
  const after = useCompare.getState();
  // A sample switch, back action, or another selection invalidates this navigation.
  if (after.target?.id !== state.target.id || after.activeTab !== section || after.memberRows) return;
  const next = after[section].items[index + 1];
  if (next) useCompare.setState({ target: next });
}
/** Loads more of the list B moves through, when it has more. */
export function loadMoreCompareList(): void {
  const list = compareList(useCompare.getState());
  if (list?.section && list.more) void loadCompareSection(list.section);
}
