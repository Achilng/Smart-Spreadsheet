<script lang="ts">
  import CompareCard from "./CompareCard.svelte";
  import type { RowRecord } from "../../api";
  import type { SectionState } from "../../stores/compare-store.svelte";
  import { formatCount } from "../../stores/app-state.svelte";

  let {
    title,
    description,
    state,
    emptyText,
    sampleUnavailable,
    onLoadMore,
    onactivate,
  }: {
    title: string;
    description?: string;
    state: SectionState;
    /** 样本缺少该分区所需信息时显示的专用空态文案。 */
    emptyText: string;
    sampleUnavailable: boolean;
    onLoadMore: () => void;
    onactivate: (row: RowRecord) => void;
  } = $props();

  const hasMore = $derived(state.items.length < state.total);
</script>

<section class="compare-section">
  <header class="section-head">
    <h2 class="section-title">{title}</h2>
    {#if state.total > 0}
      <span class="section-count">{formatCount(state.total)} 张</span>
    {/if}
    {#if description}
      <span class="section-desc">{description}</span>
    {/if}
  </header>

  {#if state.items.length > 0}
    <div class="section-grid">
      {#each state.items as row (row.id)}
        <CompareCard {row} onactivate={() => onactivate(row)} />
      {/each}
    </div>
    {#if state.loading}
      <div class="section-status">正在加载…</div>
    {:else if hasMore}
      <button type="button" class="load-more" onclick={onLoadMore}>
        加载更多（还剩 {formatCount(state.total - state.items.length)} 张）
      </button>
    {/if}
  {:else if state.loading}
    <div class="section-status">正在加载…</div>
  {:else if state.error}
    <div class="section-state error">{state.error}</div>
  {:else if sampleUnavailable}
    <div class="section-state">{emptyText}</div>
  {:else if state.total === 0}
    <div class="section-state">没有找到符合条件的图片。</div>
  {/if}
</section>

<style>
  .compare-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 22px 26px 6px;
    border-top: 1px solid var(--border);
  }

  .section-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }

  .section-title {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 700;
    color: var(--text);
  }

  .section-count {
    flex: none;
    font-size: var(--font-xs);
    font-weight: 600;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    padding: 2px 8px;
    border-radius: 999px;
  }

  .section-desc {
    font-size: var(--font-xs);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .section-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(128px, 1fr));
    gap: 14px;
  }

  .section-status {
    font-size: var(--font-sm);
    color: var(--text-3);
    padding: 4px 2px;
  }

  .section-state {
    font-size: var(--font-sm);
    color: var(--text-3);
    background: var(--surface);
    border-radius: var(--radius-m);
    padding: 18px 16px;
    text-align: center;
  }

  .section-state.error {
    color: var(--danger, #b3261e);
    word-break: break-all;
  }

  .load-more {
    align-self: flex-start;
    border: none;
    background: transparent;
    color: var(--accent);
    font-size: var(--font-sm);
    font-weight: 600;
    padding: 6px 10px;
    cursor: pointer;
    border-radius: var(--radius-s);
  }

  .load-more:hover {
    background: var(--surface-2);
  }
</style>
