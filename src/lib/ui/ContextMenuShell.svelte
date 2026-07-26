<script lang="ts">
  import { tick } from "svelte";
  import type { Snippet } from "svelte";
  import {
    popModalLayer,
    pushModalLayer,
  } from "../stores/modal-layer.svelte";
  import { softPop } from "./motion";

  interface Props {
    open: boolean;
    x: number;
    y: number;
    onclose: () => void;
    children: Snippet;
  }

  let { open, x, y, onclose, children }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);
  let posX = $state(0);
  let posY = $state(0);
  // 缩放生长的锚点：默认从点击点（左上）生长，翻转时换到对应边
  let originX = $state<"left" | "right">("left");
  let originY = $state<"top" | "bottom">("top");

  // 打开期间登记为模态层：全局 Delete/Ctrl+Z 等快捷键短路，Esc 只关本菜单。
  $effect(() => {
    if (!open) {
      return;
    }
    const token = pushModalLayer();
    return () => popModalLayer(token);
  });

  // 打开后按菜单实际尺寸夹取到视口内（靠边右键时向上/向左翻转），并聚焦首项。
  $effect(() => {
    if (!open) {
      return;
    }
    posX = x;
    posY = y;
    originX = "left";
    originY = "top";
    void tick().then(() => {
      if (!menuEl) return;
      const margin = 8;
      const rect = menuEl.getBoundingClientRect();
      let nextX = x;
      let nextY = y;
      if (nextX + rect.width + margin > window.innerWidth) {
        nextX = Math.max(margin, window.innerWidth - rect.width - margin);
        originX = "right";
      }
      if (nextY + rect.height + margin > window.innerHeight) {
        // 优先在鼠标上方翻转；空间仍不足时夹取到底部
        const flipped = y - rect.height;
        nextY = flipped >= margin
          ? flipped
          : Math.max(margin, window.innerHeight - rect.height - margin);
        originY = "bottom";
      }
      posX = nextX;
      posY = nextY;
      firstItem()?.focus();
    });
  });

  function items(): HTMLButtonElement[] {
    if (!menuEl) return [];
    return [...menuEl.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')];
  }

  function firstItem(): HTMLButtonElement | undefined {
    return items()[0];
  }

  function handlePointerDown(event: PointerEvent): void {
    if (open && menuEl && !menuEl.contains(event.target as Node)) {
      onclose();
    }
  }

  /** 菜单内键盘导航：上下循环、Home/End、Enter 由按钮原生处理。 */
  function onMenuKeydown(event: KeyboardEvent): void {
    const list = items();
    if (list.length === 0) return;
    const current = list.indexOf(document.activeElement as HTMLButtonElement);
    let next = -1;
    switch (event.key) {
      case "ArrowDown":
        next = current < 0 ? 0 : (current + 1) % list.length;
        break;
      case "ArrowUp":
        next = current < 0 ? list.length - 1 : (current - 1 + list.length) % list.length;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = list.length - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    list[next]?.focus();
  }
</script>

<svelte:window
  onpointerdown={handlePointerDown}
  onkeydown={event => {
    if (event.key === "Escape" && open) {
      event.preventDefault();
      onclose();
    }
  }}
/>

{#if open}
  <div
    class="ctx-menu"
    bind:this={menuEl}
    role="menu"
    tabindex="-1"
    style:left="{posX}px"
    style:top="{posY}px"
    style:transform-origin="{originY} {originX}"
    transition:softPop={{ duration: 145, y: -4, start: 0.98 }}
    onkeydown={onMenuKeydown}
  >
    {@render children()}
  </div>
{/if}

<style>
  .ctx-menu {
    position: fixed;
    z-index: var(--z-menu);
    min-width: 160px;
    max-height: calc(100vh - 16px);
    overflow-y: auto;
    padding: 4px;
    background: var(--surface);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-2);
    display: flex;
    flex-direction: column;
    gap: 1px;
    outline: none;
  }

  /* 菜单项通用样式（按钮式） */
  .ctx-menu :global(button) {
    display: flex;
    align-items: center;
    width: 100%;
    border: none;
    background: transparent;
    border-radius: var(--radius-s);
    padding: 6px 10px;
    text-align: left;
    font-size: var(--font-md);
    color: var(--text);
    cursor: default;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive);
  }

  .ctx-menu :global(button:hover:not(:disabled)),
  .ctx-menu :global(button:focus-visible) {
    background: var(--surface-2);
    outline: none;
  }

  .ctx-menu :global(button:disabled) {
    color: var(--text-3);
    cursor: not-allowed;
  }

  .ctx-menu :global(button.danger) {
    color: var(--danger);
  }

  .ctx-menu :global(button.danger:disabled) {
    color: var(--text-3);
  }

  .ctx-menu :global(.separator) {
    height: 1px;
    background: var(--border);
    margin: 3px 4px;
  }
</style>
