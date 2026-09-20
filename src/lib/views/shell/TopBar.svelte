<script lang="ts">
  import { errorText, formatCount } from "../../utils/format";
  import { setNotice } from "../../stores/notices.svelte";
  import { libraryState } from "../../stores/library-state.svelte";
  import { workspaceState } from "../../stores/workspace-state.svelte";
  import { taskState } from "../../stores/task-state.svelte";
  import X from "@lucide/svelte/icons/x";
  import { onDestroy, untrack } from "svelte";
  import NavigationButtons from "./NavigationButtons.svelte";
  import { navigation } from "../../stores/navigation.svelte";

  import {
    chooseImageArchive,
    chooseImageFolder,
    updateAutoArtistPrefixOnImport,
  } from "../../stores/import-actions.svelte";
  import { buildExportItems } from "../../stores/export-actions";
  import { clearSelection } from "../../stores/selection-store.svelte";
  import { rowStore, setSearch } from "../../stores/row-store.svelte";
  import { materialBrowser } from "../../stores/material-browser.svelte";
  import Dropdown, { type DropdownItem } from "../../ui/Dropdown.svelte";
  import { openToolboxWindow } from "../../windows/toolbox";
  import ViewSwitcher from "./ViewSwitcher.svelte";
  import WindowControls from "../../ui/WindowControls.svelte";
  import { softPop } from "../../ui/motion";

  let searchInput = $state("");
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  let searchSession = 0;
  let lastInputAt = 0;
  let searchScope: "materials" | "rows" = "rows";
  const isMaterials = $derived(workspaceState.viewMode === "materials");

  function flushSearch(): void {
    clearTimeout(debounceTimer);
    if ((isMaterials ? "materials" : "rows") !== searchScope) return;
    if (isMaterials) { materialBrowser.search = searchInput; return; }
    if (rowStore.search !== searchInput) {
      setSearch(searchInput, searchSession);
      clearSelection();
    }
  }

  function finishSearch(): void {
    flushSearch();
    searchSession += 1;
  }

  onDestroy(() => clearTimeout(debounceTimer));

  function onSearchInput(event: Event): void {
    const value = (event.target as HTMLInputElement).value;
    searchInput = value;
    searchScope = isMaterials ? "materials" : "rows";
    clearTimeout(debounceTimer);
    if (event instanceof InputEvent && event.isComposing) return;
    if (Date.now() - lastInputAt > 1500) searchSession += 1;
    lastInputAt = Date.now();
    debounceTimer = setTimeout(flushSearch, 300);
  }

  function clearSearch(): void {
    searchSession += 1;
    searchInput = "";
    clearTimeout(debounceTimer);
    if (isMaterials) { materialBrowser.search = ""; return; }
    if (rowStore.search !== "") {
      setSearch("");
      clearSelection();
    }
  }

  // Each library owns its search; switching views cancels any uncommitted debounce.
  $effect(() => {
    const scope = isMaterials ? "materials" : "rows";
    const external = isMaterials ? materialBrowser.search : rowStore.search;
    void navigation.token;
    untrack(() => {
      searchInput = external;
      searchScope = scope;
      clearTimeout(debounceTimer);
    });
  });

  const library = $derived(libraryState.snapshot?.library ?? null);

  const importItems = $derived<DropdownItem[]>([
    { label: "导入文件夹", action: () => void chooseImageFolder() },
    { label: "导入压缩包", hint: "zip / 7z / rar", action: () => void chooseImageArchive() },
    {
      label: "导入时自动补全画师前缀",
      hint: "仅使用库内明确 artist: 证据",
      checked: libraryState.snapshot?.autoArtistPrefixOnImport ?? false,
      action: () => void updateAutoArtistPrefixOnImport(
        !(libraryState.snapshot?.autoArtistPrefixOnImport ?? false),
      ),
    },
    {
      label: "更新现有图片",
      hint: "只更新，不新增；保留 Tag / 分组",
      action: () => (workspaceState.updateImportOpen = true),
    },
  ]);

  const exportItems = $derived<DropdownItem[]>(buildExportItems());

  const exportDisabled = $derived(taskState.busy || !library || library.rowCount === 0);

  async function openToolbox(): Promise<void> {
    try {
      await openToolboxWindow();
    } catch (error) {
      setNotice({ tone: "error", text: errorText(error) });
    }
  }
</script>

<header class="topbar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <span data-tauri-drag-region>智能表格</span>
    {#if library && library.rowCount > 0}
      <small data-tauri-drag-region>{formatCount(library.rowCount)} 张图片</small>
    {/if}
  </div>

  <NavigationButtons />
  <ViewSwitcher />

  <div class="title-spacer" data-tauri-drag-region></div>

  {#if workspaceState.viewMode !== "promptDocs"}
    <div class="search-box" data-tauri-drag-region>
      <input
        type="text"
        placeholder={isMaterials ? "搜索素材名称 / 文本内容…" : "搜索文件名 / 提示词 / 画师…"}
        aria-label={isMaterials ? "搜索素材名称和文本" : "搜索文件名、提示词和画师"}
        value={searchInput}
        oninput={onSearchInput}
        oncompositionend={onSearchInput}
        onblur={finishSearch}
        onkeydown={event => { if (event.key === "Enter" && !event.isComposing) finishSearch(); }}
        class:has-value={searchInput.length > 0}
      />
      {#if searchInput.length > 0}
        <button
          type="button"
          class="search-clear"
          onclick={clearSearch}
          title="清除搜索"
          transition:softPop={{ duration: 110, y: 0, start: 0.85 }}
        ><X size={13} strokeWidth={2} /></button>
      {/if}
    </div>
  {/if}

  <div class="actions">
    <button
      type="button"
      class="btn btn-ghost"
      disabled={taskState.busy}
      onclick={() => void openToolbox()}
    >
      工具箱
    </button>
    {#if workspaceState.viewMode !== "materials"}
      <Dropdown label="导入" items={importItems} disabled={taskState.busy} ghost />
      <Dropdown label="导出" items={exportItems} disabled={exportDisabled} primary />
    {/if}
  </div>
  <WindowControls />
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 52px;
    padding: 0 0 0 20px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    flex: none;
    min-width: 0;
  }

  .brand {
    display: flex;
    align-items: baseline;
    gap: 7px;
    flex: none;
    font-size: var(--font-lg);
    font-weight: 700;
    letter-spacing: -0.01em;
    color: var(--text);
    white-space: nowrap;
  }

  .brand small {
    font-size: var(--font-xs);
    font-weight: 400;
    color: var(--text-3);
    letter-spacing: 0;
    font-variant-numeric: tabular-nums;
  }

  /* 窄窗口：先隐藏品牌小字，给控件让位 */
  @media (max-width: 1180px) {
    .brand small {
      display: none;
    }
  }

  @media (max-width: 1040px) {
    .topbar {
      gap: 8px;
    }
  }

  @media (max-width: 950px) {
    .brand { display: none; }
    .topbar { padding-left: 10px; gap: 6px; }
    .title-spacer { min-width: 0; }
  }

  .title-spacer {
    flex: 1;
    min-width: 16px;
  }

  .search-box {
    position: relative;
    flex: 0 1 260px;
    min-width: 120px;
  }

  .search-box input {
    width: 100%;
    height: 32px;
    padding: 0 30px 0 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-full);
    background: var(--surface-2);
    font-size: var(--font-sm);
    color: var(--text);
    outline: none;
    box-sizing: border-box;
    transition:
      border-color var(--motion-fast) var(--ease-responsive),
      box-shadow var(--motion-fast) var(--ease-responsive),
      background var(--motion-fast) var(--ease-responsive);
  }

  .search-box input:hover:not(:focus) {
    background: var(--surface-3);
  }

  .search-box input:focus {
    border-color: var(--accent);
    background: var(--surface);
    box-shadow: var(--focus-ring);
  }

  .search-box input::placeholder {
    color: var(--text-3);
  }

  .search-clear {
    position: absolute;
    right: 5px;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    min-height: 24px;
    border: none;
    background: transparent;
    color: var(--text-3);
    line-height: 1;
    padding: 0;
    cursor: pointer;
    border-radius: 50%;
  }

  .search-clear:hover {
    background: var(--surface-3);
    color: var(--text);
  }

  .search-clear:active {
    transform: translateY(-50%) scale(0.85);
  }

  .actions {
    display: flex;
    gap: 4px;
    flex: none;
    margin-right: 4px;
  }

  /* 超窄窗口：按钮收紧内距 */
  @media (max-width: 1000px) {
    .actions :global(.btn) {
      padding-left: 10px;
      padding-right: 10px;
    }

    .search-box {
      flex-basis: 180px;
    }
  }

  @media (max-width: 950px) {
    .title-spacer { min-width: 0; }
  }

  @media (max-width: 850px) {
    .topbar { gap: 4px; padding-left: 6px; }
    .actions :global(.btn) { padding-left: 6px; padding-right: 6px; }
  }
</style>
