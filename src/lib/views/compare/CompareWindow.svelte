<script lang="ts">
  import Images from "@lucide/svelte/icons/images";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Notice from "../../ui/Notice.svelte";
  import WindowControls from "../../ui/WindowControls.svelte";
  import {
    compareStore,
    loadCompareSectionPage,
    openSideBySide,
    refreshCompare,
    setCompareSample,
  } from "../../stores/compare-store.svelte";
  import CompareSection from "./CompareSection.svelte";
  import ModelGroupSection from "./ModelGroupSection.svelte";
  import SampleCard from "./SampleCard.svelte";
  import SideBySide from "./SideBySide.svelte";

  // 样本 id 由后端写进新窗口 URL；复用窗口时经 compare://set-sample 推送。
  const initialRowId = (() => {
    const row = new URLSearchParams(window.location.search).get("row");
    const parsed = row == null ? Number.NaN : Number(row);
    return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
  })();

  onMount(() => {
    if (initialRowId != null) {
      void setCompareSample(initialRowId);
    }
    let disposed = false;
    let unlistenSample: (() => void) | null = null;
    let unlistenReset: (() => void) | null = null;
    // 复用窗口切换样本：窗口早已初始化，直接换数据。
    void listen<number>("compare://set-sample", event => {
      void setCompareSample(event.payload);
    }).then(fn => {
      if (disposed) fn();
      else unlistenSample = fn;
    });
    // 主窗口切换数据目录 / 迁移 / 重置后本窗口的数据源已不可信，直接关闭
    // 最安全（样本行可能已经不在新库里）。
    void listen("main://library-reset", () => {
      void closeOnLibraryReset();
    }).then(fn => {
      if (disposed) fn();
      else unlistenReset = fn;
    });
    return () => {
      disposed = true;
      unlistenSample?.();
      unlistenReset?.();
    };
  });

  async function closeOnLibraryReset(): Promise<void> {
    try {
      await getCurrentWindow().destroy();
    } catch {
      // 窗口可能已被用户关闭。
    }
  }

  const sample = $derived(compareStore.sample);

  function activateCompare(row: typeof compareStore.target): void {
    if (row) {
      openSideBySide(row);
    }
  }
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
        <div class="state-hint">样本可能已被删除；请在主窗口重新右键选择“对比”。</div>
      </div>
    {:else if !sample}
      <div class="state-page">
        <span class="state-icon" aria-hidden="true"><Images size={30} strokeWidth={1.4} /></span>
        <div class="state-title">还没有选择对比样本</div>
        <div class="state-hint">在主窗口右键任意图片，选择“对比”，即可在这里查看它与全库的关联。</div>
      </div>
    {:else if compareStore.view === "sideBySide" && compareStore.target}
      <SideBySide sample={sample.row} target={compareStore.target} />
    {:else}
      <div class="sections-scroll">
        <SampleCard
          row={sample.row}
          refreshing={compareStore.sampleLoading}
          onrefresh={() => void refreshCompare()}
        />
        <CompareSection
          title="相同画师串"
          description="完整画师串与样本一致"
          state={compareStore.artists}
          emptyText="这张图没有画师串。"
          sampleUnavailable={!sample.row.artists?.trim()}
          onLoadMore={() => void loadCompareSectionPage("artists", false)}
          onactivate={activateCompare}
        />
        <CompareSection
          title="相同 VIBE × 不同提示词"
          description="引用了同一组 VIBE，但正向提示词与样本不同"
          state={compareStore.vibeDiffStyle}
          emptyText={sample.vibeSignatureUnreadable
            ? "样本原图不可读，无法读取 VIBE 引用。"
            : "这张图没有引用 VIBE。"}
          sampleUnavailable={!sample.hasVibeSignature}
          onLoadMore={() => void loadCompareSectionPage("vibeDiffStyle", false)}
          onactivate={activateCompare}
        />
        <CompareSection
          title="相同提示词 × 不同 VIBE"
          description="正向提示词与样本相同（忽略官方质量词），但 VIBE 引用不同"
          state={compareStore.styleDiffVibe}
          emptyText="这张图没有可比较的提示词。"
          sampleUnavailable={!sample.hasStyleSignature}
          onLoadMore={() => void loadCompareSectionPage("styleDiffVibe", false)}
          onactivate={activateCompare}
        />
        <ModelGroupSection
          section={compareStore.models}
          sampleModel={sample.row.generationModel}
          sampleUnavailable={!sample.hasStyleSignature}
          onactivate={activateCompare}
        />
      </div>
    {/if}
  </div>
</div>
<Notice />

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
    color: var(--danger, #b3261e);
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
