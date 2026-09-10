<script lang="ts">
  import Images from "@lucide/svelte/icons/images";
  import Palette from "@lucide/svelte/icons/palette";
  import Layers from "@lucide/svelte/icons/layers";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import ScanLine from "@lucide/svelte/icons/scan-line";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Notice from "../../ui/Notice.svelte";
  import WindowControls from "../../ui/WindowControls.svelte";
  import { compareStore, loadCompareSectionPage, loadCompareModels, openSideBySide, refreshCompare, setCompareSample, selectCompareTab, type CompareTab } from "../../stores/compare-store.svelte";
  import CompareSection from "./CompareSection.svelte";
  import ModelGroupSection from "./ModelGroupSection.svelte";
  import SampleCard from "./SampleCard.svelte";
  import SideBySide from "./SideBySide.svelte";

  const initialRowId = Number(new URLSearchParams(window.location.search).get("row"));
  const tabs = [
    { key: "artists" as const, label: "相同画师串", hint: "查看相同画师组合的创作", icon: Palette },
    { key: "vibeDiffStyle" as const, label: "相同 VIBE", hint: "观察不同提示词的效果", icon: Layers },
    { key: "styleDiffVibe" as const, label: "相同提示词", hint: "观察不同 VIBE 的效果", icon: ScanLine },
    { key: "models" as const, label: "不同模型", hint: "比较同一画风的模型表现", icon: SlidersHorizontal },
  ];
  onMount(() => {
    let disposed = false;
    let unlistenSample: (() => void) | undefined;
    let unlistenReset: (() => void) | undefined;
    void listen<number>("compare://set-sample", event => void setCompareSample(event.payload)).then(fn => {
      if (disposed) fn(); else unlistenSample = fn;
    });
    void listen("main://library-reset", () => void getCurrentWindow().destroy().catch(() => {})).then(fn => {
      if (disposed) fn(); else unlistenReset = fn;
    });
    if (Number.isInteger(initialRowId) && initialRowId > 0) void setCompareSample(initialRowId);
    return () => { disposed = true; unlistenSample?.(); unlistenReset?.(); };
  });
  const sample = $derived(compareStore.sample);
  function chooseTab(tab: CompareTab) { void selectCompareTab(tab); }
</script>

<svelte:window oncontextmenu={event => {
  const target = event.target;
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || (target instanceof HTMLElement && target.isContentEditable)) return;
  event.preventDefault();
}} />

<div class="compare-window">
  <header class="titlebar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region><span class="brand-icon"><Images size={18} /></span><strong data-tauri-drag-region>图片对比</strong><span class="brand-divider"></span><small data-tauri-drag-region>发现创作之间的不同</small></div>
    <WindowControls />
  </header>
  <div class="compare-body">
    {#if compareStore.sampleLoading}
      <div class="state-page" role="status"><span class="loading-line"></span><h1>正在准备对比</h1><p>读取样本与关联图片…</p></div>
    {:else if compareStore.sampleError}
      <div class="state-page"><Images size={32} /><h1>样本加载失败</h1><p>{compareStore.sampleError}</p><button class="retry" onclick={() => void refreshCompare()}>重新加载</button></div>
    {:else if !sample}
      <div class="state-page"><Images size={36} strokeWidth={1.3} /><h1>从一张图片开始</h1><p>在主窗口右键图片，选择“对比”，探索画师、提示词与模型之间的关联。</p></div>
    {:else if compareStore.view === "sideBySide" && compareStore.target}
      {#key compareStore.target.id}<SideBySide sample={sample.row} target={compareStore.target} />{/key}
    {:else}
      <div class="workspace">
        <aside class="sidebar" aria-label="对比样本与关系">
          <div class="sidebar-label"><span class="sample-dot"></span>参考样本<span class="sample-id">#{sample.row.id}</span></div>
          <SampleCard row={sample.row} refreshing={compareStore.sampleLoading} onrefresh={() => void refreshCompare()} />
          <div class="nav-label">选择对比关系</div>
          <nav class="relation-nav" aria-label="对比关系">
            {#each tabs as tab}
              <button class:active={compareStore.activeTab === tab.key} aria-pressed={compareStore.activeTab === tab.key} onclick={() => chooseTab(tab.key)}>
                <span class="nav-icon"><tab.icon size={18} strokeWidth={1.7} /></span>
                <span class="nav-copy"><strong>{tab.label}</strong><small>{tab.hint}</small></span>
                <span class="nav-arrow" aria-hidden="true">›</span>
              </button>
            {/each}
          </nav>
          <p class="sidebar-tip">选择右侧图片，进入双图对照。</p>
        </aside>
        <main class="results">
          {#key compareStore.activeTab}
            {#if compareStore.activeTab === "models"}
              <ModelGroupSection section={compareStore.models} sampleModel={sample.row.generationModel} sampleUnavailable={!sample.hasStyleSignature} loading={compareStore.modelsLoading} error={compareStore.modelsError} onretry={() => void loadCompareModels()} onactivate={openSideBySide} />
            {:else}
              {@const key = compareStore.activeTab}
              <CompareSection
                title={key === "artists" ? "相同画师串" : key === "vibeDiffStyle" ? "相同 VIBE · 不同提示词" : "相同提示词 · 不同 VIBE"}
                description={key === "artists" ? "完整画师串一致，发现同一组合的不同创作。" : key === "vibeDiffStyle" ? "保持 VIBE 引用不变，观察正向提示词带来的变化。" : "正向提示词一致（忽略官方质量词），观察 VIBE 引用带来的变化。"}
                state={compareStore[key]}
                emptyText={key === "artists" ? "样本还没有画师串，试试其他对比关系。" : key === "vibeDiffStyle" ? sample.vibeSignatureUnreadable ? "样本原图不可读，无法读取 VIBE 引用。" : "样本没有引用 VIBE，试试其他对比关系。" : "样本没有可比较的提示词，试试其他对比关系。"}
                sampleUnavailable={key === "artists" ? !sample.row.artists?.trim() : key === "vibeDiffStyle" ? !sample.hasVibeSignature : !sample.hasStyleSignature}
                onLoadMore={() => void loadCompareSectionPage(key, false)}
                onPrevious={() => void loadCompareSectionPage(key, false, -1)}
                onretry={() => void loadCompareSectionPage(key, true)}
                onactivate={openSideBySide}
              />
            {/if}
          {/key}
        </main>
      </div>
    {/if}
  </div>
</div>
<Notice />

<style>
  .compare-window { height: 100vh; display: flex; flex-direction: column; background: var(--bg); color: var(--text); }
  .titlebar { height: 52px; flex: none; display: flex; justify-content: space-between; border-bottom: 1px solid var(--border); background: var(--surface); user-select: none; }
  .brand { display: flex; align-items: center; gap: 12px; padding: 0 20px; }
  .brand strong { font-size: 14px; letter-spacing: .03em; }
  .brand-icon { color: var(--accent); display: flex; }
  .brand-divider { width: 1px; height: 14px; background: var(--border); }
  .brand small { color: var(--text-3); font-size: 11px; }
  .compare-body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .workspace { display: grid; grid-template-columns: 264px minmax(0, 1fr); flex: 1; min-height: 0; }
  .sidebar { background: var(--surface); border-right: 1px solid var(--border); overflow: auto; padding: 20px 18px; }
  .sidebar-label { display: flex; gap: 7px; align-items: center; font-size: 11px; font-weight: 650; color: var(--text-2); margin-bottom: 12px; }
  .sample-dot { width: 6px; height: 6px; background: var(--accent); border-radius: 50%; }
  .sample-id { margin-left: auto; color: var(--text-3); font-weight: 400; font-variant-numeric: tabular-nums; }
  .nav-label { margin: 24px 8px 10px; font-size: 10px; letter-spacing: .08em; color: var(--text-3); }
  .relation-nav { display: flex; flex-direction: column; gap: 5px; }
  .relation-nav button { display: flex; align-items: center; gap: 11px; width: 100%; text-align: left; padding: 12px 10px; border: 1px solid transparent; border-radius: 10px; background: transparent; color: var(--text-2); }
  .relation-nav button:hover { background: var(--surface-2); }
  .relation-nav button.active { background: var(--accent-soft); border-color: var(--accent-soft-border); color: var(--accent); }
  .nav-icon { display: flex; }
  .nav-copy { display: flex; flex-direction: column; gap: 3px; }
  .nav-copy strong { font-size: 12px; font-weight: 650; }
  .nav-copy small { font-size: 10px; color: var(--text-3); }
  .active .nav-copy small { color: color-mix(in srgb, var(--accent) 75%, var(--text-2)); }
  .nav-arrow { margin-left: auto; font-size: 20px; opacity: .6; }
  .sidebar-tip { margin: 20px 8px 0; font-size: 11px; color: var(--text-3); line-height: 1.7; }
  .results { min-width: 0; overflow: auto; padding: 28px; }
  .state-page { flex: 1; display: flex; align-items: center; justify-content: center; flex-direction: column; padding: 40px; gap: 14px; text-align: center; color: var(--text-3); }
  .state-page h1 { color: var(--text); font-size: 20px; font-weight: 600; }
  .state-page p { max-width: 380px; font-size: 13px; overflow-wrap: anywhere; }
  .retry { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 8px 16px; }
  .loading-line { width: 44px; height: 4px; border-radius: 4px; background: var(--accent); animation: pulse 1s ease-in-out infinite alternate; }
  @keyframes pulse { to { opacity: .3; transform: scaleX(.6); } }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  @media (max-width: 860px) { .workspace { grid-template-columns: 220px minmax(0, 1fr); } .sidebar { padding: 16px 12px; } .results { padding: 20px; } .nav-copy small { display: none; } }
  @media (prefers-reduced-motion: reduce) { .loading-line { animation: none; } }
</style>
