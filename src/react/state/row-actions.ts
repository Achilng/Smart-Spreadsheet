import { create } from "zustand";
import { save } from "@tauri-apps/plugin-dialog";
import { deleteRows, exportRowImage, rowIdsWithArtists, selectedRowIds, showItemInExplorer, type RowRecord, type RowSelection } from "../../lib/api";
import { errorText, formatCount } from "../../lib/utils/format";
import { defaultFilters, setQuery, useLibrary, useRows } from "./library";
import { refreshLibrary } from "./library-changes";
import { clearHistory } from "./history";
import { notify } from "./notices";
import { clearSelection, setExplicitSelection } from "./selection";
import { runTask, useTasks } from "./tasks";
import { useWorkspace } from "./workspace";

type DialogKind = "tags" | "group" | "prompt" | "delete";
export interface RowActionRequest { kind: DialogKind; selection: RowSelection; count: number; serial: number }
export const useRowActions = create<{ request: RowActionRequest | null; preparing: boolean; deleting: boolean; trashOriginals: boolean; error: string | null }>(() => ({ request: null, preparing: false, deleting: false, trashOriginals: true, error: null }));
let serial = 0;

async function requestAction(kind: DialogKind, selection: RowSelection, count: number): Promise<void> {
  if (count <= 0 || useRowActions.getState().preparing || useRowActions.getState().deleting) return;
  if (useTasks.getState().busy) { notify("当前有任务进行中，请稍后重试。", "error"); return; }
  useRowActions.setState({ preparing: true });
  const directory = useLibrary.getState().snapshot?.dataDirectory;
  try {
    // Freeze filtered selections before edits can change the filter's own tags,
    // artists, or prompt text. Dialog targets never follow later UI selection.
    const ids = selection.kind === "explicit" ? [...selection.rowIds] : await selectedRowIds(structuredClone(selection));
    if (directory !== useLibrary.getState().snapshot?.dataDirectory) throw new Error("资料库已切换，请重新选择要操作的图片。");
    if (!ids.length) { notify("所选图片已不存在，请重新选择。", "error"); return; }
    const unique = [...new Set(ids)];
    useRowActions.setState({ request: { kind, selection: { kind: "explicit", rowIds: unique }, count: unique.length, serial: ++serial }, trashOriginals: true, error: null });
  } catch (error) { notify(`准备操作失败：${errorText(error)}`, "error"); }
  finally { useRowActions.setState({ preparing: false }); }
}

export const requestTagEdit = (selection: RowSelection, count: number) => requestAction("tags", selection, count);
export const requestGroupAssign = (selection: RowSelection, count: number) => requestAction("group", selection, count);
export const requestPromptEdit = (selection: RowSelection, count: number) => requestAction("prompt", selection, count);
export const requestDelete = (selection: RowSelection, count: number) => requestAction("delete", selection, count);
export function closeRowAction(): void { if (!useRowActions.getState().deleting && !useTasks.getState().busy) useRowActions.setState({ request: null, error: null }); }

export async function confirmDelete(): Promise<void> {
  const state = useRowActions.getState();
  if (!state.request || state.request.kind !== "delete" || state.deleting) return;
  useRowActions.setState({ deleting: true, error: null });
  try {
    await runTask("删除图片资料", async () => {
      const result = await deleteRows(state.request!.selection, state.trashOriginals);
      // Deletion has committed. Close before refreshing so a failed reread can
      // never invite submitting a completed destructive operation again.
      useRowActions.setState({ request: null });
      useLibrary.setState({ snapshot: result.snapshot });
      clearHistory();
      const parts = [`已删除 ${formatCount(result.deletedRows)} 行`];
      if (state.trashOriginals) {
        parts.push(`原图移入回收站 ${formatCount(result.trashedOriginalFiles)} 个`);
        if (result.archiveRowsSkipped) parts.push(`压缩包来源跳过 ${formatCount(result.archiveRowsSkipped)} 行`);
        if (result.originalFileFailures) parts.push(`原图回收失败 ${formatCount(result.originalFileFailures)} 个`);
      }
      if (result.cleanupFailures) parts.push(`受管文件清理失败 ${formatCount(result.cleanupFailures)} 个`);
      notify(`${parts.join("，")}。`, result.originalFileFailures || result.cleanupFailures ? "error" : "success");
      try { await refreshLibrary({ snapshot: result.snapshot }); }
      catch (error) { notify(`删除已完成，但刷新资料库失败：${errorText(error)}`, "error"); }
    });
  } catch (error) { useRowActions.setState({ error: `删除失败：${errorText(error)}` }); }
  finally { useRowActions.setState({ deleting: false }); }
}

export async function copyRowPrompt(row: RowRecord): Promise<void> {
  if (!row.positivePrompt) return;
  try { await navigator.clipboard.writeText(row.positivePrompt); notify("已复制 Prompt 到剪贴板。"); }
  catch { notify("复制失败，请检查剪贴板权限。", "error"); }
}
export async function exportOriginalImage(row: RowRecord): Promise<void> {
  try {
    const originalName = (row.imagePath || row.storedImagePath)?.split(/[\\/]/).pop()?.replace(/[<>:"|?*]/g, "_") || "image.png";
    const path = await save({ title: "导出原图", defaultPath: originalName, filters: [{ name: "图片文件", extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }] });
    if (typeof path !== "string") return;
    await runTask("导出原图", () => exportRowImage(row.id, path));
    notify(`已导出原图到 ${path}`);
  } catch (error) { notify(`导出失败：${errorText(error)}`, "error"); }
}
export async function showRowInExplorer(row: RowRecord): Promise<void> {
  try { await showItemInExplorer(row.id); }
  catch (error) { notify(`打开文件管理器失败：${errorText(error)}`, "error"); }
}
export function filterByArtists(artists: string): void {
  const normalized = artists.trim();
  if (!normalized) return;
  clearSelection();
  const query = useRows.getState().query;
  setQuery({ ...defaultFilters, sort: query.sort, tagMode: query.tagMode, artistFilter: normalized });
  if (!["gallery", "table"].includes(useWorkspace.getState().viewMode)) useWorkspace.getState().setView("gallery");
}
export async function selectSameArtists(artists: string): Promise<void> {
  if (!artists.trim()) return;
  const directory = useLibrary.getState().snapshot?.dataDirectory;
  try {
    const ids = await rowIdsWithArtists(artists.trim());
    if (directory !== useLibrary.getState().snapshot?.dataDirectory) return;
    setExplicitSelection(ids);
    notify(`已选中 ${formatCount(ids.length)} 张相同画师串的图。`);
  } catch (error) { notify(`选择失败：${errorText(error)}`, "error"); }
}

useLibrary.subscribe((state, previous) => {
  if (state.snapshot?.dataDirectory !== previous.snapshot?.dataDirectory && !useRowActions.getState().deleting) {
    useRowActions.setState({ request: null, error: null });
  }
});
