import { deleteRows, type RowSelection } from "../api";
import { app, bumpDataVersion, errorText, formatCount, setNotice } from "./app-state.svelte";

export const deletion = $state({
  open: false,
  selection: null as RowSelection | null,
  count: 0,
  trashOriginals: true,
  busy: false,
  error: null as string | null,
});

export function requestDelete(selection: RowSelection, count: number): void {
  if (count <= 0 || deletion.busy) {
    return;
  }
  deletion.selection = selection;
  deletion.count = count;
  deletion.trashOriginals = true;
  deletion.error = null;
  deletion.open = true;
}

export function cancelDelete(): void {
  if (!deletion.busy) {
    deletion.open = false;
    deletion.selection = null;
    deletion.error = null;
  }
}

export async function confirmDelete(): Promise<void> {
  const selection = deletion.selection;
  if (!selection || deletion.busy) {
    return;
  }
  deletion.busy = true;
  deletion.error = null;
  app.busy = true;
  try {
    const result = await deleteRows(selection, deletion.trashOriginals);
    app.snapshot = result.snapshot;
    deletion.open = false;
    deletion.selection = null;
    bumpDataVersion({ preserveScroll: true });

    const parts = [`已删除 ${formatCount(result.deletedRows)} 行`];
    if (deletion.trashOriginals) {
      parts.push(`原图移入回收站 ${formatCount(result.trashedOriginalFiles)} 个`);
      if (result.archiveRowsSkipped > 0) {
        parts.push(`压缩包来源跳过 ${formatCount(result.archiveRowsSkipped)} 行`);
      }
      if (result.originalFileFailures > 0) {
        parts.push(`原图回收失败 ${formatCount(result.originalFileFailures)} 个`);
      }
    }
    if (result.cleanupFailures > 0) {
      parts.push(`受管文件清理失败 ${formatCount(result.cleanupFailures)} 个`);
    }
    setNotice({
      tone:
        result.originalFileFailures > 0 || result.cleanupFailures > 0 ? "error" : "success",
      text: `${parts.join("，")}。`,
    });
  } catch (error) {
    deletion.error = `删除失败：${errorText(error)}`;
  } finally {
    deletion.busy = false;
    app.busy = false;
  }
}
