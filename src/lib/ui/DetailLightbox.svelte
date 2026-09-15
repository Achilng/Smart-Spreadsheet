<script lang="ts">
  import { isTopModalLayer, pushModalLayer, popModalLayer } from "../stores/modal-layer.svelte";
  import { softFade, softPop } from "./motion";

  let { open = $bindable(false), src, alt, title, onmousedown }: {
    open?: boolean; src: string | null; alt: string; title?: string;
    onmousedown?: (event: MouseEvent) => void;
  } = $props();
  let dialog = $state<HTMLDivElement | null>(null);
  let token = 0;
  $effect(() => {
    if (!open) return;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    token = pushModalLayer();
    return () => { popModalLayer(token); opener?.focus(); };
  });
  $effect(() => { if (open && dialog) dialog.focus(); });
  function close(event?: KeyboardEvent) {
    if (!isTopModalLayer(token)) return;
    event?.preventDefault();
    open = false;
  }
</script>

<svelte:window onkeydown={event => { if (open && event.key === "Escape") close(event); }} />
{#if open && src}
  <div class="lightbox" bind:this={dialog} role="dialog" aria-modal="true" aria-label="图片放大预览" tabindex="-1"
    onclick={() => close()} onkeydown={event => {
      if (event.key === "Enter") close(event);
      if (event.key === "Tab") event.preventDefault();
    }} onmousedown={event => { if (event.target instanceof HTMLImageElement) onmousedown?.(event); }} transition:softFade={{ duration: 150 }}>
    <img {src} {alt} {title} draggable="false" transition:softPop={{ duration: 190, y: 0, start: 0.98 }} />
  </div>
{/if}

<style>
  .lightbox { position: fixed; inset: 0; z-index: var(--z-lightbox); background: var(--overlay-heavy); display: flex; align-items: center; justify-content: center; padding: 32px; cursor: zoom-out; }
  .lightbox img { max-width: 100%; max-height: 100%; object-fit: contain; border-radius: var(--radius-s); box-shadow: var(--shadow-3); }
</style>
