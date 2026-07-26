<script module lang="ts">
  export interface DropdownItem {
    label: string;
    hint?: string;
    danger?: boolean;
    action: () => void;
  }
</script>

<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";

  import { softPop } from "./motion";

  let {
    label,
    items,
    disabled = false,
    primary = false,
  }: {
    label: string;
    items: DropdownItem[];
    disabled?: boolean;
    primary?: boolean;
  } = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement | null>(null);

  function onWindowPointerDown(event: PointerEvent): void {
    if (open && root && !root.contains(event.target as Node)) {
      open = false;
    }
  }

  function pick(item: DropdownItem): void {
    open = false;
    item.action();
  }
</script>

<svelte:window
  onpointerdown={onWindowPointerDown}
  onkeydown={event => {
    if (event.key === "Escape") {
      open = false;
    }
  }}
/>

<div class="dropdown" bind:this={root}>
  <button
    type="button"
    class="btn"
    class:btn-primary={primary}
    aria-haspopup="menu"
    aria-expanded={open}
    {disabled}
    onclick={() => (open = !open)}
  >
    {label}
    <span class="caret" class:is-open={open} aria-hidden="true">
      <ChevronDown size={14} strokeWidth={2} />
    </span>
  </button>
  {#if open}
    <div class="menu" role="menu" transition:softPop={{ duration: 150, y: -4, start: 0.98 }}>
      {#each items as item (item.label)}
        <button
          type="button"
          role="menuitem"
          class:danger={item.danger}
          onclick={() => pick(item)}
        >
          <span class="item-label">{item.label}</span>
          {#if item.hint}
            <span class="item-hint">{item.hint}</span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .dropdown {
    position: relative;
    flex: none;
  }

  .caret {
    display: inline-flex;
    align-items: center;
    margin-left: 2px;
    transition: transform var(--motion-fast) var(--ease-responsive);
  }

  .caret.is-open {
    transform: rotate(180deg);
  }

  .menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: var(--z-menu);
    min-width: 200px;
    padding: 4px;
    background: var(--surface);
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-2);
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .menu button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    width: 100%;
    border: none;
    background: transparent;
    border-radius: var(--radius-s);
    padding: 6px 10px;
    text-align: left;
    font-size: var(--font-md);
    color: var(--text);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive);
  }

  .menu button:hover {
    background: var(--surface-2);
  }

  .menu button.danger {
    color: var(--danger);
  }

  .item-hint {
    font-size: var(--font-xs);
    color: var(--text-3);
  }
</style>
