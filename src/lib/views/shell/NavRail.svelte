<script lang="ts">
  import type { LucideIcon } from "@lucide/svelte";
  import FileText from "@lucide/svelte/icons/file-text";
  import Files from "@lucide/svelte/icons/files";
  import Folders from "@lucide/svelte/icons/folders";
  import Images from "@lucide/svelte/icons/images";
  import Rows3 from "@lucide/svelte/icons/rows-3";
  import { app, type ViewMode } from "../../stores/app-state.svelte";
  import { clearSelection } from "../../stores/selection-store.svelte";

  const views: { mode: ViewMode; label: string; icon: LucideIcon }[] = [
    { mode: "gallery", label: "画廊", icon: Images },
    { mode: "table", label: "表格", icon: Rows3 },
    { mode: "group", label: "分组", icon: Folders },
    { mode: "duplicates", label: "重复", icon: Files },
    { mode: "promptDocs", label: "提示词", icon: FileText },
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
      <span class="nav-icon" aria-hidden="true">
        <view.icon size={18} strokeWidth={1.7} />
      </span>
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
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive);
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

  .nav-item.is-active::before {
    content: "";
    position: absolute;
    left: -6px;
    width: 2px;
    height: 18px;
    border-radius: var(--radius-full);
    background: var(--accent);
    transform-origin: center;
    animation: nav-indicator-in var(--motion-base) var(--ease-responsive);
  }

  .nav-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    transition: transform var(--motion-press) var(--ease-responsive);
  }

  .nav-item:active .nav-icon {
    transform: scale(0.94);
  }

  .nav-item::after {
    content: attr(data-tooltip);
    position: absolute;
    left: calc(100% + 8px);
    top: 50%;
    transform: translate(-4px, -50%);
    padding: 4px 10px;
    background: var(--text);
    color: var(--surface);
    font-size: var(--font-xs);
    border-radius: var(--radius-s);
    white-space: nowrap;
    pointer-events: none;
    opacity: 0;
    transition:
      opacity var(--motion-fast) var(--ease-responsive),
      transform var(--motion-fast) var(--ease-responsive);
    transition-delay: 0ms;
    z-index: var(--z-dropdown);
  }

  .nav-item:hover::after {
    opacity: 1;
    transform: translate(0, -50%);
    transition-delay: 320ms;
  }

  .nav-item:focus-visible::after {
    opacity: 1;
    transform: translate(0, -50%);
  }

  @keyframes nav-indicator-in {
    from {
      opacity: 0;
      transform: scaleY(0.45);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .nav-item.is-active::before {
      animation: none;
    }
  }

  .nav-bottom {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
</style>
