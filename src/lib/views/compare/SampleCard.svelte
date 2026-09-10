<script lang="ts">
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Copy from "@lucide/svelte/icons/copy";
  import type { RowRecord } from "../../api";
  import { setNotice } from "../../stores/app-state.svelte";
  import { modelVersionBadge } from "../../utils/model-version";
  import { rowFileName, rowResolution } from "../../utils/row-display";
  import PaneImage from "./PaneImage.svelte";
  let { row, refreshing, onrefresh }: { row: RowRecord; refreshing: boolean; onrefresh: () => void } = $props();
  const hasImage = $derived(Boolean(row.imagePath?.trim() || row.storedImagePath?.trim()));
  const badge = $derived(modelVersionBadge(row.generationModel));
  const title = $derived(row.note?.trim() || rowFileName(row) || `第 ${row.sourceOrdinal} 张`);
  let promptsOpen = $state(false);
  const fields = $derived([{ label: '正向提示词', value: row.positivePrompt }, { label: '角色提示词', value: row.characterPrompt }, { label: '负向提示词', value: row.negativePrompt }]);
  async function copy(text: string) { try { await navigator.clipboard.writeText(text); setNotice({ tone: 'success', text: '已复制到剪贴板。' }); } catch { setNotice({ tone: 'error', text: '复制失败，请重试。' }); } }
</script>
<section class="sample-card">
  <div class="sample-media">{#if hasImage}<PaneImage rowId={row.id} {hasImage} alt={title} tier="gallery" />{:else}<span>无图片</span>{/if}{#if badge}<span class="version-badge {badge.className}">{badge.label}</span>{/if}{#if row.vibeReferenceCount}<span class="vibe-badge">VIBE ×{row.vibeReferenceCount}</span>{/if}</div>
  <div class="sample-head"><h2 title={row.imagePath ?? undefined}>{title}</h2><button class="refresh" title="刷新样本与关联结果" aria-label="刷新样本与关联结果" disabled={refreshing} onclick={onrefresh}><RefreshCw size={14} /></button></div>
  <p class="facts">{rowResolution(row) || '尺寸未知'}<span>·</span>{row.time?.split(' ')[0] || '时间未知'}</p>
  {#if row.artists?.trim()}<div class="artists"><span>{row.artists.trim()}</span><button title="复制画师串" aria-label="复制画师串" onclick={() => void copy(row.artists!.trim())}><Copy size={12} /></button></div>{/if}
  <button class="toggle" aria-expanded={promptsOpen} onclick={() => promptsOpen = !promptsOpen}>样本提示词<ChevronDown size={13} style={promptsOpen ? 'transform: rotate(180deg)' : undefined} /></button>
  {#if promptsOpen}<div class="prompt-list">{#each fields as field}<div><header><span>{field.label}</span>{#if field.value}<button onclick={() => void copy(field.value!)}>复制</button>{/if}</header><pre>{field.value || '（空）'}</pre></div>{/each}</div>{/if}
</section>
<style>
  .sample-card { display: flex; flex-direction: column; gap: 10px; }
  .sample-media { position: relative; width: 100%; aspect-ratio: 1; display: flex; justify-content: center; align-items: center; background: var(--surface-3); border-radius: 12px; overflow: hidden; border: 1px solid var(--border-faint); color: var(--text-3); font-size: 12px; }
  .version-badge { position: absolute; top: 9px; left: 9px; }.vibe-badge { position: absolute; top: 9px; right: 9px; }
  .sample-head { display: flex; gap: 8px; align-items: center; margin-top: 3px; }h2 { flex: 1; font-size: 13px; font-weight: 650; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { border: 0; background: transparent; color: var(--text-3); padding: 4px; border-radius: 5px; display: inline-flex; align-items: center; }button:hover { background: var(--surface-2); color: var(--accent); }button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .facts { display: flex; gap: 7px; font-size: 10px; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .artists { padding: 9px 10px; border-radius: 8px; background: var(--surface-2); display: flex; align-items: flex-start; gap: 5px; font-size: 11px; color: var(--text-2); }.artists span { flex: 1; min-width: 0; white-space: pre-line; overflow-wrap: anywhere; max-height: 4.5em; overflow: auto; }.artists button { flex: none; }
  .toggle { padding: 5px 0; justify-content: space-between; font-size: 11px; }
  .prompt-list { display: flex; flex-direction: column; gap: 12px; }.prompt-list header { display: flex; align-items: center; justify-content: space-between; font-size: 11px; color: var(--text-2); }.prompt-list button { font-size: 10px; color: var(--accent); }
  pre { margin: 5px 0 0; max-height: 150px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; font: 11px/1.7 var(--font); background: var(--surface-2); padding: 9px; border-radius: 8px; color: var(--text-2); }
</style>
