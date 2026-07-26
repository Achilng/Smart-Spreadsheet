<script lang="ts">
  import { onMount } from "svelte";
  import type { Snippet } from "svelte";
  import {
    isTopModalLayer,
    popModalLayer,
    pushModalLayer,
  } from "../stores/modal-layer.svelte";
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
  let layerToken = -1;

  // 打开时登记模态层并记录来源焦点；关闭时注销并归还焦点。
  $effect(() => {
    if (!open) {
      return;
    }
    const opener =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    layerToken = pushModalLayer();
    return () => {
      popModalLayer(layerToken);
      layerToken = -1;
      opener?.focus();
    };
  });

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
    // 只有位于模态栈顶的实例响应 Esc，且不响应已被内层（如内联编辑）消费的按键。
    if (
      event.key === "Escape" &&
      open &&
      !event.defaultPrevented &&
      isTopModalLayer(layerToken)
    ) {
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
      transition:softPop={{ duration: 200, y: 6, start: 0.985 }}
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
    background: var(--overlay);
  }

  .modal-dialog {
    max-width: 90vw;
    max-height: 85vh;
    background: var(--surface);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow-3);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    outline: none;
  }
</style>
