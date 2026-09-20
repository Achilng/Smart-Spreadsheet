<script lang="ts">
  import type { Snippet } from "svelte";
  import CardTagSummary from "../../ui/CardTagSummary.svelte";

  let {
    x, y, width, imageHeight, title = "", subtitle, label = title, titleAlign = "left",
    isActive = false, isChecked = false, selectionActive = false, skeleton = false,
    showCheckbox = false, selectionLabel = "选择图片", tags = [], image, upperLeft, upperRight,
    onclick, ondblclick, oncontextmenu, onmousedown, onmousedowncapture, oncheck,
  }: {
    x: number; y: number; width: number; imageHeight: number;
    title?: string; subtitle?: string; label?: string; titleAlign?: "left" | "center";
    isActive?: boolean; isChecked?: boolean; selectionActive?: boolean; skeleton?: boolean;
    showCheckbox?: boolean; selectionLabel?: string; tags?: string[];
    image?: Snippet; upperLeft?: Snippet; upperRight?: Snippet;
    onclick?: (event: MouseEvent) => void; ondblclick?: (event: MouseEvent) => void;
    oncontextmenu?: (event: MouseEvent) => void; onmousedown?: (event: MouseEvent) => void;
    onmousedowncapture?: (event: MouseEvent) => void; oncheck?: (event: MouseEvent) => void;
  } = $props();
</script>

<div class="card" class:is-active={isActive} class:is-checked={isChecked}
  class:is-skeleton={skeleton} class:selection-active={selectionActive}
  style:left="{x}px" style:top="{y}px" style:width="{width}px" role="listitem" {oncontextmenu}>
  {#if !skeleton}
    {#if showCheckbox}
      <input type="checkbox" class="select-box" checked={isChecked} aria-label={selectionLabel} onclick={oncheck} />
    {/if}
    <button type="button" class="thumb" style:height="{imageHeight}px" aria-label={label} aria-pressed={isActive}
      {onmousedowncapture} {onmousedown} {onclick} {ondblclick}>
      {@render image?.()}
      {#if upperLeft}<span class="upper-left">{@render upperLeft()}</span>{/if}
      {#if tags.length}<span class="tag-overlay"><CardTagSummary {tags} /></span>{/if}
      {#if upperRight}<span class="upper-right">{@render upperRight()}</span>{/if}
    </button>
    <div class="meta">
      <div class="meta-name" title={title} style:text-align={titleAlign}>{title}</div>
      {#if subtitle}<div class="meta-sub tabular">{subtitle}</div>{/if}
    </div>
  {:else}
    <div class="thumb shimmer" style:height="{imageHeight}px"></div>
    <div class="meta"><span class="skeleton-line shimmer"></span></div>
  {/if}
</div>

<style>
  .card {
    position: absolute;
    display: flex;
    flex-direction: column;
  }

  /* 画册式：图片框单独承担圆角/阴影/抬升，图注裸排框下 */
  .thumb {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    border: none;
    padding: 0;
    background: var(--surface);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-1);
    overflow: hidden;
    transition:
      transform var(--motion-fast) var(--ease-responsive),
      box-shadow var(--motion-fast) var(--ease-responsive);
  }

  .card:hover:not(.is-skeleton) .thumb {
    transform: translateY(-2px);
    box-shadow: var(--shadow-hover);
  }

  .card:active:not(.is-skeleton) .thumb {
    transform: translateY(-2px) scale(0.99);
    transition-duration: var(--motion-press);
  }

  .card.is-active .thumb {
    outline: 2.5px solid var(--accent);
    outline-offset: 2px;
  }

  .card.is-checked:not(.is-active) .thumb {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .select-box {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: var(--z-nav);
    width: 18px;
    height: 18px;
    margin: 0;
    cursor: pointer;
    opacity: 0;
    transform: scale(0.85);
    transition:
      opacity var(--motion-fast) var(--ease-responsive),
      transform var(--motion-fast) var(--ease-responsive);
  }

  /* 卡片选中框走系统蓝（覆盖全局墨黑勾选底） */
  .select-box:checked {
    background: var(--accent);
    border-color: var(--accent);
  }

  .card:hover .select-box,
  .card.is-checked .select-box,
  .card.selection-active .select-box,
  .select-box:focus-visible {
    opacity: 1;
    transform: scale(1);
  }

  .upper-right {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 1;
  }

  .upper-left {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: 1;
    transition: transform var(--motion-fast) var(--ease-responsive);
  }

  /* 选择框可见（悬停/选中/批量选择中）时徽章下移，让出左上角 */
  .card:hover .upper-left,
  .card.is-checked .upper-left,
  .card.selection-active .upper-left {
    transform: translateY(26px);
  }

  .tag-overlay {
    position: absolute;
    left: 8px;
    right: 8px;
    bottom: 8px;
    z-index: 1;
    display: flex;
    min-width: 0;
  }

  .meta {
    padding: 8px 3px 0;
    min-width: 0;
  }

  .meta-name {
    font-size: var(--font-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta-sub {
    margin-top: 1px;
    font-size: var(--font-xs);
    color: var(--text-3);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skeleton-line {
    display: inline-block;
    height: 10px;
    width: 60%;
    border-radius: var(--radius-s);
    background: var(--surface-2);
  }

  .thumb:focus-visible { outline: 2.5px solid var(--accent); outline-offset: 2px; }
</style>
