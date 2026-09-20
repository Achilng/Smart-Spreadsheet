<script lang="ts">
  import type { Snippet } from "svelte";
  import { softFade } from "./motion";
  let { src, alt, placeholder = "正在加载图片…", error, onopen, onmousedown, badges, animate = false }: {
    src: string | null; alt: string; placeholder?: string; error?: string | null;
    onopen: () => void; onmousedown?: (event: MouseEvent) => void; badges?: Snippet; animate?: boolean;
  } = $props();
</script>

<div class="preview-box">
  {#if src}
    <button type="button" class="preview-btn" title="点击放大" aria-label={`放大预览：${alt}`} onclick={onopen} {onmousedown}>
      {#if animate}<span class="crossfade">{#key src}<img {src} {alt} draggable="false" transition:softFade={{ duration: 160 }} />{/key}</span>
      {:else}<img {src} {alt} draggable="false" />{/if}
      {@render badges?.()}
    </button>
  {:else}<span class="faint" title={error ?? undefined}>{error ? "图片不可用" : placeholder}</span>{/if}
</div>

<style>
  .crossfade { position: absolute; inset: 0; display: grid; grid-template: minmax(0, 1fr) / minmax(0, 1fr); place-items: center; }
  .crossfade img { grid-area: 1 / 1; min-height: 0; min-width: 0; }
</style>
