<script lang="ts">
  import { errorText } from "../../utils/format";
  import { setNotice } from "../../stores/notices.svelte";
  import { type ViewMode, workspaceState } from "../../stores/workspace-state.svelte";
  import { libraryState } from "../../stores/library-state.svelte";
  import { taskState } from "../../stores/task-state.svelte";
  import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
  import DetailSidebar from "../../ui/DetailSidebar.svelte";
  import { onDestroy, onMount, untrack } from "svelte";
  import {
    finishNavigationRestore, installNavigation, navigateHistory, navigation,
    navigationRoute, observeNavigation,
  } from "../../stores/navigation.svelte";

  import { getRowIndex, getRowsByIds } from "../../api";

  import { materialGalleryPicker, finishMaterialGalleryPick } from "../../stores/material-gallery-picker.svelte";
  import { rowFileName } from "../../utils/row-display";
  import { deletion, requestDelete } from "../../stores/delete-actions.svelte";
  import { dropState, listenDragDrop } from "../../stores/drop-import.svelte";
  import { redoLastAction, undoLastAction } from "../../stores/history.svelte";
  import {
    clearSelection,
    getSelectedCount,
    selection,
    selectionDto,
  } from "../../stores/selection-store.svelte";
  import { selectAllCurrentView } from "../../stores/view-selection.svelte";
  import {
    resetRows,
    revealRowInGallery,
    rowStore,
  } from "../../stores/row-store.svelte";
  import { loadTags } from "../../stores/tag-store.svelte";
  import { loadGroups } from "../../stores/group-store.svelte";
  import { anyModalOpen } from "../../stores/modal-layer.svelte";
  import { thumbnails } from "../../images/thumbnails";
  import { clearProgressiveImages } from "../../images/progressive-images";
  import { vibeStatuses } from "../../images/vibe-statuses";
  import type {
    ToolboxRowRequest,
    ToolboxSelectionSnapshot,
  } from "../../windows/toolbox";
  import ContextMenu from "./ContextMenu.svelte";
  import CanvasHeader from "./CanvasHeader.svelte";
  import DetailPanel from "./DetailPanel.svelte";
  import FilterPanel from "./FilterPanel.svelte";
  import DeleteDialog from "./DeleteDialog.svelte";
  import DropConfirmDialog from "./DropConfirmDialog.svelte";
  import DuplicateBrowseView from "../duplicates/DuplicateBrowseView.svelte";
  import GalleryView from "../gallery/GalleryView.svelte";
  import GroupBrowseView from "../groups/GroupBrowseView.svelte";
  import GroupDeleteConfirmDialog from "../groups/GroupDeleteConfirmDialog.svelte";
  import GroupManageView from "../groups/GroupManageView.svelte";
  import JsonExportDialog from "./JsonExportDialog.svelte";
  import PromptDocsView from "../prompt-docs/PromptDocsView.svelte";
  import MaterialsView from "../materials/MaterialsView.svelte";
  import SectionContextMenu from "./SectionContextMenu.svelte";
  import SelectionBar from "./SelectionBar.svelte";
  import TableView from "../table/TableView.svelte";
  import TagSidebar from "./TagSidebar.svelte";
  import TopBar from "./TopBar.svelte";
  import UpdateImportDialog from "./UpdateImportDialog.svelte";
  import { softFade, softPop } from "../../ui/motion";

  type DataViewMode = Exclude<ViewMode, "promptDocs" | "materials">;

  onDestroy(installNavigation());
  $effect.pre(() => {
    const route = navigationRoute();
    const directory = libraryState.snapshot?.dataDirectory;
    untrack(() => observeNavigation(route, directory));
  });
  $effect(() => {
    void navigation.restoring;
    void rowStore.initialLoading;
    void rowStore.refreshing;
    untrack(finishNavigationRestore);
  });

  const visitedViews = $state<Record<ViewMode, boolean>>({
    group: false,
    gallery: true,
    table: false,
    duplicates: false,
    promptDocs: false,
    materials: false,
  });

  function isDataViewMode(mode: ViewMode): mode is DataViewMode {
    return mode !== "promptDocs" && mode !== "materials";
  }

  function rememberVisitedView(mode: ViewMode): void {
    visitedViews[mode] = true;
  }

  $effect(() => {
    rememberVisitedView(workspaceState.viewMode);
  });

  // 首次挂载和工作簿替换时：清缩略图缓存（行 ID 可能复用）、重载数据、刷新 Tag 库、清空选择。
  // 撤销/重做等仅值变化（preserveSelection）时行集合未变：只重载数据与 Tag 库，
  // 保留选区和缩略图缓存——否则每按一次 Ctrl+Z 选区就被清空、整屏图闪烁。
  // untrack：这些调用内部有”读-改-写”（如 pagesVersion += 1），不能注册为本 effect 的依赖。
  $effect(() => {
    void libraryState.dataVersion;
    const preserveScroll = libraryState.preserveScrollOnDataChange;
    const preserveSelection = libraryState.preserveSelectionOnDataChange;
    untrack(() => {
      if (!preserveSelection) {
        thumbnails.clear();
        clearProgressiveImages();
        vibeStatuses.clear();
        clearSelection();
      }
      resetRows({
        keepStale: preserveScroll,
        resetScroll: !preserveScroll,
        keepActive: preserveSelection,
      });
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

      const index = await getRowIndex(request.rowId, rowStore.sort);
      revealRowInGallery(row, index);
      workspaceState.viewMode = "gallery";
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
    // 预载分组列表，供标题栏分段控件显示分组计数
    void loadGroups();
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
    if (event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey &&
      (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
      event.preventDefault();
      if (!event.isComposing && !event.repeat) navigateHistory(event.key === "ArrowLeft" ? -1 : 1);
      return;
    }
    // 任何模态浮层（管理分组、批量编辑、删除确认、灯箱等）打开时，
    // 资料库级快捷键一律短路，避免在对话框里按 Delete/Ctrl+Z 误操作底层数据。
    if (anyModalOpen()) {
      return;
    }
    if (materialGalleryPicker.active) {
      if (event.key === "Escape") { event.preventDefault(); finishMaterialGalleryPick(false); }
      return;
    }
    // 素材拥有独立文本与记录，不能触发画廊的删除、全选或撤销。
    if (workspaceState.viewMode === "materials") return;
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
      workspaceState.viewMode !== "promptDocs" &&
      !(event.target instanceof HTMLInputElement) &&
      !(event.target instanceof HTMLTextAreaElement)
    ) {
      event.preventDefault();
      void selectAllCurrentView();
      return;
    }

    if (event.key === "Delete" && workspaceState.viewMode !== "promptDocs" && !isEditing && !deletion.open) {
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

  function onHistoryMouse(event: MouseEvent): void {
    if (event.button !== 3 && event.button !== 4) return;
    event.preventDefault();
    if (event.type === "mouseup") navigateHistory(event.button === 3 ? -1 : 1);
  }
</script>

<svelte:window onkeydown={onKeydown} onmousedown={onHistoryMouse} onmouseup={onHistoryMouse} onauxclick={onHistoryMouse} />

<div class="workspace">
  <TopBar />
  {#if materialGalleryPicker.active}
    <div class="material-pick-bar" role="region" aria-label="选择素材展示图">
      <div><strong>为素材选择展示图</strong><span>{materialGalleryPicker.selected ? `已选：${rowFileName(materialGalleryPicker.selected) ?? `图片 #${materialGalleryPicker.selected.id}`}` : "在画廊中单击一张图片，可使用搜索和 Tag 筛选。"}</span></div>
      <div class="material-pick-actions">
        {#if workspaceState.viewMode !== "gallery"}<button class="btn" onclick={() => workspaceState.viewMode = "gallery"}>回到画廊</button>{/if}
        <button class="btn" onclick={() => finishMaterialGalleryPick(false)}>取消选图，返回编辑</button>
        <button class="btn btn-primary" disabled={!materialGalleryPicker.selected || taskState.busy || anyModalOpen()} onclick={() => finishMaterialGalleryPick(true)}>使用这张图片</button>
      </div>
    </div>
  {/if}
  {#if visitedViews.materials}
    {#key libraryState.snapshot?.dataDirectory}
      <div class="workspace-body prompt-docs-body" class:is-active={workspaceState.viewMode === "materials"} aria-hidden={workspaceState.viewMode !== "materials"}>
        <main class="prompt-docs-main"><MaterialsView active={workspaceState.viewMode === "materials"} /></main>
      </div>
    {/key}
  {/if}
  {#if visitedViews.promptDocs}
      <div
        class="workspace-body prompt-docs-body"
        class:is-active={workspaceState.viewMode === "promptDocs"}
        aria-hidden={workspaceState.viewMode !== "promptDocs"}
      >
        <main class="prompt-docs-main">
          <PromptDocsView />
        </main>
      </div>
    {/if}

    <div
      class="workspace-body data-body"
      class:is-active={isDataViewMode(workspaceState.viewMode)}
      aria-hidden={!isDataViewMode(workspaceState.viewMode)}
    >
      <aside class="sidebar">
        <TagSidebar />
      </aside>

      <main class="main-area">
        {#if rowStore.refreshing}
          <div class="refresh-bar" aria-hidden="true"></div>
        {/if}
        <CanvasHeader />
        <div class="view-stack">
          {#if visitedViews.group}
            <section class="view-panel" class:is-active={workspaceState.viewMode === "group"}>
              <GroupBrowseView active={workspaceState.viewMode === "group"} />
            </section>
          {/if}
          {#if visitedViews.duplicates}
            <section class="view-panel" class:is-active={workspaceState.viewMode === "duplicates"}>
              <DuplicateBrowseView active={workspaceState.viewMode === "duplicates"} />
            </section>
          {/if}
          {#if visitedViews.gallery}
            <section class="view-panel" class:is-active={workspaceState.viewMode === "gallery"}>
              <GalleryView active={workspaceState.viewMode === "gallery"} />
            </section>
          {/if}
          {#if visitedViews.table}
            <section class="view-panel" class:is-active={workspaceState.viewMode === "table"}>
              <TableView active={workspaceState.viewMode === "table"} />
            </section>
          {/if}
        </div>
        {#if !materialGalleryPicker.active}<SelectionBar />{/if}
      </main>

      <DetailSidebar open={workspaceState.detailOpen} onopen={() => workspaceState.detailOpen = true}>
        <DetailPanel />
      </DetailSidebar>
    </div>
</div>

<GroupManageView />
<GroupDeleteConfirmDialog />

<ContextMenu />
<SectionContextMenu />
<DeleteDialog />
<DropConfirmDialog />
<UpdateImportDialog />
<JsonExportDialog />
<FilterPanel />

{#if dropState.dragging && workspaceState.viewMode !== "promptDocs"}
  <div class="drop-overlay" transition:softFade={{ duration: 120 }}>
    <div class="drop-hint" transition:softPop={{ duration: 150, y: 4, start: 0.98 }}>
      {workspaceState.viewMode === "materials" ? "松开鼠标，预览并确认素材" : "松开鼠标以导入图片"}
    </div>
  </div>
{/if}

<style>
  .material-pick-bar { display: flex; flex: none; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; padding: 12px 20px; border-bottom: 1px solid var(--border); background: var(--accent-soft); }
  .material-pick-bar > div:first-child { display: grid; gap: 5px; min-width: 0; flex: 1; }
  .material-pick-bar span { font-size: var(--font-sm); color: var(--text-2); overflow-wrap: anywhere; }
  .material-pick-actions { display: flex; flex-wrap: wrap; gap: 8px; }
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

  .workspace-body:not(.is-active) {
    display: none;
  }

  /* display:none → flex 恢复渲染盒时 animation 会重放，零 JS 进场 */
  .workspace-body.is-active {
    animation: view-panel-in var(--motion-base) var(--ease-responsive);
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
    animation: view-panel-in var(--motion-base) var(--ease-responsive);
  }

  @keyframes view-panel-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
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

  /* 窄窗口：侧栏与详情面板分档收窄，保住画布最小可用宽度 */
  @media (max-width: 1240px) {
    .sidebar {
      width: 210px;
    }

  }

  @media (max-width: 1080px) {
    .sidebar {
      width: 190px;
    }

  }

  .drop-overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-dragdrop);
    display: grid;
    place-items: center;
    background: var(--overlay);
    pointer-events: none;
  }

  .drop-hint {
    padding: 20px 40px;
    background: var(--surface);
    border: 2px dashed var(--accent);
    border-radius: var(--radius-m);
    font-size: var(--font-xl);
    font-weight: 600;
    color: var(--accent);
  }

  @media (prefers-reduced-motion: reduce) {
    .refresh-bar::after {
      animation: none;
      transform: translateX(80%);
    }
  }

</style>
