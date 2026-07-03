<script lang="ts">
  import { app } from "../../stores/app-state.svelte";

  const GALLERY_MIN = 120;
  const GALLERY_MAX = 400;
  const TABLE_MIN = 40;
  const TABLE_MAX = 128;

  const isGallery = $derived(app.viewMode === "gallery");
  const isTable = $derived(app.viewMode === "table");
  const visible = $derived(isGallery || isTable);

  const min = $derived(isGallery ? GALLERY_MIN : TABLE_MIN);
  const max = $derived(isGallery ? GALLERY_MAX : TABLE_MAX);
  const value = $derived(isGallery ? app.galleryCardSize : app.tableRowHeight);

  function onInput(event: Event): void {
    const v = Number((event.target as HTMLInputElement).value);
    if (isGallery) {
      app.galleryCardSize = v;
    } else {
      app.tableRowHeight = v;
    }
  }
</script>

<div
  class="size-slider"
  class:is-hidden={!visible}
  title={isGallery ? "卡片大小" : "行高"}
  aria-hidden={!visible}
>
    <svg class="icon" viewBox="0 0 16 16" width="14" height="14">
      <rect x="3" y="3" width="4" height="4" rx="0.8" fill="currentColor" opacity="0.55" />
      <rect x="9" y="3" width="4" height="4" rx="0.8" fill="currentColor" opacity="0.55" />
      <rect x="3" y="9" width="4" height="4" rx="0.8" fill="currentColor" opacity="0.55" />
      <rect x="9" y="9" width="4" height="4" rx="0.8" fill="currentColor" opacity="0.55" />
    </svg>
    <input
      type="range"
      {min}
      {max}
      {value}
      step="1"
      oninput={onInput}
    />
    <svg class="icon" viewBox="0 0 16 16" width="14" height="14">
      <rect x="1" y="1" width="6" height="6" rx="1" fill="currentColor" opacity="0.55" />
      <rect x="9" y="1" width="6" height="6" rx="1" fill="currentColor" opacity="0.55" />
      <rect x="1" y="9" width="6" height="6" rx="1" fill="currentColor" opacity="0.55" />
      <rect x="9" y="9" width="6" height="6" rx="1" fill="currentColor" opacity="0.55" />
    </svg>
</div>

<style>
  .size-slider {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: none;
  }

  /* 非画廊/表格视图下隐藏但保留占位，避免顶栏在切换视图时重排 */
  .size-slider.is-hidden {
    visibility: hidden;
    pointer-events: none;
  }

  .icon {
    color: var(--text-3);
    flex: none;
  }

  input[type="range"] {
    width: 80px;
    height: 4px;
    appearance: none;
    background: var(--border);
    border-radius: 2px;
    outline: none;
    cursor: pointer;
  }

  input[type="range"]::-webkit-slider-thumb {
    appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent);
    border: 2px solid var(--surface);
    box-shadow: 0 0 0 1px var(--border-strong);
    cursor: pointer;
  }

  input[type="range"]::-webkit-slider-thumb:hover {
    background: var(--accent-hover);
  }
</style>
