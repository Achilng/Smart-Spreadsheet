import {
  getDedupeClusterMembers,
  listDedupeClusters,
  type DedupeCluster,
  type DedupeMode,
} from "../api";
import { app, errorText } from "./app-state.svelte";
import { rowStore } from "./row-store.svelte";
import { sectionMenu } from "./section-context-menu.svelte";
import { createSectionCache } from "./section-cache";
import type { SectionMembers } from "./section-types";
import { cloneLibraryFilters } from "../utils/library-filters";

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
    String(rowStore.hasVibe),
    String(rowStore.untaggedOnly),
    JSON.stringify(rowStore.filters),
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
      rowStore.hasVibe,
      rowStore.untaggedOnly,
      cloneLibraryFilters(rowStore.filters),
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

function fetchMembers(key: string, offset: number, limit: number) {
  return getDedupeClusterMembers(
    duplicateBrowse.dedupeMode,
    key,
    [...rowStore.tags],
    rowStore.tagMode,
    rowStore.singleArtistOnly,
    rowStore.hasVibe,
    rowStore.untaggedOnly,
    cloneLibraryFilters(rowStore.filters),
    rowStore.hideGrouped,
    offset,
    limit,
  );
}

const clusterMemberCache = createSectionCache(
  () => duplicateBrowse.memberCache,
  (key, value) => { duplicateBrowse.memberCache[key] = value; },
  () => clusterSignature,
);

export function ensureClusterMembers(key: string): Promise<void> {
  return clusterMemberCache.ensure(key, (offset, limit) => fetchMembers(key, offset, limit));
}

export function loadMoreClusterMembers(key: string): Promise<void> {
  return clusterMemberCache.loadMore(key, (offset, limit) => fetchMembers(key, offset, limit));
}
