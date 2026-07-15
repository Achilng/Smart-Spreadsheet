import { app, errorText, setNotice } from "./app-state.svelte";

const HISTORY_LIMIT = 50;

export interface HistoryAction {
  /** 用户可读的操作名，用于撤销/重做结果提示 */
  label: string;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
}

const undoStack: HistoryAction[] = [];
const redoStack: HistoryAction[] = [];

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
  undoStack.push(action);
  if (undoStack.length > HISTORY_LIMIT) {
    undoStack.shift();
  }
  redoStack.length = 0;
  syncState();
}

/** 换库、重置等让历史上下文失效的操作必须清空会话历史。 */
export function clearHistory(): void {
  undoStack.length = 0;
  redoStack.length = 0;
  syncState();
}

export async function undoLastAction(): Promise<void> {
  if (history.busy || app.busy) {
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
  if (history.busy || app.busy) {
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
