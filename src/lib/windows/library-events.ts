import { emitTo } from "@tauri-apps/api/event";

export type MainStateChange = "migrated" | "reset" | "libraryEdited";

/**
 * 通知对比窗口数据目录已被切换/迁移/重置，其样本与查询结果全部失效，
 * 窗口收到后直接关闭（未打开时静默失败）。
 */
export function notifyCompareLibraryReset(): void {
  void emitTo("compare", "main://library-reset").catch(() => {});
}

export async function notifyMainStateChanged(kind: MainStateChange): Promise<void> {
  try {
    await emitTo("main", "toolbox://app-state-changed", kind);
  } catch {
    // 主窗口可能已经关闭；数据操作本身已经成功，不应被通知失败反向判为失败。
  }
}

export function notifyToolboxLibraryChanged(origin: "main" | "toolbox"): void {
  void emitTo("toolbox", "main://library-changed", origin).catch(() => {});
}
