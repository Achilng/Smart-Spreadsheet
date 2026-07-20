<script lang="ts">
  import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, untrack } from "svelte";

  import { getRowIndex, getRowsByIds } from "../../api";
  import { app, errorText, setNotice, type ViewMode } from "../../stores/app-state.svelte";
  import { deletion, requestDelete } from "../../stores/delete-actions.svelte";
  import { dropState, listenDragDrop } from "../../stores/drop-import.svelte";
  import { redoLastAction, undoLastAction } from "../../stores/history.svelte";
  import {
    clearSelection,
    getSelectedCount,
    selectAllFiltered,
    selection,
    selectionDto,
  } from "../../stores/selection-store.svelte";
  import {
    resetRows,
    revealRowInGallery,
    rowStore,
  } from "../../stores/row-store.svelte";
  import { loadTags } from "../../stores/tag-store.svelte";
  import { thumbnails } from "../../images/thumbnails";
  import { clearProgressiveImages } from "../../images/progressive-images";
  import { vibeStatuses } from "../../images/vibe-statuses";
  import type {
    ToolboxRowRequest,
    ToolboxSelectionSnapshot,
  } from "../../windows/toolbox";
  import ContextMenu from "./ContextMenu.svelte";
  import DetailPanel from "./DetailPanel.svelte";
  import DeleteDialog from "./DeleteDialog.svelte";
  import DropConfirmDialog from "./DropConfirmDialog.svelte";
  import DuplicateBrowseView from "../duplicates/DuplicateBrowseView.svelte";
  import GalleryView from "../gallery/GalleryView.svelte";
  import GroupBrowseView from "../groups/GroupBrowseView.svelte";
  import GroupManageView from "../groups/GroupManageView.svelte";
  import NavRail from "./NavRail.svelte";
  import PromptDocsView from "../prompt-docs/PromptDocsView.svelte";
  import SectionContextMenu from "./SectionContextMenu.svelte";
  import SelectionBar from "./SelectionBar.svelte";
  import TableView from "../table/TableView.svelte";
  import TagSidebar from "./TagSidebar.svelte";
  import TopBar from "./TopBar.svelte";
  import UpdateImportDialog from "./UpdateImportDialog.svelte";
  import { softFade, softFly, softPop } from "../../ui/motion";

  type DataViewMode = Exclude<ViewMode, "promptDocs">;

  const visitedViews = $state<Record<ViewMode, boolean>>({
    group: false,
    gallery: true,
    table: false,
    duplicates: false,
    promptDocs: false,
  });

  function isDataViewMode(mode: ViewMode): mode is DataViewMode {
    return mode !== "promptDocs";
  }

  function rememberVisitedView(mode: ViewMode): void {
    visitedViews[mode] = true;
  }

  $effect(() => {
    rememberVisitedView(app.viewMode);
  });

  // 首次挂载和工作簿替换时：清缩略图缓存（行 ID 可能复用）、重载数据、刷新 Tag 库、清空选择。
  // untrack：这些调用内部有”读-改-写”（如 pagesVersion += 1），不能注册为本 effect 的依赖。
  $effect(() => {
    void app.dataVersion;
    const preserveScroll = app.preserveScrollOnDataChange;
    untrack(() => {
      thumbnails.clear();
      clearProgressiveImages();
      vibeStatuses.clear();
      resetRows({ keepStale: preserveScroll, resetScroll: !preserveScroll });
      clearSelection();
      void loadTags();
    });
  });

  async function shareSelectionWithToolbox(): Promise<void> {
    const snapshot: ToolboxSelectionSnapshot = {
      selection: selectionDto(),
      count: getSelectedCount(),
    };
    try {
      await emitTo("toolbox", "main://selection-changed", snapshot);
    } catch {
      // 工具箱尚未打开时无需同步；打开后会主动请求一次最新选区。
    }
  }

  $effect(() => {
    void selection.version;
    untrack(() => {
      void shareSelectionWithToolbox();
    });
  });

  async function openToolboxRow(request: ToolboxRowRequest): Promise<void> {
    try {
      const [row] = await getRowsByIds([request.rowId]);
      if (!row) {
        throw new Error("图片记录已不存在");
      }

      const index = await getRowIndex(request.rowId);
      revealRowInGallery(row, index);
      app.viewMode = "gallery";
      clearSelection();
    } catch (error) {
      setNotice({
        tone: "error",
        text: `无法打开搜索结果：${errorText(error)}`,
      });
    }
  }

  onMount(() => {
    const stopDragDrop = listenDragDrop();
    let disposed = false;
    let unlistenNavigation: UnlistenFn | null = null;
    let unlistenSelectionRequest: UnlistenFn | null = null;
    void listen<ToolboxRowRequest>("toolbox://open-row", event => {
      void openToolboxRow(event.payload);
    }).then(unlisten => {
      if (disposed) {
        unlisten();
      } else {
        unlistenNavigation = unlisten;
      }
    });
    void listen("toolbox://request-selection", () => {
      void shareSelectionWithToolbox();
    }).then(unlisten => {
      if (disposed) {
        unlisten();
      } else {
        unlistenSelectionRequest = unlisten;
      }
    });

    return () => {
      disposed = true;
      stopDragDrop();
      unlistenNavigation?.();
      unlistenSelectionRequest?.();
    };
  });

  function onKeydown(event: KeyboardEvent): void {
    const target = event.target;
    const isTextEditing =
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLElement && target.isContentEditable);
    const isEditing =
      isTextEditing ||
      target instanceof HTMLSelectElement ||
      target instanceof HTMLButtonElement;

    // 输入控件内保留浏览器原生文字撤销；其他位置走资料库操作历史。
    if (
      (event.ctrlKey || event.metaKey) &&
      !event.altKey &&
      !isTextEditing &&
      !deletion.open
    ) {
      const key = event.key.toLowerCase();
      if (key === "z") {
        event.preventDefault();
        if (event.shiftKey) {
          void redoLastAction();
        } else {
          void undoLastAction();
        }
        return;
      }
      if (key === "y" && !event.shiftKey) {
        event.preventDefault();
        void redoLastAction();
        return;
      }
    }

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
  <NavRail />
  <div class="workspace-inner">
    <TopBar />
    {#if visitedViews.promptDocs}
      <div
        class="workspace-body prompt-docs-body"
        class:is-active={app.viewMode === "promptDocs"}
        aria-hidden={app.viewMode !== "promptDocs"}
      >
        <main class="prompt-docs-main">
          <PromptDocsView />
        </main>
      </div>
    {/if}

    <div
      class="workspace-body data-body"
      class:is-active={isDataViewMode(app.viewMode)}
      aria-hidden={!isDataViewMode(app.viewMode)}
    >
      <aside class="sidebar">
        <TagSidebar />
      </aside>

      <main class="main-area">
        {#if rowStore.refreshing}
          <div class="refresh-bar" aria-hidden="true"></div>
        {/if}
        <div class="view-stack">
          {#if visitedViews.group}
            <section class="view-panel" class:is-active={app.viewMode === "group"}>
              <GroupBrowseView active={app.viewMode === "group"} />
            </section>
          {/if}
          {#if visitedViews.duplicates}
            <section class="view-panel" class:is-active={app.viewMode === "duplicates"}>
              <DuplicateBrowseView active={app.viewMode === "duplicates"} />
            </section>
          {/if}
          {#if visitedViews.gallery}
            <section class="view-panel" class:is-active={app.viewMode === "gallery"}>
              <GalleryView active={app.viewMode === "gallery"} />
            </section>
          {/if}
          {#if visitedViews.table}
            <section class="view-panel" class:is-active={app.viewMode === "table"}>
              <TableView active={app.viewMode === "table"} />
            </section>
          {/if}
        </div>
        <SelectionBar />
      </main>

      {#if app.detailOpen}
        <aside class="detail" transition:softFly={{ duration: 190, x: 12, y: 0 }}>
          <DetailPanel />
        </aside>
      {:else}
        <button
          type="button"
          class="detail-strip"
          title="展开详情面板"
          onclick={() => (app.detailOpen = true)}
          transition:softFade={{ duration: 120 }}
        >
          «
        </button>
      {/if}
    </div>
  </div>
</div>

<GroupManageView />

<ContextMenu />
<SectionContextMenu />
<DeleteDialog />
<DropConfirmDialog />
<UpdateImportDialog />

{#if dropState.dragging && app.viewMode !== "promptDocs"}
  <div class="drop-overlay" transition:softFade={{ duration: 120 }}>
    <div class="drop-hint" transition:softPop={{ duration: 150, y: 4, start: 0.98 }}>
      松开鼠标以导入图片
    </div>
  </div>
{/if}

<style>
  .workspace {
    height: 100%;
    display: flex;
    flex-direction: row;
  }

  .workspace-inner {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .workspace-body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  .workspace-body:not(.is-active) {
    display: none;
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

  .view-stack {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }

  .view-panel {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: none;
  }

  .view-panel.is-active {
    display: flex;
  }

  /* 筛选/搜索刷新中的细进度条：旧内容保持显示，仅顶部提示加载中 */
  .refresh-bar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    z-index: var(--z-nav);
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
    transform: translateX(-100%);
    animation: refresh-slide 0.9s linear infinite;
  }

  @keyframes refresh-slide {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(350%);
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
    font-size: var(--font-md);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive);
  }

  .detail-strip:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .drop-overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-dragdrop);
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
    font-size: var(--font-xl);
    font-weight: 600;
    color: var(--accent, #5b9ef4);
  }

  @media (prefers-reduced-motion: reduce) {
    .refresh-bar::after {
      animation: none;
      transform: translateX(80%);
    }
  }

</style>
