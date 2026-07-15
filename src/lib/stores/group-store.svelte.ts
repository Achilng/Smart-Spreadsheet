import {
  assignRowsToGroup,
  createGroup,
  deleteEmptyGroups,
  deleteGroup,
  getGroupMembers,
  listGroups,
  mutableRowState,
  renameGroup,
  restoreMutableRowStates,
  restoreGroup,
  ungroupRows,
  type GroupSummary,
  type MutableRowState,
  type RowSelection,
} from "../api";
import { bumpDataVersion, errorText } from "./app-state.svelte";
import { recordHistory } from "./history.svelte";
import { loadTags } from "./tag-store.svelte";

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
    recordHistory({
      label: `新建分组「${group.name}」`,
      undo: async () => {
        await deleteGroup(group.id);
        bumpGroupMembership();
        await loadGroups();
        bumpDataVersion({ preserveScroll: true });
      },
      redo: async () => {
        await restoreGroup(group);
        bumpGroupMembership();
        await loadGroups();
        bumpDataVersion({ preserveScroll: true });
      },
    });
    return group;
  } catch (e) {
    groupStore.error = errorText(e);
    return null;
  }
}

export async function renameExistingGroup(groupId: number, newName: string): Promise<boolean> {
  try {
    const oldName = groupStore.list.find(group => group.id === groupId)?.name;
    const renamed = await renameGroup(groupId, newName);
    await loadGroups();
    if (oldName && oldName !== renamed.name) {
      recordHistory({
        label: `重命名分组「${oldName}」`,
        undo: async () => {
          await renameGroup(groupId, oldName);
          await loadGroups();
        },
        redo: async () => {
          await renameGroup(groupId, renamed.name);
          await loadGroups();
        },
      });
    }
    return true;
  } catch (e) {
    groupStore.error = errorText(e);
    return false;
  }
}

export async function removeGroup(groupId: number): Promise<boolean> {
  try {
    const group = groupStore.list.find(candidate => candidate.id === groupId);
    const members = group ? await captureGroupMembers(group) : [];
    await deleteGroup(groupId);
    bumpGroupMembership();
    await loadGroups();
    if (group) {
      recordHistory({
        label: `删除分组「${group.name}」`,
        undo: async () => {
          await restoreGroup(group);
          try {
            if (members.length > 0) {
              await restoreGroupMemberStates(members);
            } else {
              bumpGroupMembership();
              await loadGroups();
              bumpDataVersion({ preserveScroll: true });
            }
          } catch (error) {
            // 行状态恢复失败时撤回分组定义，保证下次撤销可重试。
            await deleteGroup(group.id);
            throw error;
          }
        },
        redo: async () => {
          await deleteGroup(group.id);
          bumpGroupMembership();
          await loadGroups();
          bumpDataVersion({ preserveScroll: true });
        },
      });
    }
    return true;
  } catch (e) {
    groupStore.error = errorText(e);
    return false;
  }
}

export async function cleanEmptyGroups(): Promise<number> {
  try {
    const emptyGroups = groupStore.list.filter(group => group.memberCount === 0);
    const count = await deleteEmptyGroups();
    bumpGroupMembership();
    await loadGroups();
    if (count > 0 && emptyGroups.length > 0) {
      recordHistory({
        label: `清理 ${count} 个空分组`,
        undo: async () => {
          const restoredIds: number[] = [];
          try {
            for (const group of emptyGroups) {
              await restoreGroup(group);
              restoredIds.push(group.id);
            }
          } catch (error) {
            for (const groupId of restoredIds) {
              await deleteGroup(groupId);
            }
            throw error;
          }
          bumpGroupMembership();
          await loadGroups();
        },
        redo: async () => {
          await deleteEmptyGroups();
          bumpGroupMembership();
          await loadGroups();
        },
      });
    }
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

async function captureGroupMembers(group: GroupSummary): Promise<MutableRowState[]> {
  const rows: MutableRowState[] = [];
  let offset = 0;
  const limit = 500;
  while (offset < group.memberCount) {
    const page = await getGroupMembers(group.id, offset, limit);
    rows.push(...page.rows.map(mutableRowState));
    if (!page.hasMore || page.rows.length === 0) {
      break;
    }
    offset += page.rows.length;
  }
  return rows;
}

async function restoreGroupMemberStates(states: MutableRowState[]): Promise<void> {
  await restoreMutableRowStates(states);
  bumpGroupMembership();
  await Promise.all([loadGroups(), loadTags()]);
  bumpDataVersion({ preserveScroll: true });
}
