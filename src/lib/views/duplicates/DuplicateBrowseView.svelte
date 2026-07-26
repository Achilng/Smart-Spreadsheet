<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import { untrack } from "svelte";

  import type { DedupeCluster, DedupeMode, RowRecord } from "../../api";
  import { app, formatCount } from "../../stores/app-state.svelte";
  import {
    duplicateBrowse,
    ensureClusterMembers,
    loadMoreClusterMembers,
    syncDuplicateCaches,
  } from "../../stores/duplicate-browse-store.svelte";
  import { rowStore } from "../../stores/row-store.svelte";
  import { sectionMenu, showSectionMenu } from "../../stores/section-context-menu.svelte";
  import { restoreScrollPosition, saveScrollPosition } from "../../stores/view-state";
  import GroupSectionCard from "../groups/GroupSectionCard.svelte";
  import { softFade } from "../../ui/motion";

  let { active = true }: { active?: boolean } = $props();

  let listEl = $state<HTMLDivElement | null>(null);

  // 数据/筛选/聚合依据/别名变化时同步缓存；缓存有效时切回秒开。
  $effect(() => {
    if (!active) {
      return;
    }
    void app.dataVersion;
    void sectionMenu.aliasVersion;
    void duplicateBrowse.dedupeMode;
    void rowStore.tags;
    void rowStore.tagMode;
    void rowStore.singleArtistOnly;
    void rowStore.hasVibe;
    void rowStore.hideGrouped;
    untrack(() => syncDuplicateCaches());
  });

  // 挂载和每次切回激活时恢复滚动位置（展开的成员网格异步加载，切回第一帧
  // 高度可能不足被钳制，内部按帧重试推回原位）
  let restored = false;
  let restoring = false;

  $effect(() => {
    if (!active) {
      restored = false;
    }
  });

  $effect(() => {
    if (restored || !active || !listEl || duplicateBrowse.loading) {
      return;
    }
    restored = true;
    restoring = true;
    restoreScrollPosition(listEl, "duplicates", 60, undefined, () => (restoring = false));
  });

  function onScroll(): void {
    // 恢复期间的钳制事件不代表用户位置，不能写入记忆
    if (active && !restoring) {
      saveScrollPosition("duplicates", listEl?.scrollTop ?? 0);
    }
  }

  function setMode(mode: DedupeMode): void {
    duplicateBrowse.dedupeMode = mode;
  }

  const sortedClusters = $derived(
    duplicateBrowse.sortByCount
      ? duplicateBrowse.clusters
      : [...duplicateBrowse.clusters].sort((a, b) =>
          (a.alias ?? a.key).localeCompare(b.alias ?? b.key),
        ),
  );

  function isExpanded(key: string): boolean {
    return duplicateBrowse.expandedKeys.includes(key);
  }

  async function toggleCluster(key: string): Promise<void> {
    if (isExpanded(key)) {
      duplicateBrowse.expandedKeys = duplicateBrowse.expandedKeys.filter(k => k !== key);
    } else {
      duplicateBrowse.expandedKeys = [...duplicateBrowse.expandedKeys, key];
      await ensureClusterMembers(key);
    }
  }

  function onHeaderContextMenu(
    event: MouseEvent,
    cluster: DedupeCluster,
  ): void {
    event.preventDefault();
    if (duplicateBrowse.dedupeMode === "none") return;
    showSectionMenu(
      {
        kind: "dedupe",
        mode: duplicateBrowse.dedupeMode as "artists" | "positivePrompt",
        key: cluster.key,
        displayName: cluster.alias ?? cluster.key,
      },
      event.clientX,
      event.clientY,
    );
  }

  function setActive(row: RowRecord): void {
    rowStore.activeRow = row;
  }
</script>

<div class="duplicate-browse">
  <div class="mode-bar">
    <span class="mode-label">聚合依据：</span>
    <button
      type="button"
      class:is-active={duplicateBrowse.dedupeMode === "artists"}
      onclick={() => setMode("artists")}
    >按画师串</button>
    <button
      type="button"
      class:is-active={duplicateBrowse.dedupeMode === "positivePrompt"}
      onclick={() => setMode("positivePrompt")}
    >按正向提示词</button>
    <span class="bar-separator"></span>
    <span class="mode-label">排序：</span>
    <button
      type="button"
      class:is-active={duplicateBrowse.sortByCount}
      onclick={() => (duplicateBrowse.sortByCount = true)}
    >按数量</button>
    <button
      type="button"
      class:is-active={!duplicateBrowse.sortByCount}
      onclick={() => (duplicateBrowse.sortByCount = false)}
    >按名称</button>
  </div>

  {#if duplicateBrowse.loading}
    <div class="status empty-state"><p class="muted">正在加载重复项…</p></div>
  {:else if duplicateBrowse.error}
    <div class="status empty-state"><p class="muted">加载失败：{duplicateBrowse.error}</p></div>
  {:else if duplicateBrowse.clusters.length === 0}
    <div class="status empty-state"><p class="muted">未找到重复项（所有条目均唯一）。</p></div>
  {:else}
    <div class="cluster-list" bind:this={listEl} onscroll={onScroll}>
      {#each sortedClusters as cluster (cluster.key)}
        {@const expanded = isExpanded(cluster.key)}
        {@const data = duplicateBrowse.memberCache[cluster.key]}
        <section class="group-section">
          <button
            type="button"
            class="section-header"
            onclick={() => void toggleCluster(cluster.key)}
            oncontextmenu={(e) => onHeaderContextMenu(e, cluster)}
          >
            <span class="expand-icon" class:is-expanded={expanded}><ChevronRight size={14} strokeWidth={2} /></span>
            <span class="section-name">{cluster.alias ?? cluster.key}</span>
            {#if cluster.alias}
              <span class="section-orig-key" title={cluster.key}>({cluster.key.slice(0, 30)}{cluster.key.length > 30 ? "…" : ""})</span>
            {/if}
            <span class="section-count">{formatCount(cluster.memberCount)} 张</span>
          </button>
          {#if expanded}
            <div class="member-grid" transition:softFade={{ duration: 130 }}>
              {#if !data || (data.loading && data.rows.length === 0)}
                <p class="muted grid-status">加载中…</p>
              {:else if data.error}
                <p class="muted grid-status">加载失败：{data.error}</p>
              {:else}
                {#each data.rows as member (member.id)}
                  <GroupSectionCard row={member} onactivate={() => setActive(member)} />
                {/each}
                {#if data.rows.length < data.totalCount}
                  <button
                    type="button"
                    class="load-more-btn"
                    disabled={data.loading}
                    onclick={() => void loadMoreClusterMembers(cluster.key)}
                  >
                    {data.loading ? "加载中…" : `加载更多（还有 ${formatCount(data.totalCount - data.rows.length)} 张）`}
                  </button>
                {/if}
              {/if}
            </div>
          {/if}
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .duplicate-browse {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0;
    display: flex;
    flex-direction: column;
  }

  .mode-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    flex: none;
  }

  .mode-label {
    font-size: var(--font-md);
    color: var(--text-2);
    margin-right: 4px;
  }

  .bar-separator {
    width: 1px;
    height: 18px;
    background: var(--border);
    margin: 0 6px;
  }

  .mode-bar button {
    border: 1px solid var(--border);
    background: transparent;
    border-radius: var(--radius-s);
    padding: 4px 12px;
    font-size: var(--font-sm);
    color: var(--text-2);
    cursor: pointer;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive),
      transform var(--motion-press) var(--ease-responsive);
  }

  .mode-bar button:active {
    transform: scale(0.98);
  }

  .mode-bar button:hover {
    background: var(--surface-2);
  }

  .mode-bar button.is-active {
    background: var(--surface-2);
    color: var(--text);
    border-color: var(--border-strong);
  }

  .status {
    flex: 1;
  }

  .cluster-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 0;
  }

  .group-section {
    margin-bottom: 2px;
  }

  .section-header {
    position: sticky;
    top: 0;
    z-index: var(--z-nav);
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 10px 16px;
    border: none;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    cursor: pointer;
    font-size: var(--font-base);
    text-align: left;
    color: var(--text);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      transform var(--motion-press) var(--ease-responsive);
  }

  .section-header:active {
    transform: translateY(1px);
  }

  .section-header:hover {
    background: var(--surface-2);
  }

  .expand-icon {
    display: inline-flex;
    align-items: center;
    color: var(--text-3);
    transition: transform var(--motion-base) var(--ease-responsive);
    flex: none;
  }

  .expand-icon.is-expanded {
    transform: rotate(90deg);
  }

  .section-name {
    flex: 1;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .section-orig-key {
    font-size: var(--font-xs);
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 240px;
    flex: none;
  }

  .section-count {
    font-size: var(--font-sm);
    color: var(--text-3);
    flex: none;
  }

  .member-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    padding: 12px 16px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }

  .grid-status {
    width: 100%;
    text-align: center;
    padding: 16px 0;
    font-size: var(--font-md);
  }

  .load-more-btn {
    width: 100%;
    padding: 8px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--text-2);
    font-size: var(--font-sm);
    cursor: pointer;
  }

  .load-more-btn:hover:not(:disabled) {
    background: var(--surface);
    color: var(--text);
  }

  .load-more-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
