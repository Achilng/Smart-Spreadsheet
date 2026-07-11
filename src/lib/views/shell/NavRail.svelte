<script lang="ts">
  import { app, type ViewMode } from "../../stores/app-state.svelte";
  import { clearSelection } from "../../stores/selection-store.svelte";

  const views: { mode: ViewMode; label: string }[] = [
    { mode: "gallery", label: "画廊" },
    { mode: "table", label: "表格" },
    { mode: "group", label: "分组" },
    { mode: "duplicates", label: "重复" },
    { mode: "promptDocs", label: "提示词" },
  ];

  function switchView(mode: ViewMode): void {
    const wasGroup = app.viewMode === "group";
    const isGroup = mode === "group";
    app.viewMode = mode;
    if (wasGroup !== isGroup) {
      clearSelection();
    }
  }
</script>

<nav class="nav-rail">
  <div class="drag-spacer" data-tauri-drag-region></div>

  {#each views as view (view.mode)}
    <button
      type="button"
      class="nav-item"
      class:is-active={app.viewMode === view.mode}
      aria-pressed={app.viewMode === view.mode}
      data-tooltip={view.label}
      onclick={() => switchView(view.mode)}
    >
      {#if view.mode === "gallery"}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="3" y="3" width="8" height="8" rx="1" />
          <rect x="13" y="3" width="8" height="8" rx="1" />
          <rect x="3" y="13" width="8" height="8" rx="1" />
          <rect x="13" y="13" width="8" height="8" rx="1" />
        </svg>
      {:else if view.mode === "table"}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <line x1="4" y1="6" x2="20" y2="6" />
          <line x1="4" y1="12" x2="20" y2="12" />
          <line x1="4" y1="18" x2="20" y2="18" />
        </svg>
      {:else if view.mode === "group"}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <circle cx="9" cy="9" r="6" />
          <circle cx="15" cy="15" r="6" />
        </svg>
      {:else if view.mode === "duplicates"}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <rect x="7" y="7" width="12" height="12" rx="1.5" />
          <path d="M5 15V6a1.5 1.5 0 0 1 1.5-1.5H15" />
        </svg>
      {:else}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M4 20l1-4 11-11 3 3-11 11-4 1Z" />
          <line x1="14" y1="6" x2="17" y2="9" />
        </svg>
      {/if}
    </button>
  {/each}

  <div class="nav-bottom"></div>
</nav>

<style>
  .nav-rail {
    width: 52px;
    flex: none;
    background: var(--surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 8px 0;
    gap: 2px;
  }

  .drag-spacer {
    height: 40px;
    width: 100%;
    flex: none;
  }

  .nav-item {
    width: 40px;
    height: 40px;
    border: none;
    background: transparent;
    border-radius: var(--radius-s);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
    position: relative;
  }

  .nav-item:hover {
    background: var(--surface-2);
    color: var(--text-2);
  }

  .nav-item.is-active {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .nav-item svg {
    width: 20px;
    height: 20px;
  }

  .nav-item::after {
    content: attr(data-tooltip);
    position: absolute;
    left: calc(100% + 8px);
    top: 50%;
    transform: translateY(-50%);
    padding: 4px 10px;
    background: var(--text);
    color: var(--surface);
    font-size: var(--font-xs);
    border-radius: var(--radius-s);
    white-space: nowrap;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.15s ease;
    z-index: var(--z-dropdown);
  }

  .nav-item:hover::after {
    opacity: 1;
  }

  .nav-bottom {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
</style>
