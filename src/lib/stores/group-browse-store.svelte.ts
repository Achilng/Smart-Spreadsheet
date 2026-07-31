import { getGroupMembers, queryRows } from "../api";
import { app, errorText } from "./app-state.svelte";
import { groupStore } from "./group-store.svelte";
import { rowStore } from "./row-store.svelte";
import { createSectionCache, MEMBERS_PAGE } from "./section-cache";
import type { SectionMembers } from "./section-types";

/**
 * 分组浏览视图的模块级状态：切走再切回时保留展开状态、已加载的成员和滚动位置。
 * 缓存按签名失效——成员缓存看 dataVersion + membershipVersion，
 * 未分组区还叠加筛选/搜索参数（getGroupMembers 不受筛选影响，未分组查询受）。
 */
export const groupBrowse = $state({
  sortByCount: false,
  expandedIds: [] as number[],
  renderLimits: {} as Record<string, number>,
  memberCache: {} as Record<string, SectionMembers>,
  ungroupedExpanded: false,
  ungrouped: null as SectionMembers | null,
});

let memberSignature = "\u{0}unloaded";
let ungroupedSignature = "\u{0}unloaded";

function currentMemberSignature(): string {
  return `${app.dataVersion}\u{1}${groupStore.membershipVersion}`;
}

function currentUngroupedSignature(): string {
  return [
    currentMemberSignature(),
    rowStore.tags.join("\u{2}"),
    rowStore.tagMode,
    String(rowStore.singleArtistOnly),
    String(rowStore.hasVibe),
    String(rowStore.untaggedOnly),
    rowStore.search,
  ].join("\u{1}");
}

/** 数据/成员关系/筛选变化时失效对应缓存；展开状态与排序偏好保留。 */
export function syncGroupBrowseCaches(): void {
  const memberSig = currentMemberSignature();
  if (memberSig !== memberSignature) {
    memberSignature = memberSig;
    groupBrowse.memberCache = {};
  }
  const ungroupedSig = currentUngroupedSignature();
  if (ungroupedSig !== ungroupedSignature) {
    ungroupedSignature = ungroupedSig;
    groupBrowse.ungrouped = null;
  }
}

const groupMemberCache = createSectionCache(
  () => groupBrowse.memberCache,
  (key, value) => { groupBrowse.memberCache[key] = value; },
  () => memberSignature,
);

export function ensureGroupMembers(groupId: number): Promise<void> {
  return groupMemberCache.ensure(
    String(groupId),
    (offset, limit) => getGroupMembers(groupId, offset, limit),
  );
}

export function loadMoreGroupMembers(groupId: number): Promise<void> {
  return groupMemberCache.loadMore(
    String(groupId),
    (offset, limit) => getGroupMembers(groupId, offset, limit),
  );
}

function ungroupedQuery(offset: number) {
  return queryRows({
    offset,
    limit: MEMBERS_PAGE,
    tags: [...rowStore.tags],
    tagMode: rowStore.tagMode,
    dedupe: "none",
    singleArtistOnly: rowStore.singleArtistOnly,
    artistFilter: rowStore.artistFilter,
    hasVibe: rowStore.hasVibe,
    untaggedOnly: rowStore.untaggedOnly,
    groupView: false,
    hideGrouped: true,
    search: rowStore.search,
    sort: "timeAsc",
  });
}

export async function ensureUngrouped(): Promise<void> {
  if (groupBrowse.ungrouped) {
    return;
  }
  const signature = ungroupedSignature;
  groupBrowse.ungrouped = { rows: [], totalCount: 0, loading: true, error: null };
  try {
    const page = await ungroupedQuery(0);
    if (signature !== ungroupedSignature) {
      return;
    }
    groupBrowse.ungrouped = {
      rows: page.rows,
      totalCount: page.totalCount,
      loading: false,
      error: null,
    };
  } catch (e) {
    if (signature !== ungroupedSignature) {
      return;
    }
    groupBrowse.ungrouped = { rows: [], totalCount: 0, loading: false, error: errorText(e) };
  }
}

export async function loadMoreUngrouped(): Promise<void> {
  const data = groupBrowse.ungrouped;
  if (!data || data.loading) {
    return;
  }
  const signature = ungroupedSignature;
  data.loading = true;
  try {
    const page = await ungroupedQuery(data.rows.length);
    if (signature !== ungroupedSignature) {
      return;
    }
    data.rows = [...data.rows, ...page.rows];
    data.totalCount = page.totalCount;
    data.loading = false;
    data.error = null;
  } catch (e) {
    if (signature !== ungroupedSignature) {
      return;
    }
    data.loading = false;
    data.error = errorText(e);
  }
}
