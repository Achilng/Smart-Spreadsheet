import { invoke } from "@tauri-apps/api/core";

let opening: Promise<void> | null = null;

/**
 * 以指定图片为样本打开（或复用并切换样本）对比窗口。
 * 后端负责单实例、URL 传参与 `compare://set-sample` 推送，
 * 前端不需要自己跨窗口发事件，也没有新建窗口的就绪竞态。
 */
export function openCompareWindow(rowId: number): Promise<void> {
  if (opening) {
    return opening;
  }
  opening = invoke<void>("open_compare_window", { rowId }).finally(() => {
    opening = null;
  });
  return opening;
}
