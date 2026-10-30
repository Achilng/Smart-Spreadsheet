import { emitTo } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

import type { RowSelection } from "../api";

let opening: Promise<void> | null = null;

export interface ToolboxRowRequest {
  rowId: number;
}

export interface ToolboxSelectionSnapshot {
  filteredSelection?: RowSelection;
  selection: RowSelection;
  count: number;
}

/**
 * 由 Tauri 后端打开内部工具箱窗口。
 * 后端负责单实例、恢复与聚焦，避免前端 URL 被系统浏览器接管。
 */
export async function openToolboxWindow(target?: "llmSettings"): Promise<void> {
  if (opening) {
    await opening;
    if (!target) return;
  }
  opening = invoke<void>("open_toolbox_window", { target: target ?? null }).finally(() => {
    opening = null;
  });
  return opening;
}

export function focusMainWindow(): Promise<void> {
  return invoke<void>("focus_main_window");
}

/** Reveal a library row from any toolbox feature, then focus the main window. */
export async function openRowInMainWindow(rowId: number): Promise<void> {
  const request: ToolboxRowRequest = { rowId };
  await emitTo("main", "toolbox://open-row", request);
  await focusMainWindow();
}
