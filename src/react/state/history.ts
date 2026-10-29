import { create } from "zustand";
import { applyTagChanges, getRowsByIds, mutableRowState, restoreMutableRowStates, selectedRowIds, type MutableRowState, type RowSelection, type TagChanges } from "../../lib/api";
import { afterLibraryMutation } from "./library-changes";
import { notify } from "./notices";
import { runTask, useTasks } from "./tasks";
import { errorText } from "../../lib/utils/format";
import { registerHistoryClearer } from "../../lib/stores/history-context";

export interface HistoryAction { label: string; undo: () => Promise<void>; redo: () => Promise<void> }
const undoStack: HistoryAction[] = [];
const redoStack: HistoryAction[] = [];
let activeGroup: { label: string; actions: HistoryAction[] } | null = null;
export const useHistory = create<{ undoCount: number; redoCount: number; busy: boolean; undoLabel: string | null; redoLabel: string | null }>(() => ({ undoCount: 0, redoCount: 0, busy: false, undoLabel: null, redoLabel: null }));
const sync = () => useHistory.setState({ undoCount: undoStack.length, redoCount: redoStack.length, undoLabel: undoStack.at(-1)?.label ?? null, redoLabel: redoStack.at(-1)?.label ?? null });

export function recordHistory(action: HistoryAction): void {
  if (useHistory.getState().busy) return;
  redoStack.length = 0;
  if (activeGroup) { activeGroup.actions.push(action); return; }
  undoStack.push(action);
  if (undoStack.length > 50) undoStack.shift();
  sync();
}

export function beginHistoryGroup(label: string): boolean {
  if (useHistory.getState().busy || activeGroup) return false;
  activeGroup = { label, actions: [] }; return true;
}

export function commitHistoryGroup(): boolean {
  const group = activeGroup; activeGroup = null;
  if (!group?.actions.length) return false;
  recordHistory({ label: group.label,
    undo: async () => { for (const action of [...group.actions].reverse()) await action.undo(); },
    redo: async () => { for (const action of group.actions) await action.redo(); },
  });
  return true;
}

export function clearHistory(): void { undoStack.length = 0; redoStack.length = 0; activeGroup = null; sync(); }
registerHistoryClearer(clearHistory);

async function applyHistory(direction: "undo" | "redo"): Promise<void> {
  if (useHistory.getState().busy || activeGroup) return;
  const label = direction === "undo" ? "撤销" : "重做";
  if (useTasks.getState().busy) { notify(`当前有任务进行中，暂时无法${label}。`, "error"); return; }
  const source = direction === "undo" ? undoStack : redoStack;
  const destination = direction === "undo" ? redoStack : undoStack;
  const action = source.pop();
  if (!action) { notify(`没有可${label}的操作。`); return; }
  useHistory.setState({ busy: true }); sync();
  try { await runTask(label, () => action[direction]()); destination.push(action); notify(`已${label}：${action.label}。`); }
  catch (error) { source.push(action); notify(`${label}失败（${action.label}）：${errorText(error)}`, "error"); }
  finally { useHistory.setState({ busy: false }); sync(); }
}
export const undoLastAction = () => applyHistory("undo");
export const redoLastAction = () => applyHistory("redo");

export async function captureRowStates(ids: number[]): Promise<MutableRowState[]> {
  return ids.length ? (await getRowsByIds([...new Set(ids)].sort((a, b) => a - b))).map(mutableRowState) : [];
}
export async function captureSelectionStates(selection: RowSelection): Promise<MutableRowState[]> {
  return captureRowStates(await selectedRowIds(selection));
}
export async function restoreRowStates(states: MutableRowState[]): Promise<void> {
  if (!states.length) return;
  await restoreMutableRowStates(states);
  await afterLibraryMutation();
}
export async function recordRowStateChange(label: string, before: MutableRowState[]): Promise<boolean> {
  if (!before.length) return false;
  const after = await captureRowStates(before.map(row => row.rowId));
  if (JSON.stringify(before) === JSON.stringify(after)) return false;
  recordHistory({ label, undo: () => restoreRowStates(before), redo: () => restoreRowStates(after) });
  return true;
}

export async function refreshTagChanges(changes: TagChanges): Promise<void> {
  const ids = new Set<number>();
  for (const change of [...changes.added, ...changes.removed]) for (const id of change.rowIds) ids.add(id);
  await afterLibraryMutation({ impact: { kind: "tags", rowIds: [...ids] } });
}

/** The committed backend delta is also the history entry: no whole-row rereads. */
export function recordTagChange(changes: TagChanges, label = "编辑 Tag"): boolean {
  if (!changes.added.length && !changes.removed.length) return false;
  const restore = async (reverse: boolean) => {
    await applyTagChanges(changes, reverse);
    await refreshTagChanges(changes);
  };
  recordHistory({ label, undo: () => restore(true), redo: () => restore(false) });
  return true;
}
