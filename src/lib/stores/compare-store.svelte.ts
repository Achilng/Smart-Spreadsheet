import type { CompareModelSection, CompareSample } from "../api/compare";
import {
  getCompareSample,
  queryCompareSameArtists,
  queryCompareSameStyleAllModels,
  queryCompareSameStyleDiffVibe,
  queryCompareSameVibeDiffStyle,
} from "../api/compare";
import type { RowRecord } from "../api";

/** 分区①②③共用的分页状态。 */
export interface SectionState {
  items: RowRecord[];
  total: number;
  loading: boolean;
  error: string | null;
}

/** 窗口内两个视图：分区列表 ⇄ 并排对比。 */
export type CompareView = "sections" | "sideBySide";

function emptySection(): SectionState {
  return { items: [], total: 0, loading: false, error: null };
}

/** 每个分区首页的行数；“加载更多”同样按页追加。 */
const PAGE_SIZE = 24;

export const compareStore = $state({
  /** 当前样本行 id；null 表示尚未选择样本。 */
  rowId: null as number | null,
  sample: null as CompareSample | null,
  sampleLoading: false,
  /** 样本拉取失败（通常是打开期间被删除）。 */
  sampleError: null as string | null,

  artists: emptySection(),
  vibeDiffStyle: emptySection(),
  styleDiffVibe: emptySection(),
  models: { rows: [], totalCount: 0, truncated: false } as CompareModelSection,
  modelsLoading: false,
  modelsError: null as string | null,

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
}

export async function setCompareSample(rowId: number): Promise<void> {
  if (compareStore.rowId === rowId && compareStore.sample) {
    return;
  }
  compareStore.rowId = rowId;
  compareStore.sample = null;
  compareStore.sampleError = null;
  compareStore.sampleLoading = true;
  compareStore.view = "sections";
  compareStore.target = null;
  resetSections();
  try {
    compareStore.sample = await getCompareSample(rowId);
    // 分区各拉首页；失败只落在各自分区，不影响样本卡。
    void loadCompareSectionPage("artists", true);
    void loadCompareSectionPage("vibeDiffStyle", true);
    void loadCompareSectionPage("styleDiffVibe", true);
    void loadCompareModels();
  } catch (error) {
    compareStore.sampleError = errorText(error);
  } finally {
    compareStore.sampleLoading = false;
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

export async function loadCompareSectionPage(
  section: CompareSectionKey,
  first: boolean,
): Promise<void> {
  const rowId = compareStore.rowId;
  if (rowId == null) return;
  const state = compareStore[section];
  if (!first && (state.loading || state.items.length >= state.total)) {
    return;
  }
  const offset = first ? 0 : state.items.length;
  state.loading = true;
  state.error = null;
  try {
    const query =
      section === "artists"
        ? queryCompareSameArtists
        : section === "vibeDiffStyle"
          ? queryCompareSameVibeDiffStyle
          : queryCompareSameStyleDiffVibe;
    const page = await query(rowId, offset, PAGE_SIZE);
    if (compareStore.rowId !== rowId) return; // 期间切换了样本，丢弃过期结果
    state.items = first ? page.rows : [...state.items, ...page.rows];
    state.total = page.totalCount;
  } catch (error) {
    if (compareStore.rowId === rowId) {
      state.error = errorText(error);
    }
  } finally {
    if (compareStore.rowId === rowId) {
      state.loading = false;
    }
  }
}

export async function loadCompareModels(): Promise<void> {
  const rowId = compareStore.rowId;
  if (rowId == null) return;
  compareStore.modelsLoading = true;
  compareStore.modelsError = null;
  try {
    const section = await queryCompareSameStyleAllModels(rowId);
    if (compareStore.rowId !== rowId) return;
    compareStore.models = section;
  } catch (error) {
    if (compareStore.rowId === rowId) {
      compareStore.modelsError = errorText(error);
    }
  } finally {
    if (compareStore.rowId === rowId) {
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
