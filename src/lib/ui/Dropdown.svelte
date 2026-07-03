<script module lang="ts">
  export interface DropdownItem {
    label: string;
    hint?: string;
    danger?: boolean;
    action: () => void;
  }
</script>

<script lang="ts">
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
    <span class="caret" aria-hidden="true">▾</span>
  </button>
  {#if open}
    <div class="menu" role="menu">
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
    font-size: 10px;
    margin-left: 2px;
  }

  .menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 80;
    min-width: 200px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
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
    border-radius: 4px;
    padding: 6px 10px;
    text-align: left;
    font-size: 13px;
    color: var(--text);
  }

  .menu button:hover {
    background: var(--surface-2);
  }

  .menu button.danger {
    color: var(--danger);
  }

  .item-hint {
    font-size: 11px;
    color: var(--text-3);
  }
</style>
