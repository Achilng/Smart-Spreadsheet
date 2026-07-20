<script lang="ts">
  import Grid2x2 from "@lucide/svelte/icons/grid-2x2";
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
    <span class="icon" aria-hidden="true">
      <Grid2x2 size={9} strokeWidth={1.8} />
    </span>
    <input
      type="range"
      {min}
      {max}
      {value}
      step="1"
      oninput={onInput}
    />
    <span class="icon" aria-hidden="true">
      <Grid2x2 size={12} strokeWidth={1.8} />
    </span>
</div>

<style>
  .size-slider {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: none;
    opacity: 1;
    transition: opacity var(--motion-fast) var(--ease-responsive);
  }

  /* 非画廊/表格视图下隐藏但保留占位，避免顶栏在切换视图时重排 */
  .size-slider.is-hidden {
    visibility: hidden;
    opacity: 0;
    pointer-events: none;
  }

  .icon {
    color: var(--text-3);
    width: 14px;
    height: 14px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0.55;
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
    transition:
      transform var(--motion-fast) var(--ease-responsive),
      background var(--motion-fast) var(--ease-responsive);
  }

  input[type="range"]::-webkit-slider-thumb:hover {
    background: var(--accent-hover);
    transform: scale(1.1);
  }

  input[type="range"]:active::-webkit-slider-thumb {
    transform: scale(1.18);
  }
</style>
