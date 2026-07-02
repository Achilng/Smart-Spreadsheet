import { getGroupMembers, queryRows, type RowRecord } from "../api";
import { app, errorText } from "./app-state.svelte";
import { groupStore } from "./group-store.svelte";
import { rowStore } from "./row-store.svelte";

export const MEMBERS_PAGE = 200;

export interface SectionMembers {
  rows: RowRecord[];
  totalCount: number;
  loading: boolean;
  error: string | null;
}

/**
 * 分组浏览视图的模块级状态：切走再切回时保留展开状态、已加载的成员和滚动位置。
 * 缓存按签名失效——成员缓存看 dataVersion + membershipVersion，
 * 未分组区还叠加筛选/搜索参数（getGroupMembers 不受筛选影响，未分组查询受）。
 */
export const groupBrowse = $state({
  sortByCount: false,
  expandedIds: [] as number[],
  renderLimits: {} as Record<string, number>,
  memberCache: {} as Record<number, SectionMembers>,
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

export async function ensureGroupMembers(groupId: number): Promise<void> {
  if (groupBrowse.memberCache[groupId]) {
    return;
  }
  const signature = memberSignature;
  groupBrowse.memberCache[groupId] = { rows: [], totalCount: 0, loading: true, error: null };
  try {
    const page = await getGroupMembers(groupId, 0, MEMBERS_PAGE);
    if (signature !== memberSignature) {
      return;
    }
    groupBrowse.memberCache[groupId] = {
      rows: page.rows,
      totalCount: page.totalCount,
      loading: false,
      error: null,
    };
  } catch (e) {
    if (signature !== memberSignature) {
      return;
    }
    groupBrowse.memberCache[groupId] = {
      rows: [],
      totalCount: 0,
      loading: false,
      error: errorText(e),
    };
  }
}

export async function loadMoreGroupMembers(groupId: number): Promise<void> {
  const data = groupBrowse.memberCache[groupId];
  if (!data || data.loading) {
    return;
  }
  const signature = memberSignature;
  data.loading = true;
  try {
    const page = await getGroupMembers(groupId, data.rows.length, MEMBERS_PAGE);
    if (signature !== memberSignature) {
      return;
    }
    data.rows = [...data.rows, ...page.rows];
    data.totalCount = page.totalCount;
    data.loading = false;
    data.error = null;
  } catch (e) {
    if (signature !== memberSignature) {
      return;
    }
    data.loading = false;
    data.error = errorText(e);
  }
}

function ungroupedQuery(offset: number) {
  return queryRows({
    offset,
    limit: MEMBERS_PAGE,
    tags: [...rowStore.tags],
    tagMode: rowStore.tagMode,
    dedupe: "none",
    singleArtistOnly: rowStore.singleArtistOnly,
    groupView: false,
    hideGrouped: true,
    search: rowStore.search,
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
