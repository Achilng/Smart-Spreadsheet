<script lang="ts">
  import type { Snippet } from "svelte";
  let { src, alt, placeholder = "正在加载图片…", error, onopen, onmousedown, badges }: {
    src: string | null; alt: string; placeholder?: string; error?: string | null;
    onopen: () => void; onmousedown?: (event: MouseEvent) => void; badges?: Snippet;
  } = $props();
</script>

<div class="preview-box">
  {#if src}
    <button type="button" class="preview-btn" title="点击放大" aria-label={`放大预览：${alt}`} onclick={onopen} {onmousedown}>
      <img {src} {alt} draggable="false" />
      {@render badges?.()}
    </button>
  {:else}<span class="faint" title={error ?? undefined}>{error ? "图片不可用" : placeholder}</span>{/if}
</div>
