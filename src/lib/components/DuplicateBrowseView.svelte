<script lang="ts">
  import {
    listDedupeClusters,
    getDedupeClusterMembers,
    type DedupeCluster,
    type DedupeMode,
    type RowRecord,
  } from "../../api";
  import { errorText, formatCount } from "../app-state.svelte";
  import { rowStore } from "../row-store.svelte";
  import { sectionMenu, showSectionMenu } from "../section-context-menu.svelte";
  import GroupSectionCard from "./GroupSectionCard.svelte";

  const MEMBERS_PAGE = 200;

  interface MemberData {
    rows: RowRecord[];
    totalCount: number;
    loading: boolean;
    error: string | null;
  }

  let dedupeMode = $state<DedupeMode>("artists");

  let clusters = $state<DedupeCluster[]>([]);
  let clustersLoading = $state(false);
  let clustersError = $state<string | null>(null);

  let expandedKeys = $state<string[]>([]);
  let memberCache = $state<Record<string, MemberData>>({});

  $effect(() => {
    void dedupeMode;
    void rowStore.tags;
    void rowStore.tagMode;
    void rowStore.singleArtistOnly;
    void rowStore.hideGrouped;
    void loadClusters(true);
  });

  $effect(() => {
    void sectionMenu.aliasVersion;
    void loadClusters(false);
  });

  async function loadClusters(resetExpand: boolean): Promise<void> {
    clustersLoading = true;
    clustersError = null;
    if (resetExpand) {
      expandedKeys = [];
      memberCache = {};
    }
    try {
      clusters = await listDedupeClusters(
        dedupeMode,
        [...rowStore.tags],
        rowStore.tagMode,
        rowStore.singleArtistOnly,
        rowStore.hideGrouped,
      );
    } catch (e) {
      clustersError = errorText(e);
      clusters = [];
    } finally {
      clustersLoading = false;
    }
  }

  function isExpanded(key: string): boolean {
    return expandedKeys.includes(key);
  }

  function getMemberData(key: string): MemberData | undefined {
    return memberCache[key];
  }

  async function toggleCluster(key: string): Promise<void> {
    if (isExpanded(key)) {
      expandedKeys = expandedKeys.filter(k => k !== key);
    } else {
      expandedKeys = [...expandedKeys, key];
      if (!memberCache[key]) {
        await loadClusterMembers(key);
      }
    }
  }

  async function loadClusterMembers(key: string): Promise<void> {
    memberCache = {
      ...memberCache,
      [key]: { rows: [], totalCount: 0, loading: true, error: null },
    };
    try {
      const page = await getDedupeClusterMembers(
        dedupeMode,
        key,
        [...rowStore.tags],
        rowStore.tagMode,
        rowStore.singleArtistOnly,
        rowStore.hideGrouped,
        0,
        MEMBERS_PAGE,
      );
      memberCache = {
        ...memberCache,
        [key]: { rows: page.rows, totalCount: page.totalCount, loading: false, error: null },
      };
    } catch (e) {
      memberCache = {
        ...memberCache,
        [key]: { rows: [], totalCount: 0, loading: false, error: errorText(e) },
      };
    }
  }

  async function loadMoreMembers(key: string): Promise<void> {
    const data = memberCache[key];
    if (!data || data.loading) return;
    memberCache = { ...memberCache, [key]: { ...data, loading: true } };
    try {
      const page = await getDedupeClusterMembers(
        dedupeMode,
        key,
        [...rowStore.tags],
        rowStore.tagMode,
        rowStore.singleArtistOnly,
        rowStore.hideGrouped,
        data.rows.length,
        MEMBERS_PAGE,
      );
      memberCache = {
        ...memberCache,
        [key]: {
          rows: [...data.rows, ...page.rows],
          totalCount: page.totalCount,
          loading: false,
          error: null,
        },
      };
    } catch (e) {
      memberCache = { ...memberCache, [key]: { ...data, loading: false, error: errorText(e) } };
    }
  }

  function onHeaderContextMenu(
    event: MouseEvent,
    cluster: DedupeCluster,
  ): void {
    event.preventDefault();
    if (dedupeMode === "none") return;
    showSectionMenu(
      {
        kind: "dedupe",
        mode: dedupeMode,
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
      class:is-active={dedupeMode === "artists"}
      onclick={() => (dedupeMode = "artists")}
    >按画师串</button>
    <button
      type="button"
      class:is-active={dedupeMode === "positivePrompt"}
      onclick={() => (dedupeMode = "positivePrompt")}
    >按正向提示词</button>
  </div>

  {#if clustersLoading}
    <div class="status"><p class="muted">正在加载重复项…</p></div>
  {:else if clustersError}
    <div class="status"><p class="muted">加载失败：{clustersError}</p></div>
  {:else if clusters.length === 0}
    <div class="status"><p class="muted">未找到重复项（所有条目均唯一）。</p></div>
  {:else}
    <div class="cluster-list">
      {#each clusters as cluster (cluster.key)}
        {@const expanded = isExpanded(cluster.key)}
        {@const data = getMemberData(cluster.key)}
        <section class="group-section">
          <button
            type="button"
            class="section-header"
            onclick={() => void toggleCluster(cluster.key)}
            oncontextmenu={(e) => onHeaderContextMenu(e, cluster)}
          >
            <span class="expand-icon" class:is-expanded={expanded}>&#9654;</span>
            <span class="section-name">{cluster.alias ?? cluster.key}</span>
            {#if cluster.alias}
              <span class="section-orig-key" title={cluster.key}>({cluster.key.slice(0, 30)}{cluster.key.length > 30 ? "…" : ""})</span>
            {/if}
            <span class="section-count">{formatCount(cluster.memberCount)} 张</span>
          </button>
          {#if expanded}
            <div class="member-grid">
              {#if data?.loading && data.rows.length === 0}
                <p class="muted grid-status">加载中…</p>
              {:else if data?.error}
                <p class="muted grid-status">加载失败：{data.error}</p>
              {:else if data}
                {#each data.rows as member (member.id)}
                  <GroupSectionCard row={member} onactivate={() => setActive(member)} />
                {/each}
                {#if data.rows.length < data.totalCount}
                  <button
                    type="button"
                    class="load-more-btn"
                    disabled={data.loading}
                    onclick={() => void loadMoreMembers(cluster.key)}
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
    font-size: 13px;
    color: var(--text-2);
    margin-right: 4px;
  }

  .mode-bar button {
    border: 1px solid var(--border);
    background: transparent;
    border-radius: var(--radius-s);
    padding: 4px 12px;
    font-size: 12px;
    color: var(--text-2);
    cursor: pointer;
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
    display: flex;
    align-items: center;
    justify-content: center;
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

  .section-orig-key {
    font-size: 11px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 240px;
    flex: none;
  }

  .section-count {
    font-size: 12px;
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
