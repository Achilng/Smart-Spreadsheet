import { queryRows, type DedupeMode, type LibraryFilter, type RowRecord, type SortMode, type TagMatchMode } from "../api";
import { errorText } from "./app-state.svelte";
import { clearScrollPositions } from "./view-state";
import { cloneLibraryFilters } from "../utils/library-filters";

export const PAGE_SIZE = 200;

/**
 * 行数据的分页缓存。页内容保存在非响应式 Map 中，
 * 通过 pagesVersion 计数器通知视图重算可见区域。
 *
 * 筛选/搜索变化采用 stale-while-revalidate：旧内容保持渲染
 * （refreshing = true），新结果首页到达后原子替换，避免白屏闪烁。
 */
export const rowStore = $state({
  tags: [] as string[],
  tagMode: "and" as TagMatchMode,
  dedupe: "none" as DedupeMode,
  singleArtistOnly: false,
  /** 画师串精确筛选；空字符串表示不限制。 */
  artistFilter: "",
  hasVibe: false,
  untaggedOnly: false,
  /** Discord 风格筛选面板应用的结构化条件；条件之间固定为 AND。 */
  filters: [] as LibraryFilter[],
  groupView: false,
  hideGrouped: false,
  search: "",
  sort: "timeAsc" as SortMode,
  totalCount: 0,
  /** 首次加载 / 数据集整体更换，期间没有可显示的旧内容 */
  initialLoading: true,
  /** 新筛选结果在途，旧内容仍在渲染 */
  refreshing: false,
  error: null as string | null,
  pagesVersion: 0,
  /** 结果集语义变化（筛选/搜索/数据变更）完成时 +1，视图据此回到顶部 */
  resetToken: 0,
  activeRow: null as RowRecord | null,
  /** 工具箱等外部入口请求画廊定位的真实行序号。 */
  revealIndex: null as number | null,
  revealToken: 0,
});

let pages = new Map<number, RowRecord[]>();
/** 刷新中的新一代页缓存；首页到达后整体替换 pages */
let incoming: Map<number, RowRecord[]> | null = null;
let pendingPages = new Set<number>();
let generation = 0;
/** 本轮刷新完成时是否要求视图回到顶部 */
let resetScrollOnSwap = true;
/** 最近更新排序下的单行编辑刷新期间保留详情面板，若刷新后仍命中则继续显示。 */
let keepActiveOnSwap = false;

export function getRow(index: number): RowRecord | undefined {
  return pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE];
}

export function ensurePage(pageIndex: number): void {
  if (pageIndex < 0 || pendingPages.has(pageIndex)) {
    return;
  }
  if ((incoming ?? pages).has(pageIndex)) {
    return;
  }
  pendingPages.add(pageIndex);
  const requestGeneration = generation;
  void (async () => {
    try {
      const page = await queryRows({
        offset: pageIndex * PAGE_SIZE,
        limit: PAGE_SIZE,
        tags: [...rowStore.tags],
        tagMode: rowStore.tagMode,
        dedupe: rowStore.dedupe,
        singleArtistOnly: rowStore.singleArtistOnly,
        artistFilter: rowStore.artistFilter,
        hasVibe: rowStore.hasVibe,
        untaggedOnly: rowStore.untaggedOnly,
        filters: cloneLibraryFilters(rowStore.filters),
        groupView: rowStore.groupView,
        hideGrouped: rowStore.hideGrouped,
        search: rowStore.search,
        sort: rowStore.sort,
      });
      if (requestGeneration !== generation) {
        return;
      }
      if (incoming) {
        incoming.set(pageIndex, page.rows);
        if (
          keepActiveOnSwap &&
          rowStore.activeRow &&
          !page.rows.some(row => row.id === rowStore.activeRow?.id)
        ) {
          rowStore.activeRow = null;
        }
        keepActiveOnSwap = false;
        // 新结果首页到达：原子替换旧内容
        pages = incoming;
        incoming = null;
        rowStore.refreshing = false;
        if (resetScrollOnSwap) {
          rowStore.resetToken += 1;
        }
      } else {
        pages.set(pageIndex, page.rows);
      }
      rowStore.totalCount = page.totalCount;
      rowStore.initialLoading = false;
      rowStore.error = null;
      rowStore.pagesVersion += 1;
    } catch (error) {
      if (requestGeneration !== generation) {
        return;
      }
      rowStore.initialLoading = false;
      rowStore.refreshing = false;
      rowStore.error = errorText(error);
    } finally {
      if (requestGeneration === generation) {
        pendingPages.delete(pageIndex);
      }
    }
  })();
}

interface ResetOptions {
  /** false = 数据集整体更换（无可信旧内容），true = 保留旧内容直到新结果到达 */
  keepStale?: boolean;
  /** true = 筛选语义变化，各视图清位置回顶部；false = 就地刷新保留位置 */
  resetScroll?: boolean;
  /** 刷新后目标仍在首页时保留当前详情。仅用于最近更新排序下的单行编辑。 */
  keepActive?: boolean;
}

/**
 * 清空缓存并重新加载第一页。
 * 默认（无参）= 批量操作后的就地刷新：保留旧内容与滚动位置，新结果到达后原位替换。
 * 筛选/搜索变化传 resetScroll: true；导入/删除/换库传 keepStale: false。
 */
export function resetRows(options: ResetOptions = {}): void {
  const { keepStale = true, resetScroll = false, keepActive = false } = options;
  generation += 1;
  pendingPages = new Set();
  rowStore.error = null;
  keepActiveOnSwap = keepActive;
  if (!keepActive) {
    rowStore.activeRow = null;
  }
  resetScrollOnSwap = resetScroll;
  if (resetScroll) {
    clearScrollPositions();
  }
  if (keepStale && pages.size > 0) {
    incoming = new Map();
    rowStore.refreshing = true;
  } else {
    pages = new Map();
    incoming = null;
    rowStore.totalCount = 0;
    rowStore.initialLoading = true;
    rowStore.refreshing = false;
    if (resetScroll) {
      rowStore.resetToken += 1;
    }
    rowStore.pagesVersion += 1;
  }
  ensurePage(0);
}

export function setFilter(tags: string[], tagMode: TagMatchMode): void {
  rowStore.tags = [...tags];
  rowStore.tagMode = tagMode;
  if (tags.length > 0) {
    rowStore.untaggedOnly = false;
  }
  resetRows({ keepStale: true, resetScroll: true });
}

export function setDedupe(dedupe: DedupeMode): void {
  if (rowStore.dedupe !== dedupe) {
    rowStore.dedupe = dedupe;
    resetRows({ keepStale: true, resetScroll: true });
  }
}

export function setSingleArtistOnly(value: boolean): void {
  if (rowStore.singleArtistOnly !== value) {
    rowStore.singleArtistOnly = value;
    resetRows({ keepStale: true, resetScroll: true });
  }
}

/** 只浏览画师串与给定值完全相同的图片，并清除会继续收窄结果的其它筛选。 */
export function focusArtistFilter(artists: string): void {
  const normalized = artists.trim();
  if (!normalized) return;
  rowStore.tags = [];
  rowStore.dedupe = "none";
  rowStore.singleArtistOnly = false;
  rowStore.artistFilter = normalized;
  rowStore.hasVibe = false;
  rowStore.untaggedOnly = false;
  rowStore.filters = [];
  rowStore.groupView = false;
  rowStore.hideGrouped = false;
  rowStore.search = "";
  resetRows({ keepStale: true, resetScroll: true });
}

export function setArtistFilter(value: string): void {
  const normalized = value.trim();
  if (rowStore.artistFilter !== normalized) {
    rowStore.artistFilter = normalized;
    resetRows({ keepStale: true, resetScroll: true });
  }
}

export function setHasVibe(value: boolean): void {
  if (rowStore.hasVibe !== value) {
    rowStore.hasVibe = value;
    resetRows({ keepStale: true, resetScroll: true });
  }
}

export function setUntaggedOnly(value: boolean): void {
  if (rowStore.untaggedOnly !== value) {
    rowStore.untaggedOnly = value;
    if (value) {
      rowStore.tags = [];
    }
    resetRows({ keepStale: true, resetScroll: true });
  }
}

export function setLibraryFilters(filters: LibraryFilter[]): void {
  const next = cloneLibraryFilters(filters);
  if (JSON.stringify(rowStore.filters) !== JSON.stringify(next)) {
    rowStore.filters = next;
    resetRows({ keepStale: true, resetScroll: true });
  }
}

export function removeLibraryFilter(index: number): void {
  if (index < 0 || index >= rowStore.filters.length) return;
  setLibraryFilters(rowStore.filters.filter((_, filterIndex) => filterIndex !== index));
}

export function setGroupView(value: boolean): void {
  if (rowStore.groupView !== value) {
    rowStore.groupView = value;
    if (value) {
      rowStore.dedupe = "none";
      rowStore.hideGrouped = false;
    }
    // 视图切换不是筛选变化：保留画廊/表格的滚动位置
    resetRows({ keepStale: true, resetScroll: false });
  }
}

export function setHideGrouped(value: boolean): void {
  if (rowStore.hideGrouped !== value) {
    rowStore.hideGrouped = value;
    resetRows({ keepStale: true, resetScroll: true });
  }
}

export function setSearch(value: string): void {
  if (rowStore.search !== value) {
    rowStore.search = value;
    resetRows({ keepStale: true, resetScroll: true });
  }
}

/** 一次性清空全部筛选条件（Tag/开关/搜索），只触发一次刷新；不动 tagMode 与排序。 */
export function clearAllFilters(): void {
  const dirty =
    rowStore.tags.length > 0 ||
    rowStore.dedupe !== "none" ||
    rowStore.singleArtistOnly ||
    rowStore.artistFilter !== "" ||
    rowStore.hasVibe ||
    rowStore.untaggedOnly ||
    rowStore.filters.length > 0 ||
    rowStore.hideGrouped ||
    rowStore.search !== "";
  if (!dirty) return;
  rowStore.tags = [];
  rowStore.dedupe = "none";
  rowStore.singleArtistOnly = false;
  rowStore.artistFilter = "";
  rowStore.hasVibe = false;
  rowStore.untaggedOnly = false;
  rowStore.filters = [];
  rowStore.hideGrouped = false;
  rowStore.search = "";
  resetRows({ keepStale: true, resetScroll: true });
}

export function setSort(sort: SortMode): void {
  if (rowStore.sort !== sort) {
    rowStore.sort = sort;
    resetRows({ keepStale: true, resetScroll: true });
  }
}

/** 清除可能隐藏目标行的筛选，并请求画廊在数据就绪后滚动到指定图片。 */
export function revealRowInGallery(row: RowRecord, index: number): void {
  rowStore.tags = [];
  rowStore.tagMode = "and";
  rowStore.dedupe = "none";
  rowStore.singleArtistOnly = false;
  rowStore.artistFilter = "";
  rowStore.hasVibe = false;
  rowStore.untaggedOnly = false;
  rowStore.filters = [];
  rowStore.groupView = false;
  rowStore.hideGrouped = false;
  rowStore.search = "";
  resetRows({ keepStale: true, resetScroll: true });
  rowStore.activeRow = row;
  rowStore.revealIndex = index;
  rowStore.revealToken += 1;
}

/** 单行字段编辑后原位更新缓存，避免整表重载丢失滚动位置和活动行。 */
export function patchRowFields(rowId: number, fields: Partial<RowRecord>): void {
  for (const pageRows of pages.values()) {
    const row = pageRows.find(candidate => candidate.id === rowId);
    if (row) {
      Object.assign(row, fields);
      break;
    }
  }
  if (rowStore.activeRow?.id === rowId) {
    Object.assign(rowStore.activeRow, fields);
  }
  rowStore.pagesVersion += 1;
  if (rowStore.sort === "recentlyUpdated") {
    resetRows({ keepStale: true, resetScroll: true, keepActive: true });
  }
}

/** 单行 Tag 编辑成功后原位更新缓存，避免整表重载丢失滚动位置。 */
export function patchRowTags(rowId: number, tags: string[]): void {
  for (const pageRows of pages.values()) {
    const row = pageRows.find(candidate => candidate.id === rowId);
    if (row) {
      row.tags = [...tags];
      break;
    }
  }
  if (rowStore.activeRow?.id === rowId) {
    // 原位修改而不是替换对象：详情面板依赖对象引用判断是否需要重载预览图
    rowStore.activeRow.tags = [...tags];
  }
  rowStore.pagesVersion += 1;
  if (rowStore.untaggedOnly && tags.length > 0) {
    resetRows({ keepStale: true, resetScroll: false });
  } else if (rowStore.sort === "recentlyUpdated") {
    resetRows({ keepStale: true, resetScroll: true, keepActive: true });
  }
}
