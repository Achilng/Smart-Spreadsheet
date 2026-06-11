<script lang="ts">
  import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
  import { untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";

  import {
    deleteRows,
    findDuplicates,
    type DuplicateGroup,
    type DuplicateKeyKind,
    type DuplicateReport,
  } from "../../api";
  import { app, errorText, formatCount, setNotice } from "../app-state.svelte";
  import Thumbnail from "./Thumbnail.svelte";

  const GROUP_LIMIT = 100;

  let keyKind = $state<DuplicateKeyKind>("positivePrompt");
  let report = $state<DuplicateReport | null>(null);
  let loading = $state(false);
  let deleting = $state(false);
  let loadError = $state<string | null>(null);
  const checked = new SvelteSet<number>();

  async function load(): Promise<void> {
    loading = true;
    loadError = null;
    checked.clear();
    try {
      report = await findDuplicates(keyKind, GROUP_LIMIT);
    } catch (error) {
      report = null;
      loadError = errorText(error);
    } finally {
      loading = false;
    }
  }

  // 切换分组依据时重新加载；load 内部的状态写入不应注册为依赖
  $effect(() => {
    void keyKind;
    untrack(() => void load());
  });

  const checkedCount = $derived(checked.size);
  /** 全部勾选（即将被删空）的组：删除前必须至少留一行 */
  const emptiedGroups = $derived(
    report
      ? report.groups.filter(group => group.rows.every(row => checked.has(row.id))).length
      : 0,
  );

  function toggleRow(id: number): void {
    if (checked.has(id)) {
      checked.delete(id);
    } else {
      checked.add(id);
    }
  }

  function keepFirst(group: DuplicateGroup): void {
    group.rows.forEach((row, index) => {
      if (index === 0) {
        checked.delete(row.id);
      } else {
        checked.add(row.id);
      }
    });
  }

  function keepFirstAll(): void {
    report?.groups.forEach(keepFirst);
  }

  async function applyDelete(): Promise<void> {
    if (checkedCount === 0 || emptiedGroups > 0 || deleting) {
      return;
    }
    const confirmed = await confirmDialog(
      `将从资料库删除 ${formatCount(checkedCount)} 行（含 Tag 关联与受管副本）。原始图片文件不受影响。是否继续？`,
      { title: "删除重复行", kind: "warning", okLabel: "删除", cancelLabel: "取消" },
    );
    if (!confirmed) {
      return;
    }
    deleting = true;
    try {
      const result = await deleteRows({ kind: "explicit", rowIds: [...checked] });
      app.snapshot = result.snapshot;
      app.dataVersion += 1;
      setNotice({
        tone: "success",
        text: `已删除 ${formatCount(result.deletedRows)} 行重复数据。`,
      });
      await load();
    } catch (error) {
      setNotice({ tone: "error", text: `删除失败：${errorText(error)}` });
    } finally {
      deleting = false;
    }
  }
</script>

<svelte:window
  onkeydown={event => {
    if (event.key === "Escape" && !deleting) {
      app.dedupeOpen = false;
    }
  }}
/>

<div class="dedupe-overlay">
  <div class="dedupe-panel">
    <header class="dedupe-header">
      <h3>库内查重</h3>
      <div class="key-switch" role="group" aria-label="查重依据">
        <button
          type="button"
          class:is-active={keyKind === "positivePrompt"}
          disabled={loading || deleting}
          onclick={() => (keyKind = "positivePrompt")}
        >
          按正向提示词
        </button>
        <button
          type="button"
          class:is-active={keyKind === "artists"}
          disabled={loading || deleting}
          onclick={() => (keyKind = "artists")}
        >
          按画师串
        </button>
      </div>
      <div class="header-actions">
        <button type="button" class="btn btn-ghost" disabled={loading || deleting} onclick={() => void load()}>
          刷新
        </button>
        <button type="button" class="btn" disabled={deleting} onclick={() => (app.dedupeOpen = false)}>
          关闭
        </button>
      </div>
    </header>

    <div class="dedupe-summary">
      {#if loading}
        <span class="faint">正在检索重复行…</span>
      {:else if loadError}
        <span class="error-text">查重失败：{loadError}</span>
      {:else if report}
        <span>
          共 {formatCount(report.totalGroups)} 组重复，多余 {formatCount(report.totalRedundantRows)} 行
          {#if report.totalGroups > report.groups.length}
            <small class="faint">（仅显示前 {report.groups.length} 组，处理后刷新查看其余）</small>
          {/if}
        </span>
        {#if report.groups.length > 0}
          <button type="button" class="btn btn-ghost" disabled={deleting} onclick={keepFirstAll}>
            每组仅保留第一行
          </button>
        {/if}
      {/if}
    </div>

    <div class="dedupe-scroll">
      {#if !loading && report && report.groups.length === 0 && !loadError}
        <div class="dedupe-empty">
          <p class="faint">没有发现重复行。</p>
        </div>
      {:else if report}
        {#each report.groups as group (group.key)}
          <section class="dup-group">
            <div class="group-head">
              <p class="group-key" title={group.key}>{group.key}</p>
              <button type="button" class="btn btn-ghost" disabled={deleting} onclick={() => keepFirst(group)}>
                保留第一行
              </button>
            </div>
            <div class="group-rows">
              {#each group.rows as row (row.id)}
                <label class="dup-card" class:is-checked={checked.has(row.id)}>
                  <input
                    type="checkbox"
                    checked={checked.has(row.id)}
                    disabled={deleting}
                    onchange={() => toggleRow(row.id)}
                  />
                  <div class="dup-thumb">
                    <Thumbnail
                      rowId={row.id}
                      hasImage={Boolean(row.imagePath?.trim() || row.storedImagePath?.trim())}
                      alt="第 {row.sourceOrdinal} 行缩略图"
                    />
                  </div>
                  <div class="dup-meta">
                    <span class="faint">#{row.sourceOrdinal} · 批次 {row.batchId}</span>
                    {#if row.time}
                      <span class="faint">{row.time}</span>
                    {/if}
                    {#if row.tags.length > 0}
                      <span class="dup-tags" title={row.tags.join(", ")}>
                        {row.tags.slice(0, 2).join(", ")}{row.tags.length > 2 ? ` +${row.tags.length - 2}` : ""}
                      </span>
                    {/if}
                  </div>
                </label>
              {/each}
            </div>
          </section>
        {/each}
      {/if}
    </div>

    <footer class="dedupe-footer">
      <span>
        已勾选 {formatCount(checkedCount)} 行
        {#if emptiedGroups > 0}
          <small class="error-text">（{emptiedGroups} 组被全选，每组必须至少保留一行）</small>
        {/if}
      </span>
      <button
        type="button"
        class="btn btn-danger"
        disabled={deleting || checkedCount === 0 || emptiedGroups > 0}
        onclick={() => void applyDelete()}
      >
        {deleting ? "删除中…" : "删除勾选行"}
      </button>
    </footer>
  </div>
</div>

<style>
  .dedupe-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgb(0 0 0 / 35%);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px;
  }

  .dedupe-panel {
    width: min(960px, 100%);
    height: 100%;
    background: var(--bg);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-2);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .dedupe-header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    border-radius: var(--radius-m) var(--radius-m) 0 0;
  }

  .dedupe-header h3 {
    font-size: 15px;
    margin: 0;
  }

  .key-switch {
    display: flex;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    padding: 2px;
    gap: 2px;
  }

  .key-switch button {
    border: none;
    background: transparent;
    border-radius: 4px;
    padding: 3px 12px;
    font-size: 12.5px;
    color: var(--text-2);
  }

  .key-switch button.is-active {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow-1);
  }

  .header-actions {
    margin-left: auto;
    display: flex;
    gap: 8px;
  }

  .dedupe-summary {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 16px;
    font-size: 13px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    min-height: 42px;
  }

  .dedupe-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 0;
  }

  .dedupe-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
  }

  .dup-group {
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    background: var(--surface);
    overflow: hidden;
  }

  .group-head {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .group-key {
    flex: 1;
    margin: 0;
    font-size: 12.5px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .group-rows {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    padding: 10px 12px;
  }

  .dup-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 132px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    cursor: pointer;
    position: relative;
  }

  .dup-card:hover {
    border-color: var(--border-strong);
  }

  .dup-card.is-checked {
    border-color: var(--danger);
    background: var(--danger-soft);
  }

  .dup-card input {
    position: absolute;
    top: 10px;
    left: 10px;
    z-index: 2;
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--danger);
  }

  .dup-thumb {
    height: 96px;
    border-radius: 4px;
    overflow: hidden;
    background: var(--surface-2);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dup-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 11px;
    min-width: 0;
  }

  .dup-tags {
    color: var(--accent);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dedupe-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 16px;
    border-top: 1px solid var(--border);
    background: var(--surface);
    border-radius: 0 0 var(--radius-m) var(--radius-m);
    font-size: 13px;
  }

  .error-text {
    color: var(--danger);
  }
</style>
