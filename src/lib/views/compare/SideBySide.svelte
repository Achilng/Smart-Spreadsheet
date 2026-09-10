<script lang="ts">
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Crosshair from "@lucide/svelte/icons/crosshair";
  import Maximize2 from "@lucide/svelte/icons/maximize-2";
  import { emitTo } from "@tauri-apps/api/event";
  import type { RowRecord } from "../../api";
  import { closeSideBySide } from "../../stores/compare-store.svelte";
  import { diffPromptField, type PromptToken } from "../../utils/prompt-diff";
  import { modelVersionBadge } from "../../utils/model-version";
  import { focusMainWindow } from "../../windows/toolbox";
  import { rowFileName, rowResolution } from "../../utils/row-display";
  import PaneImage from "./PaneImage.svelte";
  import OriginalLightbox from "./OriginalLightbox.svelte";
  let { sample, target }: { sample: RowRecord; target: RowRecord } = $props();
  let tab = $state<'positive' | 'character' | 'params'>('positive');
  let showShared = $state(false);
  let onlyChangedParams = $state(false);
  const positive = $derived(diffPromptField(sample.positivePrompt, target.positivePrompt));
  const character = $derived(diffPromptField(sample.characterPrompt, target.characterPrompt));
  const diff = $derived(tab === 'character' ? character : positive);
  const changes = $derived(diff.onlyLeft.length + diff.onlyRight.length);
  function value(input: string | number | null | undefined) { return input == null || input === '' ? '—' : String(input); }
  const params = $derived([
    ['模型', value(sample.generationModel), value(target.generationModel)],
    ['采样器', value(sample.generationSampler), value(target.generationSampler)],
    ['步数', value(sample.generationSteps), value(target.generationSteps)],
    ['种子', value(sample.generationSeed), value(target.generationSeed)],
    ['Guidance', value(sample.generationScale), value(target.generationScale)],
    ['CFG Rescale', value(sample.generationCfgRescale), value(target.generationCfgRescale)],
    ['噪声调度', value(sample.generationNoiseSchedule), value(target.generationNoiseSchedule)],
    ['尺寸', value(rowResolution(sample)), value(rowResolution(target))],
  ]);
  const changedParams = $derived(params.filter(row => row[1] !== row[2]).length);
  const panes = $derived([{ label: '样本', mark: 'A', row: sample }, { label: '目标', mark: 'B', row: target }]);
  let originalRow = $state<RowRecord | null>(null);
  async function locate(rowId: number) { try { await emitTo('main', 'toolbox://open-row', { rowId }); await focusMainWindow(); } catch { /* Main window may already be closed. */ } }
</script>
<svelte:window onkeydown={event => { if (event.key === 'Escape' && !originalRow) closeSideBySide(); }} />

{#snippet tokens(items: PromptToken[], quality: boolean)}
  <div class="tokens">{#each items as token, index (index)}<span class="token" class:quality={quality && token.isQuality}>{token.display}</span>{/each}</div>
{/snippet}
<div class="side-by-side">
  <header class="head"><button class="back" onclick={closeSideBySide}><ArrowLeft size={16} />返回结果</button><div><strong>双图对照</strong><span>观察画面，核对创作条件</span></div><small>ESC 返回</small></header>
  <div class="comparison-layout">
    <div class="image-stage">
      <div class="panes">
        {#each panes as pane (pane.mark)}
          {@const hasImage = Boolean(pane.row.imagePath?.trim() || pane.row.storedImagePath?.trim())}
          {@const badge = modelVersionBadge(pane.row.generationModel)}
          <figure class="pane" class:target={pane.mark === 'B'}>
            <figcaption><span class="mark">{pane.mark}</span><strong>{pane.label}</strong><span class="resolution">{rowResolution(pane.row) || '尺寸未知'}</span></figcaption>
            <button class="pane-media" disabled={!hasImage} onclick={() => originalRow = pane.row} aria-label={`查看${pane.label}原图`}>
              {#if hasImage}<PaneImage rowId={pane.row.id} {hasImage} alt={rowFileName(pane.row) ?? pane.label} />{:else}<span>无图片</span>{/if}
              {#if badge}<span class="version-badge {badge.className}">{badge.label}</span>{/if}
              {#if hasImage}<span class="original-hint"><Maximize2 size={13} />查看原图</span>{/if}
            </button>
            <div class="pane-footer"><p title={pane.row.imagePath ?? undefined}>{pane.row.note?.trim() || rowFileName(pane.row) || `#${pane.row.id}`}</p><button onclick={() => void locate(pane.row.id)} title="在主画廊定位"><Crosshair size={12} />定位</button></div>
          </figure>
        {/each}
      </div>
      <p class="stage-note">点击图片查看完整原图 · 两侧均保持原始比例</p>
    </div>
    <aside class="inspector" aria-label="图片差异">
      <header class="inspector-head"><span>差异检查</span><small>A 样本 / B 目标</small></header>
      <nav class="diff-tabs" aria-label="差异类别">
        <button class:active={tab === 'positive'} aria-pressed={tab === 'positive'} onclick={() => tab = 'positive'}>正向提示词</button>
        <button class:active={tab === 'character'} aria-pressed={tab === 'character'} onclick={() => tab = 'character'}>角色提示词</button>
        <button class:active={tab === 'params'} aria-pressed={tab === 'params'} onclick={() => tab = 'params'}>生成参数<span>{changedParams}</span></button>
      </nav>
      <div class="inspector-scroll">
        {#if tab === 'params'}
          <div class="diff-summary"><strong>{changedParams} 项参数不同</strong><button aria-pressed={onlyChangedParams} onclick={() => onlyChangedParams = !onlyChangedParams}>{onlyChangedParams ? '显示全部' : '只看不同'}</button></div>
          {#if onlyChangedParams && !changedParams}<div class="empty">两张图片的生成参数完全一致。</div>{/if}
          <div class="parameter-list">{#each params.filter(row => !onlyChangedParams || row[1] !== row[2]) as row (row[0])}<section class:changed={row[1] !== row[2]}><h3>{row[0]}{#if row[1] !== row[2]}<span>不同</span>{/if}</h3><div><p><small>A</small>{row[1]}</p><p><small>B</small>{row[2]}</p></div></section>{/each}</div>
        {:else}
          <div class="diff-summary"><strong>{changes ? `${changes} 项提示词不同` : '提示词一致'}</strong><span>{diff.shared.length} 项共有</span></div>
          {#if changes === 0}<div class="same-note">{diff.shared.length ? '两侧提示词项一致，可以继续比较生成参数。' : `两侧${tab === 'character' ? '角色' : '正向'}提示词都为空。`}</div>{/if}
          <div class="unique-columns">
            <section class="unique a"><h3><span>A</span>仅样本有 <small>{diff.onlyLeft.length}</small></h3>{#if diff.onlyLeft.length}{@render tokens(diff.onlyLeft, tab === 'positive')}{:else}<p class="empty">没有独有项</p>{/if}</section>
            <section class="unique b"><h3><span>B</span>仅目标有 <small>{diff.onlyRight.length}</small></h3>{#if diff.onlyRight.length}{@render tokens(diff.onlyRight, tab === 'positive')}{:else}<p class="empty">没有独有项</p>{/if}</section>
          </div>
          {#if diff.shared.length}<button class="shared-toggle" aria-expanded={showShared} onclick={() => showShared = !showShared}>{showShared ? '收起' : '展开'}双方共有内容<span>{diff.shared.length}</span></button>{#if showShared}<div class="shared">{@render tokens(diff.shared, tab === 'positive')}</div>{/if}{/if}
          {#if tab === 'positive'}<p class="quality-note">官方质量词淡化显示，重复出现的提示词按实际次数比较。</p>{/if}
        {/if}
      </div>
    </aside>
  </div>
</div>
{#if originalRow}<OriginalLightbox row={originalRow} onclose={() => originalRow = null} />{/if}
<style>
  .side-by-side { display: flex; flex: 1; flex-direction: column; min-height: 0; }
  .head { display: flex; align-items: center; gap: 20px; padding: 16px 22px; background: var(--surface); border-bottom: 1px solid var(--border); }
  .head div { display: flex; align-items: baseline; gap: 10px; }.head strong { font-size: 14px; }.head span, .head small { font-size: 11px; color: var(--text-3); }.head small { margin-left: auto; }
  .back { display: flex; align-items: center; gap: 6px; border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 7px 10px; font-size: 12px; }
  .comparison-layout { flex: 1; display: grid; grid-template-columns: minmax(0, 1.55fr) minmax(340px, 1fr); min-height: 0; }
  .image-stage { padding: 24px; display: flex; flex-direction: column; justify-content: center; min-width: 0; overflow: auto; }
  .panes { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; }
  .pane { margin: 0; min-width: 0; }.pane figcaption { display: flex; align-items: center; gap: 7px; margin-bottom: 10px; font-size: 12px; }.mark { display: inline-flex; justify-content: center; align-items: center; height: 24px; width: 24px; border-radius: 7px; background: var(--accent); color: white; font-size: 11px; font-weight: 700; }.target .mark { background: #138477; }
  .resolution { margin-left: auto; font-size: 10px; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .pane-media { position: relative; width: 100%; height: clamp(220px, 55vh, 650px); display: flex; align-items: center; justify-content: center; background: var(--surface-3); border: 1px solid var(--border-strong); padding: 0; border-radius: 12px; overflow: hidden; cursor: zoom-in; }.pane-media:disabled { opacity: 1; color: var(--text-3); }
  .version-badge { position: absolute; top: 10px; left: 10px; }.original-hint { position: absolute; bottom: 10px; right: 10px; display: flex; align-items: center; gap: 5px; background: rgb(20 29 45 / 75%); color: white; border-radius: 6px; padding: 5px 8px; font-size: 10px; opacity: 0; }.pane-media:hover .original-hint, .pane-media:focus-visible .original-hint { opacity: 1; }
  .pane-footer { display: flex; align-items: center; gap: 6px; margin-top: 10px; }.pane-footer p { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; color: var(--text-2); }.pane-footer button { display: flex; align-items: center; gap: 4px; flex: none; border: 0; background: transparent; color: var(--text-3); font-size: 10px; padding: 4px; }
  .stage-note { text-align: center; margin-top: 22px; color: var(--text-3); font-size: 10px; }
  .inspector { background: var(--surface); border-left: 1px solid var(--border); min-width: 0; display: flex; flex-direction: column; min-height: 0; }.inspector-head { display: flex; align-items: center; justify-content: space-between; padding: 22px 20px 14px; font-size: 13px; font-weight: 650; }.inspector-head small { color: var(--text-3); font-size: 10px; font-weight: 400; }
  .diff-tabs { display: flex; padding: 0 16px; border-bottom: 1px solid var(--border); gap: 4px; }.diff-tabs button { flex: 1; white-space: nowrap; border: 0; border-bottom: 2px solid transparent; padding: 11px 4px; background: transparent; color: var(--text-3); font-size: 12px; }.diff-tabs .active { color: var(--accent); border-bottom-color: var(--accent); font-weight: 600; }.diff-tabs span { margin-left: 5px; font-size: 10px; }
  .inspector-scroll { padding: 20px; overflow: auto; min-height: 0; }.diff-summary { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 18px; }.diff-summary strong { font-size: 12px; font-weight: 600; }.diff-summary > span { font-size: 10px; color: var(--text-3); }.diff-summary button { border: 0; padding: 3px 0; font-size: 11px; background: transparent; color: var(--accent); }
  .unique-columns { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }.unique { border: 1px solid var(--accent-soft-border); border-radius: 10px; overflow: hidden; background: color-mix(in srgb, var(--accent-soft) 35%, var(--surface)); }.unique.b { border-color: #c5e2dc; background: color-mix(in srgb, #eaf6f2 40%, var(--surface)); }
  .unique h3 { display: flex; align-items: center; gap: 5px; padding: 10px; font-size: 11px; font-weight: 600; border-bottom: 1px solid var(--border-faint); color: var(--accent); }.unique.b h3 { color: #138477; }.unique h3 span { font-size: 9px; padding: 1px 4px; border-radius: 4px; background: var(--accent-soft); }.unique.b h3 span { background: #dff2eb; }.unique h3 small { margin-left: auto; font-variant-numeric: tabular-nums; font-weight: 400; }
  .tokens { display: flex; flex-wrap: wrap; align-content: start; gap: 6px; padding: 10px; }.token { font-size: 11px; line-height: 1.6; overflow-wrap: anywhere; max-width: 100%; padding: 3px 7px; background: var(--surface); border: 1px solid var(--border-faint); border-radius: 5px; color: var(--text-2); }.token.quality { color: var(--text-3); background: transparent; border-color: transparent; }
  .empty { padding: 16px 10px; font-size: 11px; color: var(--text-3); }.same-note { padding: 12px; margin-bottom: 16px; border-radius: 8px; font-size: 11px; line-height: 1.8; background: var(--success-soft); color: var(--success); }
  .shared-toggle { display: flex; align-items: center; justify-content: space-between; width: 100%; margin-top: 18px; padding: 12px 0; border: 0; border-bottom: 1px solid var(--border); font-size: 11px; background: transparent; color: var(--text-2); }.shared-toggle span { color: var(--text-3); }.shared { background: var(--surface-2); border-radius: 0 0 8px 8px; }.quality-note { color: var(--text-3); font-size: 10px; line-height: 1.8; margin-top: 18px; }
  .parameter-list { display: flex; flex-direction: column; gap: 9px; }.parameter-list section { border: 1px solid var(--border); border-radius: 8px; overflow: hidden; }.parameter-list h3 { display: flex; align-items: center; justify-content: space-between; font-size: 11px; font-weight: 600; background: var(--surface-2); padding: 8px 10px; color: var(--text-2); }.parameter-list h3 span { font-size: 9px; color: var(--warning); font-weight: 400; }.parameter-list section.changed { border-color: color-mix(in srgb, var(--warning) 30%, var(--border)); }.changed h3 { background: var(--warning-soft); }.parameter-list section > div { display: grid; grid-template-columns: 1fr 1fr; }.parameter-list p { min-width: 0; display: flex; gap: 7px; padding: 10px; font-size: 11px; overflow-wrap: anywhere; font-variant-numeric: tabular-nums; }.parameter-list p + p { border-left: 1px solid var(--border-faint); }.parameter-list small { flex: none; color: var(--text-3); font-size: 9px; }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }button:hover:not(:disabled) { color: var(--accent); }
  @media (max-width: 1000px) { .comparison-layout { grid-template-columns: minmax(0, 1fr) 320px; }.image-stage { padding: 16px; }.panes { gap: 10px; }.resolution { display: none; }.inspector-scroll { padding: 15px; }.head div span { display: none; } }
  @media (max-width: 760px) { .comparison-layout { display: block; overflow: auto; }.inspector { border-left: 0; border-top: 1px solid var(--border); }.inspector-scroll { overflow: visible; }.pane-media { height: 330px; }.stage-note { margin-top: 14px; } }
</style>