<script lang="ts">
  import ImageOff from "@lucide/svelte/icons/image-off";
  import ArrowUpRight from "@lucide/svelte/icons/arrow-up-right";
  import type { RowRecord } from "../../api";
  import { thumbnails } from "../../images/thumbnails";
  import { rowFileName, rowResolution } from "../../utils/row-display";
  import { modelVersionBadge } from "../../utils/model-version";
  let { row, onactivate }: { row: RowRecord; onactivate: () => void } = $props();
  const hasImage = $derived(Boolean(row.imagePath?.trim() || row.storedImagePath?.trim()));
  const label = $derived(row.note?.trim() || rowFileName(row) || `#${row.sourceOrdinal}`);
  const resolution = $derived(rowResolution(row));
  const badge = $derived(modelVersionBadge(row.generationModel));
  let element: HTMLButtonElement;
  let url = $state<string | null>(null);
  let failed = $state(false);
  $effect(() => {
    const id = row.id;
    url = thumbnails.cached(id) ?? null;
    failed = false;
    if (!hasImage || url) return;
    let cancelled = false;
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return;
      observer.disconnect();
      void thumbnails.load(id).then(loaded => { if (!cancelled) url = loaded; }, () => { if (!cancelled) failed = true; });
    }, { rootMargin: "160px" });
    observer.observe(element);
    return () => { cancelled = true; observer.disconnect(); };
  });
</script>

<button bind:this={element} type="button" class="compare-card" title={[rowFileName(row), resolution, row.imagePath].filter(Boolean).join("\n")} onclick={onactivate}>
  <span class="thumb">
    {#if url}<img src={url} alt={label} decoding="async" draggable="false" />
    {:else if !hasImage || failed}<span class="unavailable"><ImageOff size={23} strokeWidth={1.4} /><small>{failed ? "预览不可用" : "无图片"}</small></span>
    {:else}<span class="placeholder"></span>{/if}
    {#if badge}<span class="version-badge {badge.className}">{badge.label}</span>{/if}
    {#if row.vibeReferenceCount}<span class="vibe-badge">VIBE ×{row.vibeReferenceCount}</span>{/if}
    <span class="open-hint"><ArrowUpRight size={14} />对比这张</span>
  </span>
  <span class="card-info"><span class="card-label">{label}</span><span class="card-sub"><span>{resolution || '尺寸未知'}</span><span>#{row.id}</span></span></span>
</button>

<style>
  .compare-card { display: flex; flex-direction: column; width: 100%; min-width: 0; padding: 0; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); overflow: hidden; text-align: left; transition: border-color 120ms, box-shadow 120ms; }
  .compare-card:hover { border-color: var(--accent-soft-border); box-shadow: 0 5px 20px rgb(0 0 0 / 7%); }
  .compare-card:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .thumb { position: relative; width: 100%; aspect-ratio: 4 / 5; display: flex; align-items: center; justify-content: center; background: var(--surface-3); overflow: hidden; }
  .thumb img { display: block; width: 100%; height: 100%; object-fit: contain; }
  .version-badge { position: absolute; top: 9px; left: 9px; }
  .vibe-badge { position: absolute; top: 9px; right: 9px; }
  .open-hint { position: absolute; bottom: 10px; right: 10px; display: flex; align-items: center; gap: 4px; border-radius: 6px; padding: 5px 8px; color: white; background: rgb(20 29 45 / 80%); font-size: 10px; opacity: 0; transition: opacity 120ms; }
  .compare-card:hover .open-hint, .compare-card:focus-visible .open-hint { opacity: 1; }
  .card-info { padding: 10px 12px; width: 100%; min-width: 0; }
  .card-label { display: block; font-size: 12px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .card-sub { display: flex; justify-content: space-between; gap: 6px; font-size: 10px; color: var(--text-3); margin-top: 5px; font-variant-numeric: tabular-nums; }
  .unavailable { color: var(--text-3); display: flex; align-items: center; flex-direction: column; gap: 8px; }
  .unavailable small { font-size: 11px; }
  .placeholder { width: 34px; height: 4px; border-radius: 4px; background: var(--border-strong); }
  @media (prefers-reduced-motion: reduce) { .compare-card, .open-hint { transition: none; } }
</style>