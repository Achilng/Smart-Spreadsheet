<script lang="ts">
  import type { CompareSectionKind, RowRecord } from "../../api";
  import { compareStore } from "../../stores/compare-store.svelte";
  import CompareCard from "./CompareCard.svelte";
  import { formatCount } from "../../stores/app-state.svelte";

  /** 单个分区（或模型分组）的卡片网格 + 懒加载 + 加载更多。 */
  let {
    kind,
    model = null,
    totalCount,
    onactivate,
  }: {
    kind: CompareSectionKind;
    model?: string | null;
    totalCount: number;
    onactivate: (row: RowRecord) => void;
  } = $props();

  const page = $derived(compareStore.page(kind, model));

  $effect(() => {
    void compareStore.sampleRowId;
    void totalCount;
    void compareStore.ensure(kind, model);
  });

  function retry(): void {
    void compareStore.reload(kind, model);
  }
</script>

{#if page.error}
  <div class="section-error">
    <span>{page.error}</span>
    <button type="button" class="btn btn-ghost" onclick={retry}>重试</button>
  </div>
{:else if page.rows.length === 0 && !page.loading}
  <div class="section-empty">没有加载到图片</div>
{:else}
  <div class="grid">
    {#each page.rows as row (row.id)}
      <CompareCard {row} {onactivate} />
    {/each}
  </div>
  {#if page.loading}
    <div class="loading-more">加载中…</div>
  {:else if page.hasMore}
    <button
      type="button"
      class="load-more-btn"
      onclick={() => void compareStore.loadMore(kind, model)}
    >
      加载更多（还有 {formatCount(page.totalCount - page.rows.length)} 张）
    </button>
  {/if}
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(128px, 1fr));
    gap: 14px 12px;
  }

  .load-more-btn {
    justify-self: center;
    margin-top: 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    padding: 6px 18px;
    font-size: var(--font-sm);
    color: var(--text-2);
    cursor: pointer;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive);
  }

  .load-more-btn:hover {
    background: var(--surface-2);
    color: var(--text);
    border-color: var(--border-strong);
  }

  .loading-more {
    margin-top: 10px;
    text-align: center;
    font-size: var(--font-sm);
    color: var(--text-3);
  }

  .section-error {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--font-sm);
    color: var(--danger);
    padding: 8px 0;
  }

  .section-empty {
    font-size: var(--font-sm);
    color: var(--text-3);
    padding: 8px 0;
  }
</style>
