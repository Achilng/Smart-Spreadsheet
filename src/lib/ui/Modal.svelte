<script lang="ts">
  import { onMount } from "svelte";
  import type { Snippet } from "svelte";
  import { softFade, softPop } from "./motion";

  interface Props {
    open: boolean;
    onclose: () => void;
    /** 对话框宽度，默认 440px */
    width?: string;
    children: Snippet;
  }

  let { open, onclose, width = "440px", children }: Props = $props();

  let dialogEl = $state<HTMLDivElement | null>(null);

  // 挂载时把焦点移进 dialog
  onMount(() => {
    if (open && dialogEl) {
      dialogEl.focus();
    }
  });

  // open 变化时也聚焦
  $effect(() => {
    if (open && dialogEl) {
      dialogEl.focus();
    }
  });
</script>

<svelte:window
  onkeydown={event => {
    if (event.key === "Escape" && open) {
      event.preventDefault();
      onclose();
    }
  }}
/>

{#if open}
  <div
    class="modal-backdrop"
    role="presentation"
    transition:softFade={{ duration: 140 }}
    onclick={event => {
      if (event.target === event.currentTarget) onclose();
    }}
  >
    <div
      class="modal-dialog"
      bind:this={dialogEl}
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      style:width={width}
      transition:softPop={{ duration: 180, y: 6, start: 0.985 }}
    >
      {@render children()}
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal);
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--overlay-modal);
    backdrop-filter: blur(4px);
  }

  .modal-dialog {
    max-width: 90vw;
    max-height: 85vh;
    background: var(--glass-solid);
    backdrop-filter: blur(var(--glass-blur)) saturate(1.4);
    border: 1px solid rgb(255 255 255 / 55%);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-2);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    outline: none;
  }

  :global(html:not([data-glass="on"])) .modal-dialog {
    background: var(--surface-opaque);
    border-color: var(--border);
    backdrop-filter: none;
  }
</style>
