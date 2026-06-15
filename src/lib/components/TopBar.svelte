<script lang="ts">
  import {
    app,
    chooseMigration,
    chooseSearchImage,
    resetDataWithConfirmation,
    runPhashBackfill,
    formatCount,
    type ViewMode,
  } from "../app-state.svelte";
  import {
    chooseImageArchive,
    chooseImageFolder,
  } from "../import-actions.svelte";
  import {
    chooseImageFilesExport,
    chooseJsonExport,
    chooseXlsxExport,
    exportScopeLabel,
  } from "../export-actions";
  import { clearSelection } from "../selection-store.svelte";
  import { setGroupView } from "../row-store.svelte";
  import Dropdown, { type DropdownItem } from "./Dropdown.svelte";
  import WindowControls from "./WindowControls.svelte";

  const library = $derived(app.snapshot?.library ?? null);
  const lastSourceName = $derived(
    app.snapshot?.library?.lastBatch?.sourcePath.split(/[\\/]/).pop() ?? null,
  );

  const views: { mode: ViewMode; label: string }[] = [
    { mode: "group", label: "分组" },
    { mode: "gallery", label: "画廊" },
    { mode: "table", label: "表格" },
  ];

  function switchView(mode: ViewMode): void {
    const wasGroup = app.viewMode === "group";
    const isGroup = mode === "group";
    app.viewMode = mode;
    if (wasGroup !== isGroup) {
      setGroupView(isGroup);
      clearSelection();
    }
  }

  const toolItems = $derived<DropdownItem[]>([
    { label: "建议分组", hint: "按相似度聚类未分组行", action: () => (app.groupSuggestOpen = true) },
    { label: "管理分组", action: () => (app.groupManageOpen = true) },
    { label: "以图搜图", action: () => void chooseSearchImage() },
    { label: "刷新感知哈希", action: () => void runPhashBackfill() },
    { label: "智绘姬 JSON 去重", action: () => (app.jsonDedupeOpen = true) },
    { label: "迁移数据目录", action: () => void chooseMigration() },
    { label: "重置表格", hint: "清空数据重新开始", action: () => void resetDataWithConfirmation() },
  ]);

  const importItems: DropdownItem[] = [
    { label: "导入文件夹", action: () => void chooseImageFolder() },
    { label: "导入压缩包", hint: "zip / 7z / rar", action: () => void chooseImageArchive() },
  ];

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
</script>

<header class="topbar" data-tauri-drag-region>
  <div class="workbook-info" data-tauri-drag-region>
    {#if library}
      <strong class="name" data-tauri-drag-region title={app.snapshot?.library?.lastBatch?.sourcePath}>
        图片资料库
      </strong>
      <span class="meta faint" data-tauri-drag-region>
        {formatCount(library.rowCount)} 行 · {formatCount(library.batchCount)} 次导入{lastSourceName
          ? ` · 最近：${lastSourceName}`
          : ""}
      </span>
    {/if}
  </div>

  <div class="view-switch" role="group" aria-label="视图切换">
    {#each views as view (view.mode)}
      <button
        type="button"
        class:is-active={app.viewMode === view.mode}
        aria-pressed={app.viewMode === view.mode}
        onclick={() => switchView(view.mode)}
      >
        {view.label}
      </button>
    {/each}
  </div>

  <div class="actions">
    <Dropdown label="工具" items={toolItems} disabled={app.busy} />
    <Dropdown label="导入" items={importItems} disabled={app.busy} />
    <Dropdown label="导出" items={exportItems} disabled={exportDisabled} primary />
  </div>

  <WindowControls />
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 48px;
    padding: 0 0 0 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  .workbook-info {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }

  .name {
    font-size: 14px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta {
    font-size: 12px;
    white-space: nowrap;
  }

  .view-switch {
    display: flex;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    padding: 2px;
    gap: 2px;
  }

  .view-switch button {
    border: none;
    background: transparent;
    border-radius: 4px;
    padding: 3px 14px;
    font-size: 13px;
    color: var(--text-2);
  }

  .view-switch button.is-active {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow-1);
  }

  .actions {
    display: flex;
    gap: 8px;
    flex: none;
    margin-right: 4px;
  }
</style>
