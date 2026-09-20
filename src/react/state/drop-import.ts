import { create } from "zustand";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { errorText, formatCount } from "../../lib/utils/format";
import { useLibrary } from "./library";
import { useWorkspace } from "./workspace";
import { useTasks } from "./tasks";
import { outboundDrag } from "./file-drag";
import { runImageImport } from "./import-actions";
import { notify } from "./notices";

const acceptedExtensions = new Set(["zip", "7z", "rar", "png"]);
export const useDropImport = create<{ dragging: boolean; open: boolean; paths: string[]; ignoredCount: number; busy: boolean; currentIndex: number }>(() => ({ dragging: false, open: false, paths: [], ignoredCount: 0, busy: false, currentIndex: 0 }));

export function filterDroppedPaths(paths: string[]): string[] {
  return [...new Set(paths)].filter(path => {
    // Look at the basename only: a dot in an ancestor folder is irrelevant.
    const name = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";
    return Boolean(name) && (!name.includes(".") || acceptedExtensions.has(name.split(".").pop()!.toLowerCase()));
  });
}
export function requestDropImport(paths: string[]): void {
  if (useDropImport.getState().busy || useDropImport.getState().open) return;
  if (useTasks.getState().busy) { notify("当前有任务进行中，请等它完成后再拖入导入。", "error"); return; }
  const unique = [...new Set(paths)];
  const valid = filterDroppedPaths(unique);
  if (!valid.length) { notify(`本次拖入的 ${formatCount(paths.length)} 个文件都不受支持：只支持 PNG 图片、文件夹和 zip / 7z / rar 压缩包。`, "error"); return; }
  useDropImport.setState({ paths: valid, ignoredCount: unique.length - valid.length, open: true, currentIndex: 0 });
}
export function cancelDropImport(): void {
  if (!useDropImport.getState().busy) useDropImport.setState({ open: false, paths: [], ignoredCount: 0 });
}
export async function confirmDropImport(): Promise<void> {
  const state = useDropImport.getState();
  if (state.busy || !state.paths.length || useTasks.getState().busy) return;
  const directory = useLibrary.getState().snapshot?.dataDirectory;
  useDropImport.setState({ busy: true });
  try {
    for (const [index, path] of state.paths.entries()) {
      if (directory !== useLibrary.getState().snapshot?.dataDirectory) { notify("资料库已切换，已停止后续拖入导入。", "error"); break; }
      useDropImport.setState({ currentIndex: index + 1 });
      await runImageImport(path);
    }
  } catch (error) { notify(`拖入导入失败：${errorText(error)}`, "error"); }
  finally { useDropImport.setState({ busy: false, open: false, paths: [], ignoredCount: 0, currentIndex: 0 }); }
}

/** StrictMode-safe native listener; late subscription completion is disposed. */
export function listenDragDrop(): () => void {
  let disposed = false;
  let unlisten: (() => void) | null = null;
  void getCurrentWebview().onDragDropEvent(event => {
    if (disposed || !useLibrary.getState().snapshot?.dataDirectory) return;
    const { type } = event.payload;
    const blocked = useTasks.getState().busy || outboundDrag || useDropImport.getState().open;
    const mode = useWorkspace.getState().viewMode;
    if (type === "leave" || type === "drop") useDropImport.setState({ dragging: false });
    else useDropImport.setState({ dragging: !blocked && mode !== "promptDocs" });
    if (type !== "drop" || blocked) return;
    if (mode === "promptDocs") window.dispatchEvent(new CustomEvent<string[]>("prompt-doc-path-drop", { detail: event.payload.paths }));
    else if (mode === "materials") window.dispatchEvent(new CustomEvent<string[]>("material-path-drop", { detail: event.payload.paths }));
    else requestDropImport(event.payload.paths);
  }).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(error => { if (!disposed) notify(`无法监听拖入文件：${errorText(error)}`, "error"); });
  return () => { disposed = true; unlisten?.(); useDropImport.setState({ dragging: false }); };
}

useLibrary.subscribe((state, previous) => {
  if (state.snapshot?.dataDirectory !== previous.snapshot?.dataDirectory && !useDropImport.getState().busy) cancelDropImport();
});
