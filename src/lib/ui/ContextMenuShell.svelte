<script lang="ts">
  import type { Snippet } from "svelte";
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

  function handlePointerDown(event: PointerEvent): void {
    if (open && menuEl && !menuEl.contains(event.target as Node)) {
      onclose();
    }
  }
</script>

<svelte:window
  onpointerdown={handlePointerDown}
  onkeydown={event => {
    if (event.key === "Escape" && open) onclose();
  }}
/>

{#if open}
  <div
    class="ctx-menu"
    bind:this={menuEl}
    role="menu"
    style:left="{x}px"
    style:top="{y}px"
    transition:softPop={{ duration: 145, y: -4, start: 0.98 }}
  >
    {@render children()}
  </div>
{/if}

<style>
  .ctx-menu {
    position: fixed;
    z-index: var(--z-menu);
    min-width: 160px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-2);
    display: flex;
    flex-direction: column;
    gap: 1px;
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

  .ctx-menu :global(button:hover:not(:disabled)) {
    background: var(--surface-2);
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
