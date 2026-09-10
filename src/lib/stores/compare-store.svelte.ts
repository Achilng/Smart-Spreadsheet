import type { CompareModelSection, CompareSample } from "../api/compare";
import {
  getCompareSample,
  queryCompareSameArtists,
  queryCompareSameStyleAllModels,
  queryCompareSameStyleDiffVibe,
  queryCompareSameVibeDiffStyle,
} from "../api/compare";
import type { RowRecord } from "../api";
import { createRequestQueue } from "../utils/request-queue";

/** 分区①②③共用的分页状态。 */
export interface SectionState {
  items: RowRecord[];
  total: number;
  loading: boolean;
  error: string | null;
  loaded: boolean;
  offset: number;
}

/** 窗口内两个视图：分区列表 ⇄ 并排对比。 */
export type CompareView = "sections" | "sideBySide";

function emptySection(): SectionState {
  return { items: [], total: 0, loading: false, error: null, loaded: false, offset: 0 };
}

/** 每个分区首页的行数；“加载更多”同样按页追加。 */
const PAGE_SIZE = 24;
let generation = 0;
const enqueue = createRequestQueue();
export type CompareTab = CompareSectionKey | "models";

export const compareStore = $state({
  /** 当前样本行 id；null 表示尚未选择样本。 */
  rowId: null as number | null,
  sample: null as CompareSample | null,
  sampleLoading: false,
  /** 样本拉取失败（通常是打开期间被删除）。 */
  sampleError: null as string | null,
  activeTab: "artists" as CompareTab,

  artists: emptySection(),
  vibeDiffStyle: emptySection(),
  styleDiffVibe: emptySection(),
  models: { rows: [], totalCount: 0, truncated: false } as CompareModelSection,
  modelsLoading: false,
  modelsError: null as string | null,
  modelsLoaded: false,

  view: "sections" as CompareView,
  /** 并排对比的目标行。 */
  target: null as RowRecord | null,
});

function resetSections(): void {
  compareStore.artists = emptySection();
  compareStore.vibeDiffStyle = emptySection();
  compareStore.styleDiffVibe = emptySection();
  compareStore.models = { rows: [], totalCount: 0, truncated: false };
  compareStore.modelsLoading = false;
  compareStore.modelsError = null;
  compareStore.modelsLoaded = false;
}

export async function setCompareSample(rowId: number): Promise<void> {
  if (compareStore.rowId === rowId && compareStore.sample) {
    return;
  }
  compareStore.rowId = rowId;
  const requestGeneration = ++generation;
  compareStore.sample = null;
  compareStore.sampleError = null;
  compareStore.sampleLoading = true;
  compareStore.view = "sections";
  compareStore.target = null;
  resetSections();
  try {
    const sample = await enqueue(() => requestGeneration === generation, () => getCompareSample(rowId));
    if (!sample || requestGeneration !== generation) return;
    compareStore.sample = sample;
    void selectCompareTab(compareStore.activeTab);
  } catch (error) {
    if (requestGeneration === generation) compareStore.sampleError = errorText(error);
  } finally {
    if (requestGeneration === generation) compareStore.sampleLoading = false;
  }
}

/** 样本卡上的手动刷新：重新拉取样本与全部分区。 */
export async function refreshCompare(): Promise<void> {
  const rowId = compareStore.rowId;
  if (rowId == null) return;
  compareStore.rowId = null; // 绕过 setCompareSample 的同一样本短路
  await setCompareSample(rowId);
}

export type CompareSectionKey = "artists" | "vibeDiffStyle" | "styleDiffVibe";

export async function selectCompareTab(tab: CompareTab): Promise<void> {
  compareStore.activeTab = tab;
  if (!compareStore.sample) return;
  if (tab === "models") {
    if (!compareStore.modelsLoaded) await loadCompareModels();
  } else if (!compareStore[tab].loaded) {
    await loadCompareSectionPage(tab, true);
  }
}

export async function loadCompareSectionPage(
  section: CompareSectionKey,
  first: boolean,
  direction: 1 | -1 = 1,
): Promise<void> {
  const rowId = compareStore.rowId;
  if (rowId == null) return;
  const state = compareStore[section];
  if (state.loading || (!first && direction === 1 && state.offset + state.items.length >= state.total)) {
    return;
  }
  const offset = first ? 0 : Math.max(0, state.offset + direction * PAGE_SIZE);
  state.loading = true;
  state.error = null;
  const requestGeneration = generation;
  try {
    const query =
      section === "artists"
        ? queryCompareSameArtists
        : section === "vibeDiffStyle"
          ? queryCompareSameVibeDiffStyle
          : queryCompareSameStyleDiffVibe;
    const page = await enqueue(
      () => requestGeneration === generation && compareStore.activeTab === section,
      () => query(rowId, offset, PAGE_SIZE),
    );
    if (!page || requestGeneration !== generation) return;
    state.items = page.rows;
    state.offset = page.offset;
    state.total = page.totalCount;
    state.loaded = true;
  } catch (error) {
    if (requestGeneration === generation) {
      state.error = errorText(error);
    }
  } finally {
    if (requestGeneration === generation) {
      state.loading = false;
    }
  }
}

export async function loadCompareModels(): Promise<void> {
  const rowId = compareStore.rowId;
  if (rowId == null || compareStore.modelsLoading) return;
  const requestGeneration = generation;
  compareStore.modelsLoading = true;
  compareStore.modelsError = null;
  try {
    const section = await enqueue(
      () => requestGeneration === generation && compareStore.activeTab === "models",
      () => queryCompareSameStyleAllModels(rowId),
    );
    if (!section || requestGeneration !== generation) return;
    compareStore.models = section;
    compareStore.modelsLoaded = true;
  } catch (error) {
    if (requestGeneration === generation) {
      compareStore.modelsError = errorText(error);
    }
  } finally {
    if (requestGeneration === generation) {
      compareStore.modelsLoading = false;
    }
  }
}

export function openSideBySide(target: RowRecord): void {
  compareStore.target = target;
  compareStore.view = "sideBySide";
}

export function closeSideBySide(): void {
  compareStore.view = "sections";
  compareStore.target = null;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
