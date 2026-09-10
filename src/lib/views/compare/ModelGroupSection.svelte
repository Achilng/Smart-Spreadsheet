<script lang="ts">
  import Images from "@lucide/svelte/icons/images";
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CompareCard from "./CompareCard.svelte";
  import type { RowRecord } from "../../api";
  import type { CompareModelSection } from "../../api/compare";
  import { formatCount } from "../../stores/app-state.svelte";
  import { modelComparisonTier, modelVersionBadge } from "../../utils/model-version";
  let { section, sampleModel, sampleUnavailable, loading, error, onretry, onactivate }: {
    section: CompareModelSection; sampleModel: string | null; sampleUnavailable: boolean; loading: boolean; error: string | null;
    onretry: () => void; onactivate: (row: RowRecord) => void;
  } = $props();
  const groups = $derived.by(() => {
    const byTier = new Map<string, { tier: string; title: string; rank: number; rows: RowRecord[] }>();
    const sampleTier = modelComparisonTier(sampleModel);
    for (const row of section.rows) {
      const tier = modelComparisonTier(row.generationModel);
      if (tier === sampleTier) continue;
      const badge = modelVersionBadge(row.generationModel);
      let group = byTier.get(tier);
      if (!group) {
        group = { tier, title: badge?.label || row.generationModel?.trim() || '未知模型', rank: badge ? Number(/^v(\d+(?:\.\d+)?)/.exec(badge.label)?.[1] ?? 0) : -1, rows: [] };
        byTier.set(tier, group);
      }
      group.rows.push(row);
    }
    return [...byTier.values()].sort((a, b) => b.rank - a.rank || a.title.localeCompare(b.title));
  });
  const visibleCount = $derived(groups.reduce((count, group) => count + group.rows.length, 0));
  let chosen = $state('');
  let offset = $state(0);
  $effect(() => { void section; chosen = ''; offset = 0; });
  const active = $derived(groups.find(group => group.tier === chosen) ?? groups[0]);
  const visibleRows = $derived(active?.rows.slice(offset, offset + 24) ?? []);
  let root: HTMLElement;
  function turnPage(direction: number) { offset += direction * 24; root?.closest('main')?.scrollTo({ top: 0 }); }
</script>
<section bind:this={root} class="model-section" aria-busy={loading}>
  <header><div class="eyebrow">关联图片</div><h1>相同画风 · 不同模型 {#if !loading}<span>{formatCount(visibleCount)}</span>{/if}</h1><p>保持正向提示词一致，比较不同作画模型的表现。</p></header>
  {#if section.truncated}<div class="truncate-note">仅比较最新 500 张同画风图片。其他模型的结果也可能在更早的图片中。</div>{/if}
  {#if error}<div class="error" role="alert">{error}<button onclick={onretry}>重新加载</button></div>
  {:else if loading}<div class="empty" role="status">正在整理模型结果…</div>
  {:else if active}
    <nav class="model-options" aria-label="模型分组">{#each groups as group (group.tier)}<button class:active={group.tier === active.tier} aria-pressed={group.tier === active.tier} onclick={() => { chosen = group.tier; offset = 0; }}>{group.title}<span>{group.rows.length}</span></button>{/each}</nav>
    <div class="section-grid">{#each visibleRows as row (row.id)}<CompareCard {row} onactivate={() => onactivate(row)} />{/each}</div>
    <footer class="pagination"><span>{formatCount(offset + 1)}–{formatCount(offset + visibleRows.length)} / {formatCount(active.rows.length)} 张</span><div><button aria-label="上一页" disabled={offset === 0} onclick={() => turnPage(-1)}><ChevronLeft size={16} /></button><button aria-label="下一页" disabled={offset + 24 >= active.rows.length} onclick={() => turnPage(1)}><ChevronRight size={16} /></button></div></footer>
  {:else}<div class="empty"><Images size={28} strokeWidth={1.3} /><h2>{sampleUnavailable ? '缺少对比信息' : '暂时没有其他模型的图片'}</h2><p>{sampleUnavailable ? '样本没有可比较的提示词。' : section.truncated ? '最新 500 张同画风图片中，没有其他模型的结果。' : section.totalCount ? '找到的相同画风图片都与样本使用同一模型。' : '资料库中还没有符合条件的图片。'}</p></div>{/if}
</section>
<style>
  .model-section { display: flex; flex-direction: column; gap: 22px; min-height: 100%; }
  .eyebrow { color: var(--text-3); font-size: 10px; letter-spacing: .1em; margin-bottom: 5px; }
  h1 { font-size: 22px; font-weight: 650; display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  h1 span { font-size: 11px; color: var(--accent); background: var(--accent-soft); border-radius: 6px; padding: 3px 9px; }
  header p { color: var(--text-3); font-size: 12px; margin-top: 7px; }
  .model-options { display: flex; flex-wrap: wrap; gap: 8px; }
  .model-options button { display: flex; align-items: center; gap: 8px; border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 8px 12px; font-size: 12px; max-width: 100%; overflow-wrap: anywhere; }.model-options span { font-size: 10px; color: var(--text-3); }.model-options .active { border-color: var(--accent-soft-border); background: var(--accent-soft); color: var(--accent); }
  .section-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(155px, 1fr)); gap: 16px; }
  .pagination { display: flex; justify-content: space-between; align-items: center; margin-top: auto; padding-top: 18px; border-top: 1px solid var(--border); color: var(--text-3); font-size: 12px; }.pagination div { display: flex; gap: 8px; }.pagination button { display: flex; padding: 8px; border: 1px solid var(--border); background: var(--surface); border-radius: 8px; }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .empty { flex: 1; min-height: 300px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; background: var(--surface); border: 1px dashed var(--border-strong); border-radius: 14px; padding: 35px; text-align: center; color: var(--text-3); font-size: 12px; }.empty h2 { font-size: 15px; font-weight: 600; color: var(--text); }.empty p { max-width: 310px; line-height: 1.8; }
  .truncate-note { background: var(--warning-soft); color: var(--warning); padding: 12px 14px; border-radius: 8px; font-size: 12px; }
  .error { display: flex; align-items: center; justify-content: space-between; gap: 12px; background: var(--danger-soft); color: var(--danger); padding: 15px; border-radius: 10px; font-size: 12px; overflow-wrap: anywhere; }.error button { flex: none; padding: 5px 10px; border: 1px solid var(--border); background: var(--surface); border-radius: 6px; }
</style>
