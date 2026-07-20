<script lang="ts">
  import Copy from "@lucide/svelte/icons/copy";
  import Minus from "@lucide/svelte/icons/minus";
  import Square from "@lucide/svelte/icons/square";
  import X from "@lucide/svelte/icons/x";
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
    <span class="window-icon" aria-hidden="true">
      <Minus size={16} strokeWidth={1.5} />
    </span>
  </button>
  <button
    type="button"
    title={maximized ? "还原" : "最大化"}
    aria-label={maximized ? "还原" : "最大化"}
    onclick={() => void appWindow.toggleMaximize()}
  >
    <span class="window-icon" aria-hidden="true">
      {#if maximized}
        <Copy size={16} strokeWidth={1.5} />
      {:else}
        <Square size={16} strokeWidth={1.5} />
      {/if}
    </span>
  </button>
  <button type="button" class="close" title="关闭" aria-label="关闭" onclick={() => void appWindow.close()}>
    <span class="window-icon" aria-hidden="true">
      <X size={16} strokeWidth={1.5} />
    </span>
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
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive);
  }

  .window-controls button:hover {
    background: rgb(16 24 40 / 7%);
    color: var(--text);
  }

  .window-controls button.close:hover {
    background: #e81123;
    color: #ffffff;
  }

  .window-icon {
    width: 16px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: transform var(--motion-press) var(--ease-responsive);
  }

  .window-controls button:active .window-icon {
    transform: translateY(1px);
  }
</style>
