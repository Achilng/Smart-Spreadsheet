<script lang="ts">
  import { app, errorText, setNotice } from "../../stores/app-state.svelte";
  import {
    chooseImageArchive,
    chooseImageFolder,
  } from "../../stores/import-actions.svelte";
  import {
    chooseImageFilesExport,
    chooseJsonExport,
    chooseXlsxExport,
    exportScopeLabel,
  } from "../../stores/export-actions";
  import { clearSelection } from "../../stores/selection-store.svelte";
  import { setSearch } from "../../stores/row-store.svelte";
  import Dropdown, { type DropdownItem } from "../../ui/Dropdown.svelte";
  import { openToolboxWindow } from "../../windows/toolbox";
  import SizeSlider from "./SizeSlider.svelte";
  import WindowControls from "../../ui/WindowControls.svelte";

  let searchInput = $state("");
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  function onSearchInput(event: Event): void {
    const value = (event.target as HTMLInputElement).value;
    searchInput = value;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      setSearch(value);
      clearSelection();
    }, 300);
  }

  function clearSearch(): void {
    searchInput = "";
    clearTimeout(debounceTimer);
    setSearch("");
    clearSelection();
  }

  const library = $derived(app.snapshot?.library ?? null);

  const importItems = $derived<DropdownItem[]>([
    { label: "导入文件夹", action: () => void chooseImageFolder() },
    { label: "导入压缩包", hint: "zip / 7z / rar", action: () => void chooseImageArchive() },
    {
      label: "更新现有图片",
      hint: "只更新，不新增；保留 Tag / 分组",
      action: () => (app.updateImportOpen = true),
    },
  ]);

  const scopeHint = $derived(`导出${exportScopeLabel()}`);
  const exportItems = $derived<DropdownItem[]>([
    { label: "导出 xlsx", hint: scopeHint, action: () => void chooseXlsxExport() },
    { label: "导出智绘姬 JSON", hint: scopeHint, action: () => void chooseJsonExport() },
    {
      label: "导出图片（复制）",
      hint: scopeHint,
      action: () => void chooseImageFilesExport("copy"),
    },
    {
      label: "导出图片（硬链接）",
      hint: `${scopeHint} · 同盘秒出，失败自动复制`,
      action: () => void chooseImageFilesExport("hardlink"),
    },
  ]);

  const exportDisabled = $derived(app.busy || !library || library.rowCount === 0);

  async function openToolbox(): Promise<void> {
    try {
      await openToolboxWindow();
    } catch (error) {
      setNotice({ tone: "error", text: errorText(error) });
    }
  }
</script>

<header class="topbar" data-tauri-drag-region>
  {#if app.viewMode !== "promptDocs"}
    <div class="search-box" data-tauri-drag-region>
      <input
        type="text"
        placeholder="搜索文件名 / 正向 / 角色 / 负向 / 画师…"
        value={searchInput}
        oninput={onSearchInput}
        class:has-value={searchInput.length > 0}
      />
      {#if searchInput.length > 0}
        <button type="button" class="search-clear" onclick={clearSearch} title="清除搜索">&times;</button>
      {/if}
    </div>
  {:else}
    <div class="topbar-spacer" data-tauri-drag-region></div>
  {/if}

  <div class="topbar-right" data-tauri-drag-region>
    <SizeSlider />
    <div class="actions">
      <button
        type="button"
        class="btn"
        disabled={app.busy}
        onclick={() => void openToolbox()}
      >
        工具箱
      </button>
      <Dropdown label="导入" items={importItems} disabled={app.busy} />
      <Dropdown label="导出" items={exportItems} disabled={exportDisabled} primary />
    </div>
    <WindowControls />
  </div>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 40px;
    padding: 0 0 0 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  .topbar-spacer {
    flex: 1;
  }

  .topbar-right {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-left: auto;
  }

  .search-box {
    position: relative;
    flex: none;
    width: 220px;
  }

  .search-box input {
    width: 100%;
    height: 28px;
    padding: 0 26px 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--bg);
    font-size: var(--font-sm);
    color: var(--text);
    outline: none;
    box-sizing: border-box;
    transition: border-color 0.12s ease, box-shadow 0.12s ease, background 0.12s ease;
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
    right: 2px;
    top: 50%;
    transform: translateY(-50%);
    border: none;
    background: transparent;
    color: var(--text-3, #999);
    font-size: var(--font-lg);
    line-height: 1;
    padding: 2px 4px;
    cursor: pointer;
    border-radius: 2px;
  }

  .search-clear:hover {
    color: var(--text);
  }

  .actions {
    display: flex;
    gap: 8px;
    flex: none;
    margin-right: 4px;
  }

</style>
