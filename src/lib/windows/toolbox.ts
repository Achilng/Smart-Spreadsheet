import { invoke } from "@tauri-apps/api/core";

import type { RowSelection } from "../api";

let opening: Promise<void> | null = null;

export interface ToolboxRowRequest {
  rowId: number;
}

export interface ToolboxSelectionSnapshot {
  selection: RowSelection;
  count: number;
}

/**
 * 由 Tauri 后端打开内部工具箱窗口。
 * 后端负责单实例、恢复与聚焦，避免前端 URL 被系统浏览器接管。
 */
export function openToolboxWindow(): Promise<void> {
  if (opening) {
    return opening;
  }
  opening = invoke<void>("open_toolbox_window").finally(() => {
    opening = null;
  });
  return opening;
}

export function focusMainWindow(): Promise<void> {
  return invoke<void>("focus_main_window");
}
