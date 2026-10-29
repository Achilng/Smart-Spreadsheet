import { create } from "zustand";
import { assignRowsToGroup, createGroup, deleteEmptyGroups, deleteGroup, getGroupMembers, listGroups, mutableRowState, queryRows, renameGroup, restoreGroup, restoreMutableRowStates, ungroupRows, type GroupSummary, type MutableRowState, type RowSelection } from "../../lib/api";
import { errorText } from "../../lib/utils/format";
import { beginHistoryGroup, captureSelectionStates, commitHistoryGroup, recordHistory, recordRowStateChange } from "./history";
import { useLibrary, useRows } from "./library";
import { afterLibraryMutation } from "./library-changes";
import type { SortMode } from "../../lib/api";

import { createSectionPager, type SectionMembers } from "./section-members";
export type { SectionMembers } from "./section-members";
interface GroupsState {
  memberSort: SortMode;
  list: GroupSummary[]; loading: boolean; error: string | null; sortByCount: boolean; layout: "shelf" | "list"; search: string;
  expanded: string[]; members: Record<string, SectionMembers>; renderLimits: Record<string, number>; version: number;
}
export const useGroups = create<GroupsState>(() => ({ memberSort: "timeAsc", list: [], loading: false, error: null, sortByCount: false, layout: "shelf", search: "", expanded: [], members: {}, renderLimits: {}, version: 0 }));
const memberPager = createSectionPager({
  read: key => useGroups.getState().members[key],
  write: (key, value) => useGroups.setState(state => ({ members: { ...state.members, [key]: value } })),
  clear: preserve => useGroups.setState(state => ({ members: preserve ? Object.fromEntries(Object.entries(state.members).map(([key, value]) => [key, { ...value, loading: false, stale: true }])) : {} })),
  fetch: (key, offset, limit) => {
    const sort = useGroups.getState().memberSort;
    return key === "ungrouped"
      ? queryRows({ ...structuredClone(useRows.getState().query), offset, limit, dedupe: "none", groupView: false, hideGrouped: true, sort })
      : getGroupMembers(Number(key), offset, limit, sort);
  },
});
let listGeneration = 0;
let directory: string | null | undefined;
export async function loadGroups(): Promise<void> {
  const request = ++listGeneration;
  useGroups.setState({ loading: true, error: null });
  try { const list = await listGroups(); if (request === listGeneration) useGroups.setState(state => ({ list, expanded: state.expanded.filter(key => key === "ungrouped" || list.some(group => String(group.id) === key)) })); }
  catch (error) { if (request === listGeneration) useGroups.setState({ error: errorText(error) }); }
  finally { if (request === listGeneration) useGroups.setState({ loading: false }); }
}
export function syncGroups(force = false): Promise<void> {
  const rows = useRows.getState();
  const nextDirectory = useLibrary.getState().snapshot?.dataDirectory;
  const changedDirectory = nextDirectory !== directory;
  const next = JSON.stringify([nextDirectory, rows.resetToken, rows.query, useGroups.getState().memberSort]);
  if (!memberPager.sync(next, force, changedDirectory)) return Promise.resolve();
  directory = nextDirectory;
  useGroups.setState(state => ({ renderLimits: {}, version: state.version + 1, ...(changedDirectory ? { list: [], expanded: [], search: "" } : {}) }));
  const loading = loadGroups();
  for (const key of useGroups.getState().expanded) void loadGroupMembers(key);
  return loading;
}
export const invalidateGroups = () => syncGroups(true);
export function setGroupMemberSort(memberSort: SortMode): void {
  if (useGroups.getState().memberSort === memberSort) return;
  useGroups.setState({ memberSort });
  void syncGroups();
}
export const loadGroupMembers = memberPager.load;
export function toggleGroup(key: string): void {
  const state = useGroups.getState();
  const expanded = state.expanded.includes(key) ? state.expanded.filter(item => item !== key) : [...state.expanded, key];
  useGroups.setState({ expanded });
  if (expanded.includes(key)) void loadGroupMembers(key);
}
export async function refreshGroupsAfterMutation(): Promise<void> {
  await afterLibraryMutation({ afterRefresh: () => syncGroups(true) });
}
export async function createNewGroup(name: string): Promise<GroupSummary> {
  const group = await createGroup(name.trim());
  recordHistory({ label: `新建分组「${group.name}」`, undo: async () => { await deleteGroup(group.id); await refreshGroupsAfterMutation(); }, redo: async () => { await restoreGroup(group); await refreshGroupsAfterMutation(); } });
  await loadGroups(); return group;
}
export async function renameExistingGroup(group: GroupSummary, name: string): Promise<void> {
  const renamed = await renameGroup(group.id, name.trim());
  if (renamed.name !== group.name) recordHistory({ label: `重命名分组「${group.name}」`, undo: async () => { await renameGroup(group.id, group.name); await refreshGroupsAfterMutation(); }, redo: async () => { await renameGroup(group.id, renamed.name); await refreshGroupsAfterMutation(); } });
  await refreshGroupsAfterMutation();
}
export async function removeGroup(group: GroupSummary): Promise<void> {
  const members: MutableRowState[] = [];
  for (let offset = 0; ; offset += 500) {
    const page = await getGroupMembers(group.id, offset, 500);
    members.push(...page.rows.map(mutableRowState));
    if (!page.hasMore || !page.rows.length) break;
  }
  await deleteGroup(group.id);
  recordHistory({ label: `删除分组「${group.name}」`, undo: async () => {
    await restoreGroup(group);
    try { if (members.length) await restoreMutableRowStates(members); }
    catch (error) { await deleteGroup(group.id); throw error; }
    await refreshGroupsAfterMutation();
  }, redo: async () => { await deleteGroup(group.id); await refreshGroupsAfterMutation(); } });
  await refreshGroupsAfterMutation();
}
export async function cleanEmptyGroups(): Promise<number> {
  const empty = (await listGroups()).filter(group => !group.memberCount);
  const count = await deleteEmptyGroups();
  if (count && empty.length) recordHistory({ label: `清理 ${count} 个空分组`, undo: async () => {
    const restored: number[] = [];
    try { for (const group of empty) { await restoreGroup(group); restored.push(group.id); } }
    catch (error) { for (const id of restored) await deleteGroup(id); throw error; }
    await refreshGroupsAfterMutation();
  }, redo: async () => { for (const group of empty) await deleteGroup(group.id); await refreshGroupsAfterMutation(); } });
  await refreshGroupsAfterMutation(); return count;
}
export async function assignToGroup(selection: RowSelection, group: GroupSummary | null): Promise<number> {
  const before = await captureSelectionStates(selection);
  const count = group ? await assignRowsToGroup(selection, group.id) : await ungroupRows(selection);
  await afterLibraryMutation({
    history: () => recordRowStateChange(group ? `分配到分组「${group.name}」` : "取消分组", before),
    afterRefresh: () => syncGroups(true),
  });
  return count;
}
export async function mergeGroups(source: GroupSummary, target: GroupSummary): Promise<number> {
  if (source.id === target.id) throw new Error("不能合并到同一个分组。");
  const ids: number[] = [];
  for (let offset = 0; ; offset += 500) {
    const page = await getGroupMembers(source.id, offset, 500);
    ids.push(...page.rows.map(row => row.id));
    if (!page.hasMore || !page.rows.length) break;
  }
  const grouped = beginHistoryGroup(`将「${source.name}」合并到「${target.name}」`);
  try {
    const count = ids.length ? await assignToGroup({ kind: "explicit", rowIds: ids }, target) : 0;
    await removeGroup({ ...source, memberCount: 0 });
    return count;
  } finally { if (grouped) commitHistoryGroup(); }
}
