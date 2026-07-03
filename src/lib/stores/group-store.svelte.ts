import {
  assignRowsToGroup,
  createGroup,
  deleteEmptyGroups,
  deleteGroup,
  listGroups,
  renameGroup,
  ungroupRows,
  type GroupSummary,
  type RowSelection,
} from "../api";
import { errorText } from "./app-state.svelte";

export const groupStore = $state({
  list: [] as GroupSummary[],
  loading: false,
  error: null as string | null,
  /** 分组成员关系变化（分配/移出/删除分组）时 +1，成员缓存据此失效 */
  membershipVersion: 0,
});

export function bumpGroupMembership(): void {
  groupStore.membershipVersion += 1;
}

export async function loadGroups(): Promise<void> {
  groupStore.loading = true;
  groupStore.error = null;
  try {
    groupStore.list = await listGroups();
  } catch (e) {
    groupStore.error = errorText(e);
  } finally {
    groupStore.loading = false;
  }
}

export async function createNewGroup(name: string): Promise<GroupSummary | null> {
  try {
    const group = await createGroup(name);
    await loadGroups();
    return group;
  } catch (e) {
    groupStore.error = errorText(e);
    return null;
  }
}

export async function renameExistingGroup(groupId: number, newName: string): Promise<boolean> {
  try {
    await renameGroup(groupId, newName);
    await loadGroups();
    return true;
  } catch (e) {
    groupStore.error = errorText(e);
    return false;
  }
}

export async function removeGroup(groupId: number): Promise<boolean> {
  try {
    await deleteGroup(groupId);
    bumpGroupMembership();
    await loadGroups();
    return true;
  } catch (e) {
    groupStore.error = errorText(e);
    return false;
  }
}

export async function cleanEmptyGroups(): Promise<number> {
  try {
    const count = await deleteEmptyGroups();
    bumpGroupMembership();
    await loadGroups();
    return count;
  } catch (e) {
    groupStore.error = errorText(e);
    return 0;
  }
}

export async function assignToGroup(selection: RowSelection, groupId: number): Promise<number> {
  try {
    const count = await assignRowsToGroup(selection, groupId);
    bumpGroupMembership();
    await loadGroups();
    return count;
  } catch (e) {
    groupStore.error = errorText(e);
    return 0;
  }
}

export async function removeFromGroup(selection: RowSelection): Promise<number> {
  try {
    const count = await ungroupRows(selection);
    bumpGroupMembership();
    await loadGroups();
    return count;
  } catch (e) {
    groupStore.error = errorText(e);
    return 0;
  }
}
