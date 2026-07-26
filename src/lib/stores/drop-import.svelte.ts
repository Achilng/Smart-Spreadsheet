import { getCurrentWebview } from "@tauri-apps/api/webview";

import { app, formatCount, setNotice } from "./app-state.svelte";
import { outboundDrag } from "./file-drag";
import { runImageImport } from "./import-actions.svelte";

const ACCEPTED_EXTENSIONS = new Set([
  "zip", "7z", "rar",
  "png",
]);

export const dropState = $state({
  dragging: false,
  open: false,
  paths: [] as string[],
  /** 本次拖入中被过滤掉的不支持文件数，确认对话框里说明 */
  ignoredCount: 0,
  busy: false,
  /** 逐项导入进度：当前第几个（1 起）/共几个 */
  currentIndex: 0,
});

function isAcceptedPath(p: string): boolean {
  const ext = p.split(".").pop()?.toLowerCase() ?? "";
  if (ACCEPTED_EXTENSIONS.has(ext)) return true;
  if (!p.includes(".")) return true;
  return false;
}

export function filterDroppedPaths(raw: string[]): string[] {
  return raw.filter(isAcceptedPath);
}

export function requestDropImport(paths: string[]): void {
  if (dropState.busy) return;
  if (app.busy) {
    setNotice({ tone: "error", text: "当前有任务进行中，请等它完成后再拖入导入。" });
    return;
  }
  const valid = filterDroppedPaths(paths);
  if (valid.length === 0) {
    setNotice({
      tone: "error",
      text: `本次拖入的 ${formatCount(paths.length)} 个文件都不受支持：只支持 PNG 图片、文件夹和 zip / 7z / rar 压缩包。`,
    });
    return;
  }
  dropState.paths = valid;
  dropState.ignoredCount = paths.length - valid.length;
  dropState.open = true;
}

export function cancelDropImport(): void {
  if (!dropState.busy) {
    dropState.open = false;
    dropState.paths = [];
    dropState.ignoredCount = 0;
  }
}

export async function confirmDropImport(): Promise<void> {
  if (dropState.busy || dropState.paths.length === 0) return;

  dropState.busy = true;
  try {
    for (const [index, path] of dropState.paths.entries()) {
      dropState.currentIndex = index + 1;
      await runImageImport(path);
    }
  } finally {
    dropState.busy = false;
    dropState.open = false;
    dropState.paths = [];
    dropState.ignoredCount = 0;
    dropState.currentIndex = 0;
  }
}

export function listenDragDrop(): () => void {
  let unlisten: (() => void) | null = null;

  getCurrentWebview()
    .onDragDropEvent((event) => {
      if (!app.snapshot?.dataDirectory) return;

      if (app.viewMode === "promptDocs") {
        dropState.dragging = false;
        if (event.payload.type === "drop" && !app.busy) {
          window.dispatchEvent(
            new CustomEvent<string[]>("prompt-doc-path-drop", {
              detail: event.payload.paths,
            }),
          );
        }
        return;
      }

      if (event.payload.type === "enter" || event.payload.type === "over") {
        if (!outboundDrag) dropState.dragging = true;
      } else if (event.payload.type === "leave") {
        dropState.dragging = false;
      } else if (event.payload.type === "drop") {
        dropState.dragging = false;
        if (!app.busy && !dropState.open && !outboundDrag) {
          requestDropImport(event.payload.paths);
        }
      }
    })
    .then((fn) => {
      unlisten = fn;
    });

  return () => {
    unlisten?.();
  };
}
