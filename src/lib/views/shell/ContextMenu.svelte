<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import { exportRowImage, rowIdsWithArtists, showItemInExplorer } from "../../api";
  import { formatCount, setNotice } from "../../stores/app-state.svelte";
  import { contextMenu, hideContextMenu } from "../../stores/context-menu.svelte";
  import { requestDelete } from "../../stores/delete-actions.svelte";
  import {
    getSelectedCount,
    selectionDto,
    setExplicitSelection,
  } from "../../stores/selection-store.svelte";

  const row = $derived(contextMenu.row);
  const hasPrompt = $derived(Boolean(row?.positivePrompt?.trim()));
  const hasArtists = $derived(Boolean(row?.artists?.trim()));
  const hasImage = $derived(
    Boolean(row && (row.imagePath?.trim() || row.storedImagePath?.trim())),
  );

  function handlePointerDown(event: PointerEvent): void {
    if (contextMenu.open && menuEl && !menuEl.contains(event.target as Node)) {
      hideContextMenu();
    }
  }

  let menuEl = $state<HTMLDivElement | null>(null);

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

<svelte:window
  onpointerdown={handlePointerDown}
  onkeydown={event => {
    if (event.key === "Escape" && contextMenu.open) {
      hideContextMenu();
    }
  }}
/>

{#if contextMenu.open && row}
  <div
    class="context-menu"
    bind:this={menuEl}
    role="menu"
    style:left="{contextMenu.x}px"
    style:top="{contextMenu.y}px"
  >
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
  </div>
{/if}

<style>
  .context-menu {
    position: fixed;
    z-index: var(--z-menu);
    min-width: 180px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-2);
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .context-menu button {
    display: flex;
    align-items: center;
    width: 100%;
    border: none;
    background: transparent;
    border-radius: var(--radius-s);
    padding: 6px 10px;
    text-align: left;
    font-size: var(--font-md);
    color: var(--text);
    cursor: default;
  }

  .context-menu button:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .context-menu button:disabled {
    color: var(--text-3);
    cursor: not-allowed;
  }

  .context-menu button.danger {
    color: var(--danger);
  }

  .context-menu button.danger:disabled {
    color: var(--text-3);
  }

  .separator {
    height: 1px;
    background: var(--border);
    margin: 3px 4px;
  }
</style>
