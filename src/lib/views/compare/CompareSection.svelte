<script lang="ts">
  import { formatCount } from "../../utils/format";
  import Images from "@lucide/svelte/icons/images";
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CompareCard from "./CompareCard.svelte";
  import type { RowRecord } from "../../api";
  import type { SectionState } from "../../stores/compare-store.svelte";

  let { title, description, state, emptyText, sampleUnavailable, onLoadMore, onPrevious, onretry, onactivate }: {
    title: string; description?: string; state: SectionState; emptyText: string; sampleUnavailable: boolean;
    onLoadMore: () => void; onPrevious: () => void; onretry: () => void; onactivate: (row: RowRecord) => void;
  } = $props();
  const hasMore = $derived(state.offset + state.items.length < state.total);
  let root: HTMLElement;
  $effect(() => { void state.offset; root?.closest('main')?.scrollTo({ top: 0 }); });
</script>
<section class="compare-section" bind:this={root} aria-busy={state.loading}>
  <header class="section-head"><div><div class="eyebrow">关联图片</div><h1>{title} {#if state.loaded}<span class="section-count">{formatCount(state.total)}</span>{/if}</h1><p>{description}</p></div></header>
  {#if state.error}<div class="error" role="alert"><span>{state.error}</span><button onclick={onretry}>重新加载</button></div>{/if}
  {#if state.items.length > 0}
    <div class="section-grid" class:loading={state.loading}>{#each state.items as row (row.id)}<CompareCard {row} onactivate={() => onactivate(row)} />{/each}</div>
    <footer class="pagination"><span>{state.loading ? "正在加载…" : `${formatCount(state.offset + 1)}–${formatCount(state.offset + state.items.length)} / ${formatCount(state.total)} 张`}</span><div><button title="上一页" aria-label="上一页" disabled={state.loading || state.offset === 0} onclick={onPrevious}><ChevronLeft size={16} /></button><button title="下一页" aria-label="下一页" disabled={state.loading || !hasMore} onclick={onLoadMore}><ChevronRight size={16} /></button></div></footer>
  {:else if state.loading}
    <div class="skeleton-grid" role="status" aria-label="正在加载关联图片">{#each Array(6) as _}<div class="skeleton"></div>{/each}</div>
  {:else if !state.error}
    <div class="empty"><span class="empty-icon"><Images size={28} strokeWidth={1.3} /></span><h2>{sampleUnavailable ? "缺少对比信息" : "暂时没有匹配图片"}</h2><p>{sampleUnavailable ? emptyText : "资料库中还没有符合这个关系的图片，可以切换其他关系继续探索。"}</p></div>
  {/if}
</section>
<style>
  .compare-section { display: flex; flex-direction: column; gap: 22px; min-height: 100%; }
  .eyebrow { color: var(--text-3); font-size: 10px; letter-spacing: .1em; margin-bottom: 5px; }
  h1 { font-size: 22px; font-weight: 650; letter-spacing: -.02em; display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .section-count { font-size: 11px; letter-spacing: 0; color: var(--accent); background: var(--accent-soft); padding: 3px 9px; border-radius: 6px; font-variant-numeric: tabular-nums; }
  .section-head p { color: var(--text-3); font-size: 12px; margin-top: 7px; line-height: 1.7; }
  .section-grid, .skeleton-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(155px, 1fr)); gap: 16px; }
  .section-grid.loading { opacity: .6; pointer-events: none; }
  .pagination { display: flex; align-items: center; justify-content: space-between; margin-top: auto; padding-top: 18px; border-top: 1px solid var(--border); color: var(--text-3); font-size: 12px; font-variant-numeric: tabular-nums; }
  .pagination div { display: flex; gap: 8px; }
  .pagination button { display: flex; padding: 8px; border: 1px solid var(--border); background: var(--surface); border-radius: 8px; }
  button:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .empty { display: flex; flex: 1; min-height: 300px; flex-direction: column; align-items: center; justify-content: center; gap: 12px; text-align: center; padding: 35px; border: 1px dashed var(--border-strong); border-radius: 14px; background: var(--surface); }
  .empty-icon { display: flex; padding: 16px; background: var(--surface-2); color: var(--text-3); border-radius: 16px; }
  .empty h2 { font-size: 15px; font-weight: 600; }.empty p { max-width: 310px; font-size: 12px; line-height: 1.8; color: var(--text-3); }
  .error { display: flex; align-items: center; justify-content: space-between; gap: 15px; background: var(--danger-soft); color: var(--danger); padding: 14px; border-radius: 10px; font-size: 12px; overflow-wrap: anywhere; }
  .error button { flex: none; background: var(--surface); border: 1px solid var(--border); border-radius: 6px; padding: 6px 10px; }
  .skeleton { aspect-ratio: 4/6; border-radius: 12px; background: linear-gradient(120deg, var(--surface-3), var(--surface), var(--surface-3)); }
</style>
