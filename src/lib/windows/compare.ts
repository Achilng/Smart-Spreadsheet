import { emitTo } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

/**
 * 对比窗口跨窗口事件：
 * - 主窗口右键“对比”时记住样本并推送 `main://compare-sample`；
 * - 对比窗口挂载时向主窗口拉取一次当前样本（`compare://request-sample`），
 *   避免新建窗口时监听器尚未就绪而错过首次推送；
 * - 对比窗口请求跳转时使用 `toolbox://open-row`（主画廊定位）与
 *   `compare://open-artists`（画师串筛选）。
 */
export const COMPARE_SAMPLE_EVENT = "main://compare-sample";
export const COMPARE_REQUEST_EVENT = "compare://request-sample";
export const COMPARE_OPEN_ARTISTS_EVENT = "compare://open-artists";

export interface CompareSamplePayload {
  rowId: number;
}

export interface CompareArtistsPayload {
  artists: string;
}

/** 主窗口记住的最新对比样本；对比窗口挂载时向主窗口索取。 */
let currentSampleRowId: number | null = null;

export function getCurrentCompareSample(): number | null {
  return currentSampleRowId;
}

/**
 * 打开（或聚焦）对比窗口并切换样本。主窗口与对比窗口都可调用：
 * 主窗口调用会更新“当前样本”记忆，对比窗口内部切换样本时传 `remember = false`。
 */
export async function openCompareWindowWithSample(
  rowId: number,
  remember = true,
): Promise<void> {
  if (remember) {
    currentSampleRowId = rowId;
  }
  await invoke<void>("open_compare_window");
  try {
    await emitTo("compare", COMPARE_SAMPLE_EVENT, { rowId } satisfies CompareSamplePayload);
  } catch {
    // 新建窗口的监听器可能尚未就绪；对比窗口挂载后会主动拉取一次样本。
  }
}
