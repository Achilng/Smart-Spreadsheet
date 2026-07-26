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
    /** true 时 Esc 与点击遮罩不关闭（异步操作进行中，防止结果随组件卸载丢失） */
    busy?: boolean;
    /** 指向标题元素 id，透传为 aria-labelledby */
    labelledby?: string;
    children: Snippet;
  }

  let { open, onclose, width = "440px", busy = false, labelledby, children }: Props = $props();

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

  function requestClose(): void {
    if (!busy) onclose();
  }

  const FOCUSABLE =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  /** 焦点陷阱：Tab 在对话框内循环，不逃逸到被遮罩盖住的背景。 */
  function onDialogKeydown(event: KeyboardEvent): void {
    if (event.key !== "Tab" || !dialogEl) return;
    const nodes = [...dialogEl.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
      el => el.offsetParent !== null,
    );
    if (nodes.length === 0) {
      event.preventDefault();
      return;
    }
    const first = nodes[0];
    const last = nodes[nodes.length - 1];
    const active = document.activeElement;
    if (event.shiftKey) {
      if (active === first || active === dialogEl) {
        event.preventDefault();
        last.focus();
      }
    } else if (active === last) {
      event.preventDefault();
      first.focus();
    }
  }
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
      requestClose();
    }
  }}
/>

{#if open}
  <div
    class="modal-backdrop"
    role="presentation"
    transition:softFade={{ duration: 140 }}
    onclick={event => {
      if (event.target === event.currentTarget) requestClose();
    }}
  >
    <div
      class="modal-dialog"
      bind:this={dialogEl}
      role="dialog"
      aria-modal="true"
      aria-labelledby={labelledby}
      tabindex="-1"
      style:width={width}
      transition:softPop={{ duration: 200, y: 6, start: 0.985 }}
      onkeydown={onDialogKeydown}
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
