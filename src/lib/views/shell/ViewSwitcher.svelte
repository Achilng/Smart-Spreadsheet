<script lang="ts">
  import { app, formatCount, type ViewMode } from "../../stores/app-state.svelte";
  import { duplicateBrowse } from "../../stores/duplicate-browse-store.svelte";
  import { groupStore } from "../../stores/group-store.svelte";
  import { clearSelection, resetSelectionAnchor } from "../../stores/selection-store.svelte";
  import { VIEW_MODES } from "./view-modes";

  let segmentedEl = $state<HTMLElement | null>(null);
  let buttonEls: HTMLButtonElement[] = $state([]);
  let indicator = $state<{ x: number; w: number } | null>(null);

  function switchView(mode: ViewMode): void {
    const previousMode = app.viewMode;
    const stayedInFlatView =
      (previousMode === "gallery" || previousMode === "table") &&
      (mode === "gallery" || mode === "table");
    app.viewMode = mode;
    resetSelectionAnchor();
    // 画廊/表格共享同一筛选结果，可保留选区；分组/重复项的卡片集合不同，
    // 跨边界时清空，避免批量操作包含当前视图没有显示的行。
    if (previousMode !== mode && !stayedInFlatView) {
      clearSelection();
    }
  }

  function countFor(mode: ViewMode): number | null {
    if (mode === "group" && groupStore.list.length > 0) {
      return groupStore.list.length;
    }
    if (mode === "duplicates" && duplicateBrowse.clusters.length > 0) {
      return duplicateBrowse.clusters.length;
    }
    return null;
  }

  function ariaLabelFor(mode: ViewMode, label: string): string {
    const count = countFor(mode);
    return count != null ? `${label}，${formatCount(count)} 个` : label;
  }

  // 指示器跟随激活按钮：切换视图 / 计数变化 / 媒体查询与窗口尺寸变化时重测。
  // 兄弟按钮宽度变化必然改变收缩包裹的父容器宽度，父级 ResizeObserver 兜底。
  $effect(() => {
    const index = VIEW_MODES.findIndex(view => view.mode === app.viewMode);
    void groupStore.list.length;
    void duplicateBrowse.clusters.length;
    const el = buttonEls[index];
    const parent = segmentedEl;
    if (!el || !parent) {
      return;
    }
    const measure = () => {
      indicator = { x: el.offsetLeft, w: el.offsetWidth };
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    observer.observe(parent);
    return () => observer.disconnect();
  });
</script>

<nav class="segmented" aria-label="视图切换" bind:this={segmentedEl}>
  {#if indicator}
    <span
      class="indicator"
      aria-hidden="true"
      style:transform="translateX({indicator.x}px)"
      style:width="{indicator.w}px"
    ></span>
  {/if}
  {#each VIEW_MODES as view, index (view.mode)}
    <button
      type="button"
      class="seg"
      class:is-active={app.viewMode === view.mode}
      aria-pressed={app.viewMode === view.mode}
      aria-label={ariaLabelFor(view.mode, view.label)}
      onclick={() => switchView(view.mode)}
      bind:this={buttonEls[index]}
    >
      {view.label}
      {#if countFor(view.mode) != null}
        <span class="n" aria-hidden="true">{formatCount(countFor(view.mode) ?? 0)}</span>
      {/if}
    </button>
  {/each}
</nav>

<style>
  .segmented {
    position: relative;
    display: flex;
    background: var(--surface-3);
    border-radius: var(--radius-full);
    padding: 3px;
    gap: 2px;
    flex: none;
  }

  .indicator {
    position: absolute;
    top: 3px;
    bottom: 3px;
    left: 0;
    background: var(--surface);
    border-radius: var(--radius-full);
    box-shadow: 0 1px 4px rgb(0 0 0 / 10%);
    transition:
      transform var(--motion-base) var(--ease-responsive),
      width var(--motion-base) var(--ease-responsive);
  }

  .seg {
    position: relative;
    z-index: 1;
    height: 28px;
    padding: 0 16px;
    border: none;
    background: transparent;
    border-radius: var(--radius-full);
    display: inline-flex;
    align-items: center;
    font-size: 12.5px;
    color: var(--text-2);
    white-space: nowrap;
    transition: color var(--motion-fast) var(--ease-responsive);
  }

  .seg:hover:not(.is-active) {
    color: var(--text);
  }

  .seg.is-active {
    color: var(--text);
    font-weight: 600;
  }

  .seg .n {
    font-size: 10.5px;
    color: var(--text-3);
    margin-left: 6px;
    font-variant-numeric: tabular-nums;
  }

  /* 窄窗口：收紧分段控件内距，隐藏计数角标 */
  @media (max-width: 1100px) {
    .seg {
      padding: 0 11px;
    }

    .seg .n {
      display: none;
    }
  }
</style>
