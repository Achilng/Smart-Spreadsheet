import { useEffect } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getAppSnapshot, getRowIndex, getRowsByIds } from "../../lib/api";
import type { MainStateChange } from "../../lib/windows/library-events";
import { installCloseGuards, registerCloseGuard } from "../../lib/stores/close-guard";
import { installNavigation, navigateHistory } from "../state/navigation";
import { clearHistory, undoLastAction, redoLastAction } from "../state/history";
import { clearSelection, selectedCount, selectionDto, useSelection } from "../state/selection";
import { selectAllCurrentView } from "../state/view-selection";
import { useTasks } from "../state/tasks";
import { useWorkspace } from "../state/workspace";
import { notify } from "../state/notices";
import { errorText } from "../../lib/utils/format";
import { hasFieldDrafts } from "./FieldEditor";
import { defaultFilters, reloadRows, useLibrary, useRows } from "../state/library";
import { refreshLibrary } from "../state/library-changes";
import { connectLibrary, runStartupMaintenance, useMaintenance } from "../state/library-session";
import { listenDragDrop } from "../state/drop-import";
import { cancelPendingFileDrag } from "../state/file-drag";
import { requestDelete } from "../state/row-actions";
import { captureScrollSnapshot } from "../../lib/stores/view-state";
import { galleryCellPosition, galleryLayout } from "../../lib/images/gallery-layout";
import { syncGroups } from "../state/groups";
import { syncDuplicates } from "../state/duplicates";

async function revealRow(rowId: number): Promise<void> {
  const directory = useLibrary.getState().snapshot?.dataDirectory;
  const sort = useRows.getState().query.sort;
  const [rows, index] = await Promise.all([getRowsByIds([rowId]), getRowIndex(rowId, sort)]);
  if (directory !== useLibrary.getState().snapshot?.dataDirectory) return;
  if (!rows[0] || index < 0) throw new Error("图片记录已不存在");
  useWorkspace.setState({ viewMode: "gallery", detailOpen: true });
  const width = document.querySelector(".r-main-area")?.clientWidth ?? 600;
  const top = galleryCellPosition(index, galleryLayout(width, useWorkspace.getState().galleryCardSize, Math.max(index + 1, useRows.getState().total))).y;
  const scroll = captureScrollSnapshot();
  scroll.positions = [["gallery", Math.max(0, top - 16)]];
  scroll.ranges = [["gallery", { first: index, last: index }]];
  scroll.unfilteredPositions = null; scroll.unfilteredRanges = null; scroll.filtered = false;
  useRows.setState({ query: { ...defaultFilters, sort }, activeRow: rows[0] }); clearSelection();
  await reloadRows({ navigation: scroll, keepActive: true });
}

export function useWorkspaceLifecycle(): void {
  useEffect(() => {
    let disposed = false;
    let stopClose: (() => void) | undefined;
    const cleanups: (() => void)[] = [];
    const retain = (promise: Promise<() => void>) => { void promise.then(stop => { if (disposed) stop(); else cleanups.push(stop); }).catch(error => { if (!disposed) notify(`窗口同步不可用：${errorText(error)}`, "error"); }); };
    const stopNavigation = installNavigation();
    const stopGuard = registerCloseGuard(() => [useTasks.getState().busy ? `${useTasks.getState().label}正在进行中` : "", useMaintenance.getState().label ? "历史资料索引尚未完成，下次启动可继续" : "", hasFieldDrafts() ? "有未保存的图片编辑草稿" : ""].filter(Boolean).join("；") || null);
    const shareSelection = () => { void emitTo("toolbox", "main://selection-changed", { selection: selectionDto(), count: selectedCount(useSelection.getState()) }).catch(() => {}); };
    cleanups.push(useSelection.subscribe((next, previous) => { if (next.version !== previous.version) shareSelection(); }));
    retain(listen("toolbox://request-selection", shareSelection));
    retain(listen<{ rowId: number }>("toolbox://open-row", event => { void revealRow(event.payload.rowId).catch(error => notify(`无法定位图片：${errorText(error)}`, "error")); }));
    // Serialize notifications so a slow refresh cannot overwrite a later switch.
    let updates = Promise.resolve();
    retain(listen<MainStateChange>("toolbox://app-state-changed", event => {
      updates = updates.then(async () => {
        if (disposed) return;
        clearHistory();
        if (event.payload === "libraryEdited") await refreshLibrary({ origin: "toolbox" });
        else { await connectLibrary(await getAppSnapshot()); void runStartupMaintenance(); notify(event.payload === "reset" ? "表格已重置，请重新导入数据。" : "数据目录已迁移，主窗口已重新连接。"); }
      }).catch(error => notify(`刷新资料库失败：${errorText(error)}`, "error"));
    }));
    cleanups.push(listenDragDrop());
    const syncSections = () => { const snapshot = useLibrary.getState().snapshot; if (snapshot?.dataDirectory && !snapshot.startupError) { void syncGroups(); syncDuplicates(); } };
    cleanups.push(useRows.subscribe((next, previous) => { if (next.resetToken !== previous.resetToken) syncSections(); }));
    cleanups.push(useLibrary.subscribe((next, previous) => { if (next.snapshot?.dataDirectory !== previous.snapshot?.dataDirectory) syncSections(); }));
    void installCloseGuards().then(stop => { if (disposed) stop(); else stopClose = stop; }).catch(error => { if (!disposed) notify(`无法安装窗口关闭保护：${errorText(error)}`, "error"); });
    const beforeUnload = (event: BeforeUnloadEvent) => { if (hasFieldDrafts() || useTasks.getState().busy) { event.preventDefault(); event.returnValue = ""; } };
    const keydown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest('input, textarea, select, [contenteditable="true"]') || event.isComposing || document.querySelector('[role="dialog"]')) return;
      if (event.altKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) { event.preventDefault(); if (!event.repeat) void navigateHistory(event.key === "ArrowLeft" ? -1 : 1); return; }
      if (["materials", "promptDocs"].includes(useWorkspace.getState().viewMode)) return;
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") { event.preventDefault(); void (event.shiftKey ? redoLastAction() : undoLastAction()); }
      else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "y") { event.preventDefault(); void redoLastAction(); }
      else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a") {
        event.preventDefault(); void selectAllCurrentView().catch(error => notify(`选择失败：${errorText(error)}`, "error"));
      } else if (event.key === "Delete" && !target?.closest('button, [role="menu"], [role="listbox"]')) {
        const count = selectedCount(useSelection.getState()), row = useRows.getState().activeRow;
        if (count || row) { event.preventDefault(); void requestDelete(count ? selectionDto() : { kind: "explicit", rowIds: [row!.id] }, count || 1); }
      }
      else if (event.key === "Escape") clearSelection();
    };
    const mouse = (event: MouseEvent) => { if (event.button === 3 || event.button === 4) { event.preventDefault(); if (event.type === "mouseup") void navigateHistory(event.button === 3 ? -1 : 1); } };
    const contextmenu = (event: MouseEvent) => { if (!(event.target instanceof Element && event.target.closest('input, textarea, [contenteditable="true"]'))) event.preventDefault(); };
    window.addEventListener("beforeunload", beforeUnload); window.addEventListener("keydown", keydown);
    for (const name of ["mousedown", "mouseup", "auxclick"] as const) window.addEventListener(name, mouse);
    window.addEventListener("contextmenu", contextmenu);
    return () => { disposed = true; stopClose?.(); stopGuard(); stopNavigation(); cleanups.forEach(stop => stop()); cancelPendingFileDrag(); window.removeEventListener("beforeunload", beforeUnload); window.removeEventListener("keydown", keydown); window.removeEventListener("contextmenu", contextmenu); for (const name of ["mousedown", "mouseup", "auxclick"] as const) window.removeEventListener(name, mouse); };
  }, []);
}
