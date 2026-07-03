import {
  getDedupeClusterMembers,
  listDedupeClusters,
  type DedupeCluster,
  type DedupeMode,
} from "../api";
import { app, errorText } from "./app-state.svelte";
import { rowStore } from "./row-store.svelte";
import { sectionMenu } from "./section-context-menu.svelte";
import type { SectionMembers } from "./section-types";

const MEMBERS_PAGE = 200;

/**
 * 重复视图的模块级状态：聚合结果、展开状态和已加载成员跨视图切换保留，
 * 切回时无需重新聚合。数据/筛选/聚合依据变化时整体失效并重载；
 * 别名变化只静默重载聚合列表，保留展开状态与成员缓存。
 */
export const duplicateBrowse = $state({
  dedupeMode: "artists" as DedupeMode,
  sortByCount: true,
  clusters: [] as DedupeCluster[],
  loading: false,
  error: null as string | null,
  expandedKeys: [] as string[],
  memberCache: {} as Record<string, SectionMembers>,
});

let clusterSignature = "\u{0}unloaded";
let aliasSeen = -1;
let loadGeneration = 0;

function currentClusterSignature(): string {
  return [
    String(app.dataVersion),
    duplicateBrowse.dedupeMode,
    rowStore.tags.join("\u{2}"),
    rowStore.tagMode,
    String(rowStore.singleArtistOnly),
    String(rowStore.hideGrouped),
  ].join("\u{1}");
}

/** 数据/筛选/聚合依据变化时重载并清空展开状态；仅别名变化时静默刷新列表。 */
export function syncDuplicateCaches(): void {
  const signature = currentClusterSignature();
  if (signature !== clusterSignature) {
    clusterSignature = signature;
    duplicateBrowse.expandedKeys = [];
    duplicateBrowse.memberCache = {};
    void loadClusters();
  } else if (sectionMenu.aliasVersion !== aliasSeen) {
    void loadClusters();
  }
  aliasSeen = sectionMenu.aliasVersion;
}

async function loadClusters(): Promise<void> {
  const generation = ++loadGeneration;
  // 有旧内容时保持渲染，只在后台替换，避免闪"加载中"
  duplicateBrowse.loading = duplicateBrowse.clusters.length === 0;
  duplicateBrowse.error = null;
  try {
    const clusters = await listDedupeClusters(
      duplicateBrowse.dedupeMode,
      [...rowStore.tags],
      rowStore.tagMode,
      rowStore.singleArtistOnly,
      rowStore.hideGrouped,
    );
    if (generation !== loadGeneration) {
      return;
    }
    duplicateBrowse.clusters = clusters;
  } catch (e) {
    if (generation !== loadGeneration) {
      return;
    }
    duplicateBrowse.error = errorText(e);
    duplicateBrowse.clusters = [];
  } finally {
    if (generation === loadGeneration) {
      duplicateBrowse.loading = false;
    }
  }
}

function fetchMembers(key: string, offset: number) {
  return getDedupeClusterMembers(
    duplicateBrowse.dedupeMode,
    key,
    [...rowStore.tags],
    rowStore.tagMode,
    rowStore.singleArtistOnly,
    rowStore.hideGrouped,
    offset,
    MEMBERS_PAGE,
  );
}

export async function ensureClusterMembers(key: string): Promise<void> {
  if (duplicateBrowse.memberCache[key]) {
    return;
  }
  const signature = clusterSignature;
  duplicateBrowse.memberCache[key] = { rows: [], totalCount: 0, loading: true, error: null };
  try {
    const page = await fetchMembers(key, 0);
    if (signature !== clusterSignature) {
      return;
    }
    duplicateBrowse.memberCache[key] = {
      rows: page.rows,
      totalCount: page.totalCount,
      loading: false,
      error: null,
    };
  } catch (e) {
    if (signature !== clusterSignature) {
      return;
    }
    duplicateBrowse.memberCache[key] = {
      rows: [],
      totalCount: 0,
      loading: false,
      error: errorText(e),
    };
  }
}

export async function loadMoreClusterMembers(key: string): Promise<void> {
  const data = duplicateBrowse.memberCache[key];
  if (!data || data.loading) {
    return;
  }
  const signature = clusterSignature;
  data.loading = true;
  try {
    const page = await fetchMembers(key, data.rows.length);
    if (signature !== clusterSignature) {
      return;
    }
    data.rows = [...data.rows, ...page.rows];
    data.totalCount = page.totalCount;
    data.loading = false;
    data.error = null;
  } catch (e) {
    if (signature !== clusterSignature) {
      return;
    }
    data.loading = false;
    data.error = errorText(e);
  }
}
