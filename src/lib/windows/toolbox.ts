import { invoke } from "@tauri-apps/api/core";

let opening: Promise<void> | null = null;

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
