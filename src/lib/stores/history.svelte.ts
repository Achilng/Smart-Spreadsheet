import { app, errorText, registerHistoryClearer, setNotice } from "./app-state.svelte";

const HISTORY_LIMIT = 50;

export interface HistoryAction {
  /** 用户可读的操作名，用于撤销/重做结果提示 */
  label: string;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
}

const undoStack: HistoryAction[] = [];
const redoStack: HistoryAction[] = [];
let activeGroup: { label: string; actions: HistoryAction[] } | null = null;

export const history = $state({
  undoCount: 0,
  redoCount: 0,
  busy: false,
  undoLabel: null as string | null,
  redoLabel: null as string | null,
});

function syncState(): void {
  history.undoCount = undoStack.length;
  history.redoCount = redoStack.length;
  history.undoLabel = undoStack.at(-1)?.label ?? null;
  history.redoLabel = redoStack.at(-1)?.label ?? null;
}

/** 记录一个已经成功执行的可逆操作。新操作会丢弃现有重做分支。 */
export function recordHistory(action: HistoryAction): void {
  if (history.busy) {
    return;
  }
  if (activeGroup) {
    activeGroup.actions.push(action);
    redoStack.length = 0;
    return;
  }
  undoStack.push(action);
  if (undoStack.length > HISTORY_LIMIT) {
    undoStack.shift();
  }
  redoStack.length = 0;
  syncState();
}

/** 在一个用户操作内收集多个子操作，提交后只占一步历史。 */
export function beginHistoryGroup(label: string): boolean {
  if (history.busy || activeGroup) {
    return false;
  }
  activeGroup = { label, actions: [] };
  return true;
}

export function commitHistoryGroup(): boolean {
  const group = activeGroup;
  activeGroup = null;
  if (!group || group.actions.length === 0) {
    return false;
  }
  const groupedAction: HistoryAction = {
    label: group.label,
    undo: async () => {
      for (const action of [...group.actions].reverse()) {
        await action.undo();
      }
    },
    redo: async () => {
      for (const action of group.actions) {
        await action.redo();
      }
    },
  };
  undoStack.push(group.actions.length === 1 ? { ...group.actions[0], label: group.label } : groupedAction);
  if (undoStack.length > HISTORY_LIMIT) {
    undoStack.shift();
  }
  syncState();
  return true;
}

/** 换库、重置等让历史上下文失效的操作必须清空会话历史。 */
export function clearHistory(): void {
  activeGroup = null;
  undoStack.length = 0;
  redoStack.length = 0;
  syncState();
}

registerHistoryClearer(clearHistory);

export async function undoLastAction(): Promise<void> {
  if (history.busy || app.busy || activeGroup) {
    return;
  }
  const action = undoStack.pop();
  if (!action) {
    return;
  }
  history.busy = true;
  app.busy = true;
  syncState();
  try {
    await action.undo();
    redoStack.push(action);
    setNotice({ tone: "success", text: `已撤销：${action.label}。` });
  } catch (error) {
    undoStack.push(action);
    setNotice({ tone: "error", text: `撤销失败（${action.label}）：${errorText(error)}` });
  } finally {
    history.busy = false;
    app.busy = false;
    syncState();
  }
}

export async function redoLastAction(): Promise<void> {
  if (history.busy || app.busy || activeGroup) {
    return;
  }
  const action = redoStack.pop();
  if (!action) {
    return;
  }
  history.busy = true;
  app.busy = true;
  syncState();
  try {
    await action.redo();
    undoStack.push(action);
    setNotice({ tone: "success", text: `已重做：${action.label}。` });
  } catch (error) {
    redoStack.push(action);
    setNotice({ tone: "error", text: `重做失败（${action.label}）：${errorText(error)}` });
  } finally {
    history.busy = false;
    app.busy = false;
    syncState();
  }
}
