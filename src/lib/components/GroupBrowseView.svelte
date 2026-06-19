<script lang="ts">
  import { getGroupMembers, queryRows, type RowRecord } from "../../api";
  import { errorText, formatCount } from "../app-state.svelte";
  import { groupStore, loadGroups } from "../group-store.svelte";
  import { rowStore } from "../row-store.svelte";
  import { showSectionMenu } from "../section-context-menu.svelte";
  import GroupSectionCard from "./GroupSectionCard.svelte";

  const MEMBERS_PAGE = 200;
  const RENDER_BATCH = 40;

  interface MemberData {
    rows: RowRecord[];
    totalCount: number;
    loading: boolean;
    error: string | null;
  }

  let expandedIds = $state<number[]>([]);
  let memberCache = $state<Record<number, MemberData>>({});
  let renderLimits = $state<Record<string, number>>({});
  let sortByCount = $state(false);

  let ungroupedRows = $state<RowRecord[]>([]);
  let ungroupedTotal = $state(0);
  let ungroupedExpanded = $state(false);
  let ungroupedLoading = $state(false);
  let ungroupedError = $state<string | null>(null);

  $effect(() => {
    void loadGroups();
  });

  const sortedGroups = $derived(
    sortByCount
      ? [...groupStore.list].sort((a, b) => b.memberCount - a.memberCount)
      : groupStore.list,
  );

  function isExpanded(groupId: number): boolean {
    return expandedIds.includes(groupId);
  }

  function getRenderLimit(key: string): number {
    return renderLimits[key] ?? RENDER_BATCH;
  }

  function observeSentinel(node: HTMLElement, key: string) {
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          renderLimits[key] = (renderLimits[key] ?? RENDER_BATCH) + RENDER_BATCH;
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
      expandedIds = expandedIds.filter(id => id !== groupId);
      delete renderLimits[groupId];
    } else {
      expandedIds = [...expandedIds, groupId];
      if (!memberCache[groupId]) {
        await loadGroupMembers(groupId);
      }
    }
  }

  async function loadGroupMembers(groupId: number): Promise<void> {
    memberCache[groupId] = { rows: [], totalCount: 0, loading: true, error: null };
    try {
      const page = await getGroupMembers(groupId, 0, MEMBERS_PAGE);
      memberCache[groupId] = {
        rows: page.rows,
        totalCount: page.totalCount,
        loading: false,
        error: null,
      };
    } catch (e) {
      memberCache[groupId] = {
        rows: [],
        totalCount: 0,
        loading: false,
        error: errorText(e),
      };
    }
  }

  async function loadMoreMembers(groupId: number): Promise<void> {
    const data = memberCache[groupId];
    if (!data || data.loading) return;
    data.loading = true;
    try {
      const page = await getGroupMembers(groupId, data.rows.length, MEMBERS_PAGE);
      data.rows = [...data.rows, ...page.rows];
      data.totalCount = page.totalCount;
      data.loading = false;
      data.error = null;
    } catch (e) {
      data.loading = false;
      data.error = errorText(e);
    }
  }

  async function toggleUngrouped(): Promise<void> {
    ungroupedExpanded = !ungroupedExpanded;
    if (ungroupedExpanded && ungroupedRows.length === 0) {
      await loadUngroupedRows();
    }
    if (!ungroupedExpanded) {
      delete renderLimits["ungrouped"];
    }
  }

  async function loadUngroupedRows(): Promise<void> {
    ungroupedLoading = true;
    ungroupedError = null;
    try {
      const page = await queryRows({
        offset: 0,
        limit: MEMBERS_PAGE,
        tags: [...rowStore.tags],
        tagMode: rowStore.tagMode,
        dedupe: "none",
        singleArtistOnly: rowStore.singleArtistOnly,
        groupView: false,
        hideGrouped: true,
        search: rowStore.search,
      });
      ungroupedRows = page.rows;
      ungroupedTotal = page.totalCount;
    } catch (e) {
      ungroupedError = errorText(e);
    } finally {
      ungroupedLoading = false;
    }
  }

  async function loadMoreUngrouped(): Promise<void> {
    if (ungroupedLoading) return;
    ungroupedLoading = true;
    try {
      const page = await queryRows({
        offset: ungroupedRows.length,
        limit: MEMBERS_PAGE,
        tags: [...rowStore.tags],
        tagMode: rowStore.tagMode,
        dedupe: "none",
        singleArtistOnly: rowStore.singleArtistOnly,
        groupView: false,
        hideGrouped: true,
        search: rowStore.search,
      });
      ungroupedRows = [...ungroupedRows, ...page.rows];
      ungroupedTotal = page.totalCount;
    } catch (e) {
      ungroupedError = errorText(e);
    } finally {
      ungroupedLoading = false;
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
  {#if groupStore.loading}
    <div class="status"><p class="muted">正在加载分组…</p></div>
  {:else if groupStore.error}
    <div class="status"><p class="muted">加载失败：{groupStore.error}</p></div>
  {:else if groupStore.list.length === 0 && !ungroupedExpanded}
    <div class="status"><p class="muted">暂无分组。可通过工具菜单「建议分组」创建。</p></div>
  {:else}
    <div class="group-toolbar">
      <span class="toolbar-label">排序：</span>
      <button
        type="button"
        class:is-active={!sortByCount}
        onclick={() => (sortByCount = false)}
      >按名称</button>
      <button
        type="button"
        class:is-active={sortByCount}
        onclick={() => (sortByCount = true)}
      >按数量</button>
    </div>

    <div class="group-list">
      {#each sortedGroups as group (group.id)}
        {@const expanded = isExpanded(group.id)}
        {@const data = memberCache[group.id]}
        {@const renderKey = String(group.id)}
        {@const limit = getRenderLimit(renderKey)}
        <section class="group-section">
          <button
            type="button"
            class="section-header"
            onclick={() => void toggleGroup(group.id)}
            oncontextmenu={(e) => onHeaderContextMenu(e, group.id, group.name)}
          >
            <span class="expand-icon" class:is-expanded={expanded}>&#9654;</span>
            <span class="section-name">{group.name}</span>
            <span class="section-count">{formatCount(group.memberCount)} 张</span>
          </button>
          {#if expanded}
            <div class="member-grid">
              {#if data?.loading && data.rows.length === 0}
                <p class="muted grid-status">加载中…</p>
              {:else if data?.error}
                <p class="muted grid-status">加载失败：{data.error}</p>
              {:else if data}
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
                    onclick={() => void loadMoreMembers(group.id)}
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
          <span class="expand-icon" class:is-expanded={ungroupedExpanded}>&#9654;</span>
          <span class="section-name">未分组</span>
        </button>
        {#if ungroupedExpanded}
          {@const uLimit = getRenderLimit("ungrouped")}
          <div class="member-grid">
            {#if ungroupedLoading && ungroupedRows.length === 0}
              <p class="muted grid-status">加载中…</p>
            {:else if ungroupedError}
              <p class="muted grid-status">加载失败：{ungroupedError}</p>
            {:else if ungroupedRows.length === 0}
              <p class="muted grid-status">没有未分组的行。</p>
            {:else}
              {#each ungroupedRows.slice(0, uLimit) as row (row.id)}
                <GroupSectionCard {row} onactivate={() => setActive(row)} />
              {/each}
              {#if uLimit < ungroupedRows.length}
                <div class="render-sentinel" use:observeSentinel={"ungrouped"}></div>
              {/if}
              {#if ungroupedRows.length < ungroupedTotal && uLimit >= ungroupedRows.length}
                <button
                  type="button"
                  class="load-more-btn"
                  disabled={ungroupedLoading}
                  onclick={() => void loadMoreUngrouped()}
                >
                  {ungroupedLoading ? "加载中…" : `加载更多（还有 ${formatCount(ungroupedTotal - ungroupedRows.length)} 张）`}
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
    display: flex;
    align-items: center;
    justify-content: center;
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
    font-size: 13px;
    color: var(--text-2);
    margin-right: 4px;
  }

  .group-toolbar button {
    border: 1px solid var(--border);
    background: transparent;
    border-radius: var(--radius-s);
    padding: 4px 12px;
    font-size: 12px;
    color: var(--text-2);
    cursor: pointer;
  }

  .group-toolbar button:hover {
    background: var(--surface-2);
  }

  .group-toolbar button.is-active {
    background: var(--surface-2);
    color: var(--text);
    border-color: var(--border-strong);
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
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 10px 16px;
    border: none;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    cursor: pointer;
    font-size: 14px;
    text-align: left;
    color: var(--text);
  }

  .section-header:hover {
    background: var(--surface-2);
  }

  .expand-icon {
    font-size: 10px;
    color: var(--text-3);
    transition: transform 0.15s ease;
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
    font-size: 12px;
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
    font-size: 13px;
  }

  .load-more-btn {
    width: 100%;
    padding: 8px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--text-2);
    font-size: 12px;
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
