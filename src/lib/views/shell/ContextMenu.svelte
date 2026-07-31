<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import { exportRowImage, rowIdsWithArtists, showItemInExplorer } from "../../api";
  import ContextMenuShell from "../../ui/ContextMenuShell.svelte";
  import { app, formatCount, setNotice } from "../../stores/app-state.svelte";
  import { contextMenu, hideContextMenu } from "../../stores/context-menu.svelte";
  import { requestDelete } from "../../stores/delete-actions.svelte";
  import {
    clearSelection,
    getSelectedCount,
    selectionDto,
    setExplicitSelection,
  } from "../../stores/selection-store.svelte";
  import { focusArtistFilter } from "../../stores/row-store.svelte";

  const row = $derived(contextMenu.row);
  const hasPrompt = $derived(Boolean(row?.positivePrompt?.trim()));
  const hasArtists = $derived(Boolean(row?.artists?.trim()));
  const hasImage = $derived(
    Boolean(row && (row.imagePath?.trim() || row.storedImagePath?.trim())),
  );

  async function copyPrompt(): Promise<void> {
    if (!row?.positivePrompt) return;
    hideContextMenu();
    try {
      await navigator.clipboard.writeText(row.positivePrompt);
      setNotice({ tone: "success", text: "已复制 Prompt 到剪贴板。" });
    } catch {
      setNotice({ tone: "error", text: "复制失败，请检查剪贴板权限。" });
    }
  }

  async function exportImage(): Promise<void> {
    if (!row) return;
    hideContextMenu();
    const originalName =
      row.imagePath
        ?.split(/[\\/]/)
        .pop()
        ?.replace(/[<>:"|?*]/g, "_") ?? "image.png";
    const destination = await save({
      title: "导出原图",
      defaultPath: originalName,
      filters: [{ name: "图片文件", extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }],
    });
    if (typeof destination !== "string") return;
    try {
      await exportRowImage(row.id, destination);
      setNotice({ tone: "success", text: `已导出原图到 ${destination}` });
    } catch (error) {
      setNotice({
        tone: "error",
        text: `导出失败：${error instanceof Error ? error.message : String(error)}`,
      });
    }
  }

  async function openInExplorer(): Promise<void> {
    if (!row) return;
    hideContextMenu();
    try {
      await showItemInExplorer(row.id);
    } catch (error) {
      setNotice({
        tone: "error",
        text: `打开文件管理器失败：${error instanceof Error ? error.message : String(error)}`,
      });
    }
  }

  async function selectSameArtists(): Promise<void> {
    const artists = row?.artists?.trim();
    if (!artists) return;
    hideContextMenu();
    try {
      const ids = await rowIdsWithArtists(artists);
      setExplicitSelection(ids);
      setNotice({
        tone: "success",
        text: `已选中 ${formatCount(ids.length)} 张相同画师串的图。`,
      });
    } catch (error) {
      setNotice({
        tone: "error",
        text: `选择失败：${error instanceof Error ? error.message : String(error)}`,
      });
    }
  }

  function showSameArtists(): void {
    const artists = row?.artists?.trim();
    if (!artists) return;
    hideContextMenu();
    clearSelection();
    focusArtistFilter(artists);
    if (app.viewMode !== "gallery" && app.viewMode !== "table") {
      app.viewMode = "gallery";
    }
  }

  function deleteRow(): void {
    if (!row) return;
    hideContextMenu();
    const count = getSelectedCount();
    if (count > 0) {
      requestDelete(selectionDto(), count);
    } else {
      requestDelete({ kind: "explicit", rowIds: [row.id] }, 1);
    }
  }
</script>

{#if row}
  <ContextMenuShell open={contextMenu.open} x={contextMenu.x} y={contextMenu.y} onclose={hideContextMenu}>
    <button
      type="button"
      role="menuitem"
      disabled={!hasPrompt}
      onclick={() => void copyPrompt()}
    >
      复制 Prompt
    </button>
    <button
      type="button"
      role="menuitem"
      disabled={!hasImage}
      onclick={() => void exportImage()}
    >
      导出原图
    </button>
    <button
      type="button"
      role="menuitem"
      disabled={!hasImage}
      onclick={() => void openInExplorer()}
    >
      在文件管理器中打开
    </button>
    <div class="separator"></div>
    <button
      type="button"
      role="menuitem"
      disabled={!hasArtists}
      onclick={showSameArtists}
    >
      只看当前画师串
    </button>
    <button
      type="button"
      role="menuitem"
      disabled={!hasArtists}
      onclick={() => void selectSameArtists()}
    >
      选中相同画师串
    </button>
    <div class="separator"></div>
    <button
      type="button"
      role="menuitem"
      class="danger"
      onclick={deleteRow}
    >
      {getSelectedCount() > 0 ? `删除已选 ${getSelectedCount()} 行` : "删除"}
    </button>
  </ContextMenuShell>
{/if}
