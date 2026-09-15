<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    viewport = $bindable(null), measuredWidth = $bindable(0), measuredHeight = $bindable(0),
    spacerHeight, busy = false, onscroll, children,
  }: {
    viewport?: HTMLDivElement | null; measuredWidth?: number; measuredHeight?: number;
    spacerHeight?: number; busy?: boolean; onscroll?: () => void; children: Snippet;
  } = $props();
</script>

<div class="gallery-view">
  <div class="gallery-viewport" bind:this={viewport} bind:clientWidth={measuredWidth} bind:clientHeight={measuredHeight} {onscroll} aria-busy={busy}>
    <div class="gallery-content" style:height={spacerHeight === undefined ? "100%" : `${spacerHeight}px`} role={spacerHeight === undefined ? undefined : "list"}>
      {@render children()}
    </div>
  </div>
</div>

<style>
  .gallery-view { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; }
  .gallery-viewport {
    flex: 1; min-height: 0; overflow-y: auto; overflow-anchor: none; position: relative;
    /* Leave room for the gallery selection bar and the last row's shadow. */
    padding-bottom: 64px;
  }
  .gallery-content { position: relative; }
</style>
