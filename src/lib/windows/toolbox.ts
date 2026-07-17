import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

const TOOLBOX_LABEL = "toolbox";

let opening: Promise<void> | null = null;

/**
 * 打开单实例工具箱。窗口已存在时恢复并聚焦，避免重复创建多个工具状态。
 */
export function openToolboxWindow(): Promise<void> {
  if (opening) {
    return opening;
  }
  opening = openOrFocusToolbox().finally(() => {
    opening = null;
  });
  return opening;
}

async function openOrFocusToolbox(): Promise<void> {
  const existing = await WebviewWindow.getByLabel(TOOLBOX_LABEL);
  if (existing) {
    await existing.show();
    await existing.unminimize();
    await existing.setFocus();
    return;
  }

  const toolbox = new WebviewWindow(TOOLBOX_LABEL, {
    url: "/?window=toolbox",
    title: "智能表格 · 工具箱",
    width: 960,
    height: 680,
    minWidth: 760,
    minHeight: 520,
    center: true,
    resizable: true,
    decorations: false,
    parent: "main",
  });

  await new Promise<void>((resolve, reject) => {
    void toolbox.once("tauri://created", () => resolve());
    void toolbox.once<unknown>("tauri://error", event => {
      reject(new Error(`无法打开工具箱：${String(event.payload)}`));
    });
  });
}
