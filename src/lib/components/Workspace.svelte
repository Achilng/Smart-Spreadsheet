<script lang="ts">
  import { onMount, untrack } from "svelte";

  import { app } from "../app-state.svelte";
  import { deletion, requestDelete } from "../delete-actions.svelte";
  import { dropState, listenDragDrop } from "../drop-import.svelte";
  import {
    clearSelection,
    getSelectedCount,
    selectAllFiltered,
    selectionDto,
  } from "../selection-store.svelte";
  import { resetRows, rowStore } from "../row-store.svelte";
  import { loadTags } from "../tag-store.svelte";
  import { thumbnails } from "../thumbnails";
  import ContextMenu from "./ContextMenu.svelte";
  import DetailPanel from "./DetailPanel.svelte";
  import DeleteDialog from "./DeleteDialog.svelte";
  import DropConfirmDialog from "./DropConfirmDialog.svelte";
  import AlbumBrowseView from "./AlbumBrowseView.svelte";
  import ArtistGeneratorView from "./ArtistGeneratorView.svelte";
  import DuplicateBrowseView from "./DuplicateBrowseView.svelte";
  import GalleryView from "./GalleryView.svelte";
  import GroupBrowseView from "./GroupBrowseView.svelte";
  import GroupManageView from "./GroupManageView.svelte";
  import GroupSuggestionView from "./GroupSuggestionView.svelte";
  import JsonDedupeView from "./JsonDedupeView.svelte";
  import PromptDocsView from "./PromptDocsView.svelte";
  import SearchResultsView from "./SearchResultsView.svelte";
  import SectionContextMenu from "./SectionContextMenu.svelte";
  import SelectionBar from "./SelectionBar.svelte";
  import TableView from "./TableView.svelte";
  import TagSidebar from "./TagSidebar.svelte";
  import TopBar from "./TopBar.svelte";

  // 首次挂载和工作簿替换时：清缩略图缓存（行 ID 可能复用）、重载数据、刷新 Tag 库、清空选择。
  // untrack：这些调用内部有”读-改-写”（如 pagesVersion += 1），不能注册为本 effect 的依赖。
  $effect(() => {
    void app.dataVersion;
    untrack(() => {
      thumbnails.clear();
      resetRows({ keepStale: false, resetScroll: true });
      clearSelection();
      void loadTags();
    });
  });

  onMount(() => listenDragDrop());

  function onKeydown(event: KeyboardEvent): void {
    const target = event.target;
    const isEditing =
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target instanceof HTMLSelectElement ||
      target instanceof HTMLButtonElement ||
      (target instanceof HTMLElement && target.isContentEditable);

    // Ctrl+A 全选筛选结果（输入框内除外）
    if (
      event.key.toLowerCase() === "a" &&
      (event.ctrlKey || event.metaKey) &&
      app.viewMode !== "promptDocs" &&
      !(event.target instanceof HTMLInputElement) &&
      !(event.target instanceof HTMLTextAreaElement)
    ) {
      event.preventDefault();
      if (rowStore.totalCount > 0) {
        void selectAllFiltered();
      }
      return;
    }

    if (event.key === "Delete" && app.viewMode !== "promptDocs" && !isEditing && !deletion.open) {
      const selectedCount = getSelectedCount();
      if (selectedCount > 0) {
        event.preventDefault();
        requestDelete(selectionDto(), selectedCount);
      } else if (rowStore.activeRow) {
        event.preventDefault();
        requestDelete({ kind: "explicit", rowIds: [rowStore.activeRow.id] }, 1);
      }
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="workspace">
  <TopBar />
  {#if app.viewMode === "promptDocs"}
    <div class="workspace-body prompt-docs-body">
      <main class="prompt-docs-main">
        <PromptDocsView />
      </main>
    </div>
  {:else}
    <div class="workspace-body">
      <aside class="sidebar">
        <TagSidebar />
      </aside>

      <main class="main-area">
        {#if rowStore.refreshing}
          <div class="refresh-bar" aria-hidden="true"></div>
        {/if}
        {#if app.viewMode === "group"}
          <GroupBrowseView />
        {:else if app.viewMode === "albums"}
          <AlbumBrowseView />
        {:else if app.viewMode === "duplicates"}
          <DuplicateBrowseView />
        {:else if app.viewMode === "gallery"}
          <GalleryView />
        {:else}
          <TableView />
        {/if}
        <SelectionBar />
      </main>

      {#if app.detailOpen}
        <aside class="detail">
          <DetailPanel />
        </aside>
      {:else}
        <button
          type="button"
          class="detail-strip"
          title="展开详情面板"
          onclick={() => (app.detailOpen = true)}
        >
          «
        </button>
      {/if}
    </div>
  {/if}
</div>

{#if app.jsonDedupeOpen}
  <JsonDedupeView />
{/if}

{#if app.artistGenOpen}
  <ArtistGeneratorView />
{/if}

{#if app.groupSuggestOpen}
  <GroupSuggestionView />
{/if}

{#if app.groupManageOpen}
  <GroupManageView />
{/if}

<SearchResultsView />
<ContextMenu />
<SectionContextMenu />
<DeleteDialog />
<DropConfirmDialog />

{#if dropState.dragging && app.viewMode !== "promptDocs"}
  <div class="drop-overlay">
    <div class="drop-hint">松开鼠标以导入图片</div>
  </div>
{/if}

<style>
  .workspace {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .workspace-body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  .sidebar {
    width: 240px;
    flex: none;
    background: var(--surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .main-area {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    position: relative;
  }

  /* 筛选/搜索刷新中的细进度条：旧内容保持显示，仅顶部提示加载中 */
  .refresh-bar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    z-index: 30;
    overflow: hidden;
    background: transparent;
    pointer-events: none;
  }

  .refresh-bar::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    width: 40%;
    border-radius: 2px;
    background: var(--accent);
    animation: refresh-slide 0.9s ease-in-out infinite;
  }

  @keyframes refresh-slide {
    0% {
      left: -40%;
    }
    100% {
      left: 100%;
    }
  }

  .prompt-docs-body {
    background: var(--bg);
  }

  .prompt-docs-main {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }

  .detail {
    width: 340px;
    flex: none;
    background: var(--surface);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .detail-strip {
    width: 22px;
    flex: none;
    border: none;
    border-left: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-3);
    font-size: 13px;
  }

  .detail-strip:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .drop-overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    background: rgb(15 20 28 / 45%);
    pointer-events: none;
  }

  .drop-hint {
    padding: 20px 40px;
    background: var(--surface);
    border: 2px dashed var(--accent, #5b9ef4);
    border-radius: var(--radius-m);
    font-size: 18px;
    font-weight: 600;
    color: var(--accent, #5b9ef4);
  }

</style>
