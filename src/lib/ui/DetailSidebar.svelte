<script lang="ts">
  import type { Snippet } from "svelte";
  import ChevronsLeft from "@lucide/svelte/icons/chevrons-left";
  import { panelSlide, softFade } from "./motion";
  let { open, onopen, children }: { open: boolean; onopen: () => void; children: Snippet } = $props();
</script>

{#if open}
  <aside class="detail" transition:panelSlide={{ duration: 200 }}><div class="detail-inner">{@render children()}</div></aside>
{:else}
  <button type="button" class="detail-strip" title="展开详情面板" aria-label="展开详情面板" onclick={onopen} transition:softFade={{ duration: 120 }}><ChevronsLeft size={13} strokeWidth={1.8} /></button>
{/if}

<style>
  .detail { --detail-width: 340px; width: var(--detail-width); flex: none; background: var(--surface); border-left: 1px solid var(--border); display: flex; flex-direction: row; justify-content: flex-end; min-height: 0; overflow: hidden; }
  .detail-inner { width: var(--detail-width); flex: none; display: flex; flex-direction: column; min-height: 0; }
  @media (max-width: 1240px) { .detail { --detail-width: 300px; } }
  @media (max-width: 1080px) { .detail { --detail-width: 270px; } }
  .detail-strip { width: 22px; flex: none; display: flex; align-items: center; justify-content: center; border: none; border-left: 1px solid var(--border); background: var(--surface); color: var(--text-3); transition: background var(--motion-fast) var(--ease-responsive), color var(--motion-fast) var(--ease-responsive); }
  .detail-strip:hover { background: var(--surface-2); color: var(--text); }
</style>
