import { getCurrentWindow } from "@tauri-apps/api/window";
import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";

/**
 * 窗口关闭守卫：任务进行中 / 有未保存内容时拦截关闭并确认。
 * 各组件用 registerCloseGuard 登记自己的阻塞原因（返回 null 表示放行），
 * 窗口入口组件调用 installCloseGuards 挂接 Tauri 的 CloseRequested。
 */
export type CloseGuard = () => string | null | Promise<string | null>;

const guards = new Set<CloseGuard>();

export function registerCloseGuard(guard: CloseGuard): () => void {
  guards.add(guard);
  return () => {
    guards.delete(guard);
  };
}

let confirming = false;

export async function installCloseGuards(): Promise<() => void> {
  const appWindow = getCurrentWindow();
  return appWindow.onCloseRequested(async event => {
    if (confirming) {
      event.preventDefault();
      return;
    }
    const reasons: string[] = [];
    for (const guard of guards) {
      try {
        const reason = await guard();
        if (reason) {
          reasons.push(reason);
        }
      } catch {
        // 守卫自身出错不应卡死关闭流程
      }
    }
    if (reasons.length === 0) {
      return;
    }
    // 先阻止本次关闭，再询问；确认后手动销毁窗口。
    event.preventDefault();
    confirming = true;
    try {
      const confirmed = await confirmDialog(
        `${reasons.map(reason => `• ${reason}`).join("\n")}\n\n现在关闭可能丢失进度或未保存的内容，确定要关闭吗？`,
        { title: "确认关闭窗口", kind: "warning", okLabel: "仍要关闭", cancelLabel: "取消" },
      );
      if (confirmed) {
        await appWindow.destroy();
      }
    } finally {
      confirming = false;
    }
  });
}
