import {
  getRowsByIds,
  mutableRowState,
  restoreMutableRowStates,
  selectedRowIds,
  type MutableRowState,
  type RowSelection,
} from "../api";
import { bumpDataVersion } from "./app-state.svelte";
import { bumpGroupMembership, loadGroups } from "./group-store.svelte";
import { recordHistory } from "./history.svelte";
import { loadTags } from "./tag-store.svelte";

export async function captureRowStates(rowIds: number[]): Promise<MutableRowState[]> {
  if (rowIds.length === 0) {
    return [];
  }
  const rows = await getRowsByIds([...new Set(rowIds)].sort((left, right) => left - right));
  return rows.map(mutableRowState);
}

export async function captureSelectionStates(
  selection: RowSelection,
): Promise<MutableRowState[]> {
  return captureRowStates(await selectedRowIds(selection));
}

export async function restoreRowStates(states: MutableRowState[]): Promise<void> {
  if (states.length === 0) {
    return;
  }
  await restoreMutableRowStates(states);
  bumpGroupMembership();
  await Promise.all([loadGroups(), loadTags()]);
  bumpDataVersion({ preserveScroll: true });
}

/**
 * 操作成功后读取同一批行的新状态并记入历史。
 * 返回 false 表示操作没有产生可见行状态变化。
 */
export async function recordRowStateChange(
  label: string,
  before: MutableRowState[],
): Promise<boolean> {
  if (before.length === 0) {
    return false;
  }
  const after = await captureRowStates(before.map(state => state.rowId));
  if (JSON.stringify(before) === JSON.stringify(after)) {
    return false;
  }
  recordHistory({
    label,
    undo: () => restoreRowStates(before),
    redo: () => restoreRowStates(after),
  });
  return true;
}
