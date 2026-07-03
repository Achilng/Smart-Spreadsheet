<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";

  const appWindow = getCurrentWindow();

  let maximized = $state(false);

  $effect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    const sync = (): void => {
      void appWindow.isMaximized().then(value => {
        if (!cancelled) {
          maximized = value;
        }
      });
    };
    sync();
    void appWindow.onResized(sync).then(fn => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  });
</script>

<div class="window-controls">
  <button type="button" title="最小化" aria-label="最小化" onclick={() => void appWindow.minimize()}>
    <svg viewBox="0 0 10 10" aria-hidden="true">
      <line x1="0.5" y1="5" x2="9.5" y2="5" />
    </svg>
  </button>
  <button
    type="button"
    title={maximized ? "还原" : "最大化"}
    aria-label={maximized ? "还原" : "最大化"}
    onclick={() => void appWindow.toggleMaximize()}
  >
    {#if maximized}
      <svg viewBox="0 0 10 10" aria-hidden="true">
        <rect x="0.5" y="2.5" width="7" height="7" fill="none" />
        <path d="M 2.5 2.5 V 0.5 H 9.5 V 7.5 H 7.5" fill="none" />
      </svg>
    {:else}
      <svg viewBox="0 0 10 10" aria-hidden="true">
        <rect x="0.5" y="0.5" width="9" height="9" fill="none" />
      </svg>
    {/if}
  </button>
  <button type="button" class="close" title="关闭" aria-label="关闭" onclick={() => void appWindow.close()}>
    <svg viewBox="0 0 10 10" aria-hidden="true">
      <line x1="0.5" y1="0.5" x2="9.5" y2="9.5" />
      <line x1="9.5" y1="0.5" x2="0.5" y2="9.5" />
    </svg>
  </button>
</div>

<style>
  .window-controls {
    display: flex;
    align-self: stretch;
    flex: none;
  }

  .window-controls button {
    width: 44px;
    border: none;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-2);
    padding: 0;
    border-radius: 0;
  }

  .window-controls button:hover {
    background: rgb(16 24 40 / 7%);
    color: var(--text);
  }

  .window-controls button.close:hover {
    background: #e81123;
    color: #ffffff;
  }

  .window-controls svg {
    width: 10px;
    height: 10px;
    stroke: currentColor;
    stroke-width: 1;
    shape-rendering: crispEdges;
  }
</style>
