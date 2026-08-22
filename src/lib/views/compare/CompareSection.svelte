<script lang="ts">
  import type { CompareSectionKind, CompareSectionSummary, RowRecord } from "../../api";
  import { emitTo } from "@tauri-apps/api/event";
  import SquareArrowOutUpRight from "@lucide/svelte/icons/square-arrow-out-up-right";
  import { formatCount } from "../../stores/app-state.svelte";
  import {
    COMPARE_OPEN_ARTISTS_EVENT,
    type CompareArtistsPayload,
  } from "../../windows/compare";
  import { focusMainWindow } from "../../windows/toolbox";
  import { modelVersionBadge } from "../../utils/model-version";
  import SectionGrid from "./SectionGrid.svelte";

  let {
    kind,
    summary,
    sample,
    onactivate,
  }: {
    kind: CompareSectionKind;
    summary: CompareSectionSummary;
    sample: RowRecord;
    onactivate: (row: RowRecord) => void;
  } = $props();

  const groupedKind = $derived(
    kind === "artistsByModel" || kind === "samePromptModelDiff",
  );
  const isGrouped = $derived(groupedKind && summary.totalCount > 0);

  const definition = $derived.by(() => {
    const hasArtists = Boolean(sample.artists?.trim());
    const hasVibe = (sample.vibeReferenceCount ?? 0) > 0;
    switch (kind) {
      case "sameArtists":
        return {
          title: "相同画师串",
          hint: "全库中画师串与样本完全一致的图片",
          empty: hasArtists
            ? "库里没有其它图片使用相同画师串"
            : "样本没有画师串",
          jump: true,
        };
      case "artistsByModel":
        return {
          title: "画师串 × 模型",
          hint: "相同画师串的图片按作画模型分组",
          empty: hasArtists
            ? "库里没有其它图片使用相同画师串"
            : "样本没有画师串",
          jump: false,
        };
      case "sameVibePromptDiff":
        return {
          title: "相同 VIBE · 提示词不同",
          hint: "使用了与样本完全相同的 VIBE 组合，但提示词不同",
          empty: hasVibe
            ? "没有找到使用同一组 VIBE 且提示词不同的图片"
            : "样本没有 VIBE",
          jump: false,
        };
      case "samePromptVibeDiff":
        return {
          title: "相同提示词 · VIBE 不同",
          hint: "提示词与样本一致，但使用了不同的 VIBE 组合",
          empty: "没有找到提示词一致但 VIBE 组合不同的图片",
          jump: false,
        };
      case "samePromptModelDiff":
        return {
          title: "相同提示词 · 模型不同",
          hint: "提示词与样本完全一致、由其它模型生成的图片，按模型分组",
          empty: "没有找到提示词一致且模型不同的图片",
          jump: false,
        };
    }
  });

  function groupLabel(model: string): { label: string; badgeClass: string | null; title: string } {
    if (!model) {
      return { label: "未知模型", badgeClass: null, title: "库内没有记录作画模型" };
    }
    const badge = modelVersionBadge(model);
    return {
      label: badge?.label ?? model,
      badgeClass: badge?.className ?? null,
      title: model,
    };
  }

  async function jumpToGallery(): Promise<void> {
    const artists = sample.artists?.trim();
    if (!artists) return;
    try {
      await emitTo("main", COMPARE_OPEN_ARTISTS_EVENT, { artists } satisfies CompareArtistsPayload);
      await focusMainWindow();
    } catch {
      // 主窗口不可用时忽略；分区浏览本身不受影响
    }
  }
</script>

<section class="compare-section">
  <header class="section-head">
    <h3 class="section-title">{definition.title}</h3>
    <span class="section-count tabular">{formatCount(summary.totalCount)}</span>
    <span class="section-hint">{definition.hint}</span>
    {#if definition.jump && summary.totalCount > 0}
      <button type="button" class="jump-btn" onclick={() => void jumpToGallery()}>
        <SquareArrowOutUpRight size={13} strokeWidth={1.7} />
        去主画廊看全部
      </button>
    {/if}
  </header>

  {#if summary.totalCount === 0}
    <p class="section-empty-text">{definition.empty}</p>
  {:else if isGrouped}
    {#each summary.modelGroups as group (group.model)}
      <div class="model-group">
        <h4 class="model-group-head">
          {#if groupLabel(group.model).badgeClass}
            <span class="version-badge {groupLabel(group.model).badgeClass}">{groupLabel(group.model).label}</span>
          {:else}
            <span class="group-name" title={groupLabel(group.model).title}>{groupLabel(group.model).label}</span>
          {/if}
          <span class="group-count tabular">{formatCount(group.rowCount)}</span>
          {#if groupLabel(group.model).badgeClass}
            <span class="group-name-raw" title={group.model}>{group.model}</span>
          {/if}
        </h4>
        <SectionGrid {kind} model={group.model} totalCount={group.rowCount} {onactivate} />
      </div>
    {/each}
  {:else}
    <SectionGrid {kind} totalCount={summary.totalCount} {onactivate} />
  {/if}
</section>

<style>
  .compare-section {
    padding: 18px 22px 20px;
    border-bottom: 1px solid var(--border);
  }

  .section-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 12px;
  }

  .section-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 700;
  }

  .section-count {
    font-size: var(--font-sm);
    font-weight: 600;
    color: var(--accent);
  }

  .section-hint {
    font-size: var(--font-xs);
    color: var(--text-3);
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .jump-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-left: auto;
    flex: none;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: transparent;
    padding: 4px 12px;
    font-size: var(--font-xs);
    color: var(--text-2);
    cursor: pointer;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive);
  }

  .jump-btn:hover {
    background: var(--surface-2);
    color: var(--text);
    border-color: var(--border-strong);
  }

  .section-empty-text {
    margin: 0;
    padding: 14px 16px;
    border: 1px dashed var(--border);
    border-radius: var(--radius-m);
    font-size: var(--font-sm);
    color: var(--text-3);
    background: var(--surface-2);
  }

  .model-group {
    padding: 10px 0 2px;
  }

  .model-group + .model-group {
    border-top: 1px dashed var(--border);
    margin-top: 10px;
  }

  .model-group-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 10px;
  }

  .group-name {
    font-size: var(--font-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 360px;
  }

  .group-name-raw {
    font-size: var(--font-xs);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 280px;
  }

  .group-count {
    font-size: var(--font-xs);
    font-weight: 600;
    color: var(--text-3);
  }
</style>
