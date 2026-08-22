<script lang="ts">
  import type { RowRecord } from "../../api";
  import Images from "@lucide/svelte/icons/images";
  import { onMount } from "svelte";
  import { emitTo, listen } from "@tauri-apps/api/event";
  import WindowControls from "../../ui/WindowControls.svelte";
  import { compareStore } from "../../stores/compare-store.svelte";
  import {
    COMPARE_REQUEST_EVENT,
    COMPARE_SAMPLE_EVENT,
    type CompareSamplePayload,
  } from "../../windows/compare";
  import CompareSection from "./CompareSection.svelte";
  import SampleCard from "./SampleCard.svelte";
  import SideBySide from "./SideBySide.svelte";

  const sample = $derived(compareStore.sample);
  const compareTarget = $derived(compareStore.compareTarget);

  function activateCompare(row: RowRecord): void {
    compareStore.compareTarget = row;
  }

  onMount(() => {
    let disposed = false;
    let unlistenSample: (() => void) | null = null;
    void listen<CompareSamplePayload>(COMPARE_SAMPLE_EVENT, event => {
      void compareStore.setSample(event.payload.rowId);
    }).then(unlisten => {
      if (disposed) unlisten();
      else unlistenSample = unlisten;
    });
    // 新建窗口时监听器可能晚于主窗口的首次推送；主动向主窗口拉取当前样本
    void emitTo("main", COMPARE_REQUEST_EVENT);
    return () => {
      disposed = true;
      unlistenSample?.();
    };
  });
</script>

<svelte:window
  oncontextmenu={event => {
    const target = event.target;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLElement && target.isContentEditable)
    ) {
      return;
    }
    event.preventDefault();
  }}
/>

<div class="compare-window">
  <header class="titlebar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <span data-tauri-drag-region>图片对比</span>
      <small data-tauri-drag-region>智能表格</small>
    </div>
    <WindowControls />
  </header>

  <div class="compare-body">
    {#if compareStore.sampleLoading}
      <div class="state-page">
        <div class="state-text">正在加载样本…</div>
      </div>
    {:else if compareStore.sampleError}
      <div class="state-page">
        <div class="state-title">样本加载失败</div>
        <div class="state-text">{compareStore.sampleError}</div>
        <div class="state-hint">图片可能已被删除；请在主窗口重新右键选择“对比”。</div>
      </div>
    {:else if !sample}
      <div class="state-page">
        <span class="state-icon" aria-hidden="true"><Images size={30} strokeWidth={1.4} /></span>
        <div class="state-title">还没有选择对比样本</div>
        <div class="state-hint">在主窗口右键任意图片，选择“对比”，即可在这里查看它与全库的关联。</div>
      </div>
    {:else if compareTarget}
      <SideBySide
        sample={sample.row}
        target={compareTarget}
        onback={() => {
          compareStore.compareTarget = null;
        }}
      />
    {:else}
      <div class="sections-scroll">
        <SampleCard row={sample.row} />
        {#each sample.sections as summary (summary.kind)}
          <CompareSection
            kind={summary.kind}
            {summary}
            sample={sample.row}
            onactivate={activateCompare}
          />
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .compare-window {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg);
    color: var(--text);
  }

  .titlebar {
    display: flex;
    align-items: stretch;
    justify-content: space-between;
    height: 52px;
    flex: none;
    user-select: none;
  }

  .brand {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 0 18px;
  }

  .brand span {
    font-size: var(--font-md);
    font-weight: 700;
  }

  .brand small {
    color: var(--text-3);
  }

  .compare-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .sections-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .state-page {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 40px;
    text-align: center;
  }

  .state-icon {
    color: var(--text-4);
    display: flex;
  }

  .state-title {
    font-size: 1.05rem;
    font-weight: 700;
  }

  .state-text {
    font-size: var(--font-md);
    color: var(--danger);
    max-width: 480px;
    word-break: break-all;
  }

  .state-hint {
    font-size: var(--font-sm);
    color: var(--text-3);
    max-width: 420px;
    line-height: 1.7;
  }
</style>
