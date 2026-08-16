import {
  getDedupeClusterMembers,
  getGroupMembers,
  queryRows,
  type RowPage,
} from "../api";
import { app, errorText, setNotice, type ViewMode } from "./app-state.svelte";
import { duplicateBrowse } from "./duplicate-browse-store.svelte";
import { groupBrowse } from "./group-browse-store.svelte";
import { groupStore } from "./group-store.svelte";
import { rowStore } from "./row-store.svelte";
import {
  getSelectedCount,
  selectAllFiltered,
  setExplicitSelection,
} from "./selection-store.svelte";
import { cloneLibraryFilters } from "../utils/library-filters";

const SELECT_PAGE_SIZE = 500;

export const viewSelection = $state({
  lastScopeSignature: "",
  lastTotal: 0,
});

function groupScopeSignature(): string {
  return [
    "group",
    String(app.dataVersion),
    String(groupStore.membershipVersion),
    rowStore.tags.join("\u{2}"),
    rowStore.tagMode,
    String(rowStore.singleArtistOnly),
    rowStore.artistFilter,
    String(rowStore.hasVibe),
    String(rowStore.untaggedOnly),
    JSON.stringify(rowStore.filters),
    rowStore.search,
  ].join("\u{1}");
}

function duplicateScopeSignature(): string {
  return [
    "duplicates",
    String(app.dataVersion),
    duplicateBrowse.dedupeMode,
    rowStore.tags.join("\u{2}"),
    rowStore.tagMode,
    String(rowStore.singleArtistOnly),
    String(rowStore.hasVibe),
    String(rowStore.untaggedOnly),
    JSON.stringify(rowStore.filters),
    String(rowStore.hideGrouped),
  ].join("\u{1}");
}

function currentScopeSignature(mode: ViewMode = app.viewMode): string {
  if (mode === "group") return groupScopeSignature();
  if (mode === "duplicates") return duplicateScopeSignature();
  return [
    mode,
    String(rowStore.resetToken),
    String(rowStore.totalCount),
  ].join("\u{1}");
}

async function appendRemainingRows(
  ids: Set<number>,
  loadedRows: readonly { id: number }[],
  totalCount: number,
  fetchPage: (offset: number, limit: number) => Promise<RowPage>,
): Promise<void> {
  for (const row of loadedRows) {
    ids.add(row.id);
  }
  for (let offset = loadedRows.length; offset < totalCount; offset += SELECT_PAGE_SIZE) {
    const page = await fetchPage(offset, SELECT_PAGE_SIZE);
    for (const row of page.rows) {
      ids.add(row.id);
    }
    if (page.rows.length === 0) {
      break;
    }
  }
}

async function groupViewRowIds(): Promise<number[]> {
  const ids = new Set<number>();
  const groups = [...groupStore.list];
  for (const group of groups) {
    const loaded = groupBrowse.memberCache[group.id]?.rows ?? [];
    await appendRemainingRows(
      ids,
      loaded,
      group.memberCount,
      (offset, limit) => getGroupMembers(group.id, offset, limit),
    );
  }

  const loadedUngrouped = groupBrowse.ungrouped?.rows ?? [];
  let ungroupedTotal = groupBrowse.ungrouped?.totalCount;
  const ungroupedPage = (offset: number, limit: number) => queryRows({
    offset,
    limit,
    tags: [...rowStore.tags],
    tagMode: rowStore.tagMode,
    dedupe: "none",
    singleArtistOnly: rowStore.singleArtistOnly,
    artistFilter: rowStore.artistFilter,
    hasVibe: rowStore.hasVibe,
    untaggedOnly: rowStore.untaggedOnly,
    filters: cloneLibraryFilters(rowStore.filters),
    groupView: false,
    hideGrouped: true,
    search: rowStore.search,
    sort: "timeAsc",
  });
  if (ungroupedTotal == null) {
    const firstPage = await ungroupedPage(0, SELECT_PAGE_SIZE);
    ungroupedTotal = firstPage.totalCount;
    await appendRemainingRows(ids, firstPage.rows, firstPage.totalCount, ungroupedPage);
  } else {
    await appendRemainingRows(ids, loadedUngrouped, ungroupedTotal, ungroupedPage);
  }
  return [...ids];
}

async function duplicateViewRowIds(): Promise<number[]> {
  const ids = new Set<number>();
  const clusters = [...duplicateBrowse.clusters];
  for (const cluster of clusters) {
    const loaded = duplicateBrowse.memberCache[cluster.key]?.rows ?? [];
    await appendRemainingRows(
      ids,
      loaded,
      cluster.memberCount,
      (offset, limit) => getDedupeClusterMembers(
        duplicateBrowse.dedupeMode,
        cluster.key,
        [...rowStore.tags],
        rowStore.tagMode,
        rowStore.singleArtistOnly,
        rowStore.hasVibe,
        rowStore.untaggedOnly,
        rowStore.filters,
        rowStore.hideGrouped,
        offset,
        limit,
      ),
    );
  }
  return [...ids];
}

/** 当前视图“全选”的目标数；分组筛选下未加载未分组区时可能暂时未知。 */
export function currentViewSelectionTotal(): number | null {
  if (app.viewMode === "duplicates") {
    return duplicateBrowse.clusters.reduce((total, cluster) => total + cluster.memberCount, 0);
  }
  if (app.viewMode === "group") {
    const knownUngrouped = groupBrowse.ungrouped?.totalCount;
    if (knownUngrouped != null) {
      return groupStore.list.reduce((total, group) => total + group.memberCount, knownUngrouped);
    }
    const signature = groupScopeSignature();
    if (viewSelection.lastScopeSignature === signature) {
      return viewSelection.lastTotal;
    }
    const filtersInactive =
      rowStore.tags.length === 0 &&
      !rowStore.singleArtistOnly &&
      rowStore.artistFilter === "" &&
      !rowStore.hasVibe &&
      !rowStore.untaggedOnly &&
      rowStore.filters.length === 0 &&
      rowStore.search === "";
    return filtersInactive ? (app.snapshot?.library?.rowCount ?? null) : null;
  }
  return rowStore.totalCount;
}

/**
 * 全选当前视图的真实卡片集合。
 * 画廊/表格继续使用后端 filtered 选区；分组/重复项会补取折叠或未加载成员，
 * 固化为 explicit 行 ID，保证后续编辑、导出和删除都只作用于这个视图里的图片。
 */
export async function selectAllCurrentView(): Promise<number> {
  const mode = app.viewMode;
  if (mode !== "group" && mode !== "duplicates") {
    return selectAllFiltered();
  }
  const signature = currentScopeSignature(mode);
  try {
    const ids = mode === "group" ? await groupViewRowIds() : await duplicateViewRowIds();
    if (app.viewMode !== mode || currentScopeSignature(mode) !== signature) {
      return getSelectedCount();
    }
    setExplicitSelection(ids);
    viewSelection.lastScopeSignature = signature;
    viewSelection.lastTotal = ids.length;
    return ids.length;
  } catch (error) {
    setNotice({ tone: "error", text: `全选失败：${errorText(error)}` });
    return getSelectedCount();
  }
}
