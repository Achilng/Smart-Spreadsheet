<script lang="ts">
  import {
    app,
    chooseExport,
    chooseMigration,
    chooseWorkbook,
    formatCount,
    type ViewMode,
  } from "../app-state.svelte";
  import WindowControls from "./WindowControls.svelte";

  const workbook = $derived(app.snapshot?.workbook ?? null);

  const views: { mode: ViewMode; label: string }[] = [
    { mode: "gallery", label: "画廊" },
    { mode: "table", label: "表格" },
  ];
</script>

<header class="topbar" data-tauri-drag-region>
  <div class="workbook-info" data-tauri-drag-region>
    {#if workbook}
      <strong class="name" data-tauri-drag-region title={workbook.importedName}>
        {workbook.importedName}
      </strong>
      <span class="meta faint" data-tauri-drag-region>
        {workbook.sheetName} · {formatCount(workbook.rowCount)} 行
      </span>
    {/if}
  </div>

  <div class="view-switch" role="group" aria-label="视图切换">
    {#each views as view (view.mode)}
      <button
        type="button"
        class:is-active={app.viewMode === view.mode}
        aria-pressed={app.viewMode === view.mode}
        onclick={() => (app.viewMode = view.mode)}
      >
        {view.label}
      </button>
    {/each}
  </div>

  <div class="actions">
    <button type="button" class="btn btn-ghost" disabled={app.busy} onclick={() => void chooseMigration()}>
      迁移目录
    </button>
    <button type="button" class="btn" disabled={app.busy} onclick={() => void chooseWorkbook()}>
      替换工作簿
    </button>
    <button type="button" class="btn btn-primary" disabled={app.busy} onclick={() => void chooseExport()}>
      导出副本
    </button>
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
