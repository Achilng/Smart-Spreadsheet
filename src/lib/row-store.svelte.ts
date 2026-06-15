import { queryRows, type DedupeMode, type RowRecord, type TagMatchMode } from "../api";
import { errorText } from "./app-state.svelte";

export const PAGE_SIZE = 200;

/**
 * 行数据的分页缓存。页内容保存在非响应式 Map 中，
 * 通过 pagesVersion 计数器通知视图重算可见区域。
 */
export const rowStore = $state({
  tags: [] as string[],
  tagMode: "and" as TagMatchMode,
  dedupe: "none" as DedupeMode,
  singleArtistOnly: false,
  groupView: false,
  hideGrouped: false,
  totalCount: 0,
  initialLoading: true,
  error: null as string | null,
  pagesVersion: 0,
  activeRow: null as RowRecord | null,
});

let pages = new Map<number, RowRecord[]>();
let pendingPages = new Set<number>();
let generation = 0;

export function getRow(index: number): RowRecord | undefined {
  return pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE];
}

export function ensurePage(pageIndex: number): void {
  if (pageIndex < 0 || pages.has(pageIndex) || pendingPages.has(pageIndex)) {
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
        groupView: rowStore.groupView,
        hideGrouped: rowStore.hideGrouped,
      });
      if (requestGeneration !== generation) {
        return;
      }
      pages.set(pageIndex, page.rows);
      rowStore.totalCount = page.totalCount;
      rowStore.initialLoading = false;
      rowStore.error = null;
      rowStore.pagesVersion += 1;
    } catch (error) {
      if (requestGeneration !== generation) {
        return;
      }
      rowStore.initialLoading = false;
      rowStore.error = errorText(error);
    } finally {
      if (requestGeneration === generation) {
        pendingPages.delete(pageIndex);
      }
    }
  })();
}

/** 清空缓存并重新加载第一页。筛选变化、批量操作和工作簿替换后调用。 */
export function resetRows(): void {
  generation += 1;
  pages = new Map();
  pendingPages = new Set();
  rowStore.totalCount = 0;
  rowStore.initialLoading = true;
  rowStore.error = null;
  rowStore.activeRow = null;
  rowStore.pagesVersion += 1;
  ensurePage(0);
}

export function setFilter(tags: string[], tagMode: TagMatchMode): void {
  rowStore.tags = [...tags];
  rowStore.tagMode = tagMode;
  resetRows();
}

export function setDedupe(dedupe: DedupeMode): void {
  if (rowStore.dedupe !== dedupe) {
    rowStore.dedupe = dedupe;
    resetRows();
  }
}

export function setSingleArtistOnly(value: boolean): void {
  if (rowStore.singleArtistOnly !== value) {
    rowStore.singleArtistOnly = value;
    resetRows();
  }
}

export function setGroupView(value: boolean): void {
  if (rowStore.groupView !== value) {
    rowStore.groupView = value;
    if (value) {
      rowStore.dedupe = "none";
      rowStore.hideGrouped = false;
    }
    resetRows();
  }
}

export function setHideGrouped(value: boolean): void {
  if (rowStore.hideGrouped !== value) {
    rowStore.hideGrouped = value;
    resetRows();
  }
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
}
