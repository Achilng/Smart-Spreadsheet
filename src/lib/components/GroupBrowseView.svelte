<script lang="ts">
  import { untrack } from "svelte";
  import { getGroupMembers, queryRows, type RowRecord } from "../../api";
  import { errorText, formatCount } from "../app-state.svelte";
  import { groupStore, loadGroups } from "../group-store.svelte";
  import { rowStore } from "../row-store.svelte";
  import GroupSectionCard from "./GroupSectionCard.svelte";

  const MEMBERS_PAGE = 200;

  interface GroupState {
    expanded: boolean;
    members: RowRecord[];
    totalCount: number;
    loading: boolean;
    error: string | null;
  }

  let groups = $state<Map<number, GroupState>>(new Map());
  let ungroupedRows = $state<RowRecord[]>([]);
  let ungroupedTotal = $state(0);
  let ungroupedExpanded = $state(false);
  let ungroupedLoading = $state(false);
  let ungroupedError = $state<string | null>(null);

  $effect(() => {
    void loadGroups();
  });

  $effect(() => {
    const list = groupStore.list;
    const current = untrack(() => groups);
    groups = new Map(
      list.map(g => [
        g.id,
        current.get(g.id) ?? {
          expanded: false,
          members: [],
          totalCount: g.memberCount,
          loading: false,
          error: null,
        },
      ]),
    );
  });

  async function toggleGroup(groupId: number): Promise<void> {
    const state = groups.get(groupId);
    if (!state) return;
    const wasExpanded = state.expanded;
    state.expanded = !wasExpanded;
    groups = new Map(groups);
    if (!wasExpanded && state.members.length === 0) {
      await loadGroupMembers(groupId);
    }
  }

  async function loadGroupMembers(groupId: number): Promise<void> {
    const state = groups.get(groupId);
    if (!state) return;
    state.loading = true;
    state.error = null;
    groups = new Map(groups);
    try {
      const page = await getGroupMembers(groupId, 0, MEMBERS_PAGE);
      state.members = page.rows;
      state.totalCount = page.totalCount;
    } catch (e) {
      state.error = errorText(e);
    } finally {
      state.loading = false;
      groups = new Map(groups);
    }
  }

  async function loadMoreMembers(groupId: number): Promise<void> {
    const state = groups.get(groupId);
    if (!state || state.loading) return;
    state.loading = true;
    groups = new Map(groups);
    try {
      const page = await getGroupMembers(groupId, state.members.length, MEMBERS_PAGE);
      state.members = [...state.members, ...page.rows];
      state.totalCount = page.totalCount;
    } catch (e) {
      state.error = errorText(e);
    } finally {
      state.loading = false;
      groups = new Map(groups);
    }
  }

  async function toggleUngrouped(): Promise<void> {
    ungroupedExpanded = !ungroupedExpanded;
    if (ungroupedExpanded && ungroupedRows.length === 0) {
      await loadUngroupedRows();
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
      });
      ungroupedRows = [...ungroupedRows, ...page.rows];
      ungroupedTotal = page.totalCount;
    } catch (e) {
      ungroupedError = errorText(e);
    } finally {
      ungroupedLoading = false;
    }
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
  {:else if groupStore.list.length === 0 && ungroupedTotal === 0 && !ungroupedExpanded}
    <div class="status"><p class="muted">暂无分组。可通过工具菜单「建议分组」创建。</p></div>
  {:else}
    {#each groupStore.list as group (group.id)}
      {@const state = groups.get(group.id)}
      {#if state}
        <section class="group-section">
          <button type="button" class="section-header" onclick={() => void toggleGroup(group.id)}>
            <span class="expand-icon" class:is-expanded={state.expanded}>&#9654;</span>
            <span class="section-name">{group.name}</span>
            <span class="section-count">{formatCount(group.memberCount)} 张</span>
          </button>
          {#if state.expanded}
            <div class="member-grid">
              {#if state.loading && state.members.length === 0}
                <p class="muted grid-status">加载中…</p>
              {:else if state.error}
                <p class="muted grid-status">加载失败：{state.error}</p>
              {:else}
                {#each state.members as member (member.id)}
                  <GroupSectionCard row={member} onactivate={() => setActive(member)} />
                {/each}
                {#if state.members.length < state.totalCount}
                  <button
                    type="button"
                    class="load-more-btn"
                    disabled={state.loading}
                    onclick={() => void loadMoreMembers(group.id)}
                  >
                    {state.loading ? "加载中…" : `加载更多（还有 ${formatCount(state.totalCount - state.members.length)} 张）`}
                  </button>
                {/if}
              {/if}
            </div>
          {/if}
        </section>
      {/if}
    {/each}

    <section class="group-section ungrouped-section">
      <button type="button" class="section-header" onclick={() => void toggleUngrouped()}>
        <span class="expand-icon" class:is-expanded={ungroupedExpanded}>&#9654;</span>
        <span class="section-name">未分组</span>
      </button>
      {#if ungroupedExpanded}
        <div class="member-grid">
          {#if ungroupedLoading && ungroupedRows.length === 0}
            <p class="muted grid-status">加载中…</p>
          {:else if ungroupedError}
            <p class="muted grid-status">加载失败：{ungroupedError}</p>
          {:else if ungroupedRows.length === 0}
            <p class="muted grid-status">没有未分组的行。</p>
          {:else}
            {#each ungroupedRows as row (row.id)}
              <GroupSectionCard {row} onactivate={() => setActive(row)} />
            {/each}
            {#if ungroupedRows.length < ungroupedTotal}
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
  {/if}
</div>

<style>
  .group-browse {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 0;
  }

  .status {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
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
