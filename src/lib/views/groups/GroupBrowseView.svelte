<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import { untrack } from "svelte";

  import type { RowRecord } from "../../api";
  import { app, formatCount } from "../../stores/app-state.svelte";
  import {
    ensureGroupMembers,
    ensureUngrouped,
    groupBrowse,
    loadMoreGroupMembers,
    loadMoreUngrouped,
    syncGroupBrowseCaches,
  } from "../../stores/group-browse-store.svelte";
  import { groupStore, loadGroups } from "../../stores/group-store.svelte";
  import { rowStore } from "../../stores/row-store.svelte";
  import { showSectionMenu } from "../../stores/section-context-menu.svelte";
  import { restoreScrollPosition, saveScrollPosition } from "../../stores/view-state";
  import GroupSectionCard from "./GroupSectionCard.svelte";
  import { softFade } from "../../ui/motion";

  let { active = true }: { active?: boolean } = $props();

  const RENDER_BATCH = 40;

  let listEl = $state<HTMLDivElement | null>(null);

  // 数据/成员关系/筛选变化时失效缓存并重载；展开状态和已加载成员跨切换保留。
  // untrack：ensure* 读写 memberCache，不能让缓存写入触发本 effect 重跑。
  $effect(() => {
    if (!active) {
      return;
    }
    void app.dataVersion;
    void groupStore.membershipVersion;
    void rowStore.tags;
    void rowStore.tagMode;
    void rowStore.singleArtistOnly;
    void rowStore.hasVibe;
    void rowStore.search;
    untrack(() => {
      syncGroupBrowseCaches();
      void loadGroups();
      for (const groupId of groupBrowse.expandedIds) {
        void ensureGroupMembers(groupId);
      }
      if (groupBrowse.ungroupedExpanded) {
        void ensureUngrouped();
      }
    });
  });

  // 挂载和每次切回激活时恢复滚动位置。成员网格是异步加载 + content-visibility
  // 按需渲染，切回第一帧总高度可能不足、scrollTop 被浏览器钳制，
  // restoreScrollPosition 内部会按帧重试推回原位。
  let restored = false;
  let restoring = false;

  $effect(() => {
    if (!active) {
      restored = false;
    }
  });

  $effect(() => {
    if (restored || !active || !listEl) {
      return;
    }
    if (groupStore.loading && groupStore.list.length === 0) {
      return;
    }
    restored = true;
    restoring = true;
    restoreScrollPosition(listEl, "groups", 60, undefined, () => (restoring = false));
  });

  function onScroll(): void {
    // 恢复期间的钳制事件不代表用户位置，不能写入记忆
    if (active && !restoring) {
      saveScrollPosition("groups", listEl?.scrollTop ?? 0);
    }
  }

  const sortedGroups = $derived(
    groupBrowse.sortByCount
      ? [...groupStore.list].sort((a, b) => b.memberCount - a.memberCount)
      : groupStore.list,
  );

  function isExpanded(groupId: number): boolean {
    return groupBrowse.expandedIds.includes(groupId);
  }

  function getRenderLimit(key: string): number {
    return groupBrowse.renderLimits[key] ?? RENDER_BATCH;
  }

  function observeSentinel(node: HTMLElement, key: string) {
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          groupBrowse.renderLimits[key] = (groupBrowse.renderLimits[key] ?? RENDER_BATCH) + RENDER_BATCH;
        }
      },
      { rootMargin: "200px" },
    );
    observer.observe(node);
    return {
      destroy() {
        observer.disconnect();
      },
    };
  }

  async function toggleGroup(groupId: number): Promise<void> {
    if (isExpanded(groupId)) {
      groupBrowse.expandedIds = groupBrowse.expandedIds.filter(id => id !== groupId);
      delete groupBrowse.renderLimits[groupId];
    } else {
      groupBrowse.expandedIds = [...groupBrowse.expandedIds, groupId];
      await ensureGroupMembers(groupId);
    }
  }

  async function toggleUngrouped(): Promise<void> {
    groupBrowse.ungroupedExpanded = !groupBrowse.ungroupedExpanded;
    if (groupBrowse.ungroupedExpanded) {
      await ensureUngrouped();
    } else {
      delete groupBrowse.renderLimits["ungrouped"];
    }
  }

  function onHeaderContextMenu(
    event: MouseEvent,
    groupId: number,
    name: string,
  ): void {
    event.preventDefault();
    showSectionMenu(
      { kind: "group", groupId, name },
      event.clientX,
      event.clientY,
    );
  }

  function setActive(row: RowRecord): void {
    rowStore.activeRow = row;
  }
</script>

<div class="group-browse">
  <div class="group-toolbar">
    <span class="toolbar-label">排序：</span>
    <button
      type="button"
      class:is-active={!groupBrowse.sortByCount}
      onclick={() => (groupBrowse.sortByCount = false)}
    >按名称</button>
    <button
      type="button"
      class:is-active={groupBrowse.sortByCount}
      onclick={() => (groupBrowse.sortByCount = true)}
    >按数量</button>
    <button
      type="button"
      class="manage-button"
      onclick={() => (app.groupManageOpen = true)}
    >管理分组</button>
  </div>

  {#if groupStore.loading && groupStore.list.length === 0}
    <div class="status empty-state"><p class="muted">正在加载分组…</p></div>
  {:else if groupStore.error}
    <div class="status empty-state"><p class="muted">加载失败：{groupStore.error}</p></div>
  {:else if groupStore.list.length === 0 && !groupBrowse.ungroupedExpanded}
    <div class="status empty-state"><p class="muted">暂无分组。可选择图片后创建分组。</p></div>
  {:else}
    <div class="group-list" bind:this={listEl} onscroll={onScroll}>
      {#each sortedGroups as group (group.id)}
        {@const expanded = isExpanded(group.id)}
        {@const data = groupBrowse.memberCache[group.id]}
        {@const renderKey = String(group.id)}
        {@const limit = getRenderLimit(renderKey)}
        <section class="group-section">
          <button
            type="button"
            class="section-header"
            onclick={() => void toggleGroup(group.id)}
            oncontextmenu={(e) => onHeaderContextMenu(e, group.id, group.name)}
          >
            <span class="expand-icon" class:is-expanded={expanded}><ChevronRight size={14} strokeWidth={2} /></span>
            <span class="section-name">{group.name}</span>
            <span class="section-count">{formatCount(group.memberCount)} 张</span>
          </button>
          {#if expanded}
            <div class="member-grid" transition:softFade={{ duration: 130 }}>
              {#if !data || (data.loading && data.rows.length === 0)}
                <p class="muted grid-status">加载中…</p>
              {:else if data.error}
                <p class="muted grid-status">加载失败：{data.error}</p>
              {:else}
                {#each data.rows.slice(0, limit) as member (member.id)}
                  <GroupSectionCard row={member} onactivate={() => setActive(member)} />
                {/each}
                {#if limit < data.rows.length}
                  <div class="render-sentinel" use:observeSentinel={renderKey}></div>
                {/if}
                {#if data.rows.length < data.totalCount && limit >= data.rows.length}
                  <button
                    type="button"
                    class="load-more-btn"
                    disabled={data.loading}
                    onclick={() => void loadMoreGroupMembers(group.id)}
                  >
                    {data.loading ? "加载中…" : `加载更多（还有 ${formatCount(data.totalCount - data.rows.length)} 张）`}
                  </button>
                {/if}
              {/if}
            </div>
          {/if}
        </section>
      {/each}

      <section class="group-section ungrouped-section">
        <button type="button" class="section-header" onclick={() => void toggleUngrouped()}>
          <span class="expand-icon" class:is-expanded={groupBrowse.ungroupedExpanded}><ChevronRight size={14} strokeWidth={2} /></span>
          <span class="section-name">未分组</span>
        </button>
        {#if groupBrowse.ungroupedExpanded}
          {@const uData = groupBrowse.ungrouped}
          {@const uLimit = getRenderLimit("ungrouped")}
          <div class="member-grid" transition:softFade={{ duration: 130 }}>
            {#if !uData || (uData.loading && uData.rows.length === 0)}
              <p class="muted grid-status">加载中…</p>
            {:else if uData.error}
              <p class="muted grid-status">加载失败：{uData.error}</p>
            {:else if uData.rows.length === 0}
              <p class="muted grid-status">没有未分组的行。</p>
            {:else}
              {#each uData.rows.slice(0, uLimit) as row (row.id)}
                <GroupSectionCard {row} onactivate={() => setActive(row)} />
              {/each}
              {#if uLimit < uData.rows.length}
                <div class="render-sentinel" use:observeSentinel={"ungrouped"}></div>
              {/if}
              {#if uData.rows.length < uData.totalCount && uLimit >= uData.rows.length}
                <button
                  type="button"
                  class="load-more-btn"
                  disabled={uData.loading}
                  onclick={() => void loadMoreUngrouped()}
                >
                  {uData.loading ? "加载中…" : `加载更多（还有 ${formatCount(uData.totalCount - uData.rows.length)} 张）`}
                </button>
              {/if}
            {/if}
          </div>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .group-browse {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .status {
    flex: 1;
  }

  .group-toolbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .toolbar-label {
    font-size: var(--font-md);
    color: var(--text-2);
    margin-right: 4px;
  }

  .group-toolbar button {
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

  .group-toolbar button:active {
    transform: scale(0.98);
  }

  .group-toolbar button:hover {
    background: var(--surface-2);
  }

  .group-toolbar button.is-active {
    background: var(--surface-2);
    color: var(--text);
    border-color: var(--border-strong);
  }

  .group-toolbar .manage-button {
    margin-left: auto;
  }

  .group-list {
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

  .section-count {
    font-size: var(--font-sm);
    color: var(--text-3);
    flex: none;
  }

  .ungrouped-section .section-name {
    color: var(--text-2);
  }

  .member-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    padding: 12px 16px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
    content-visibility: auto;
    contain-intrinsic-height: auto 500px;
  }

  .render-sentinel {
    width: 100%;
    height: 1px;
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
