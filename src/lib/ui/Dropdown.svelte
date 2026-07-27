<script module lang="ts">
  export interface DropdownItem {
    label: string;
    hint?: string;
    danger?: boolean;
    checked?: boolean;
    action: () => void;
  }
</script>

<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Check from "@lucide/svelte/icons/check";

  import { softPop } from "./motion";

  let {
    label,
    items,
    disabled = false,
    primary = false,
    ghost = false,
    direction = "down",
  }: {
    label: string;
    items: DropdownItem[];
    disabled?: boolean;
    primary?: boolean;
    ghost?: boolean;
    direction?: "down" | "up";
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
    class:btn-ghost={ghost}
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
    <div
      class="menu"
      class:opens-up={direction === "up"}
      role="menu"
      transition:softPop={{ duration: 150, y: direction === "up" ? 4 : -4, start: 0.98 }}
    >
      {#each items as item (item.label)}
        <button
          type="button"
          role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
          aria-checked={item.checked}
          class:danger={item.danger}
          onclick={() => pick(item)}
        >
          <span class="item-copy">
            <span class="item-label">{item.label}</span>
            {#if item.hint}
              <span class="item-hint">{item.hint}</span>
            {/if}
          </span>
          {#if item.checked}<Check class="item-check" size={15} strokeWidth={2.2} />{/if}
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
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-2);
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .menu.opens-up {
    top: auto;
    bottom: calc(100% + 4px);
  }

  .menu button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
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

  .item-copy {
    display: grid;
    gap: 1px;
  }

  :global(.item-check) {
    flex: none;
    color: var(--accent);
  }
</style>
