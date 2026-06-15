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
});

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
    await loadGroups();
    return count;
  } catch (e) {
    groupStore.error = errorText(e);
    return 0;
  }
}
