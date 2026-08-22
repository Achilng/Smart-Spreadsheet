<script lang="ts">
  import type { RowRecord } from "../../api";
  import { emitTo } from "@tauri-apps/api/event";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Check from "@lucide/svelte/icons/check";
  import Copy from "@lucide/svelte/icons/copy";
  import SquareArrowOutUpRight from "@lucide/svelte/icons/square-arrow-out-up-right";
  import { focusMainWindow, type ToolboxRowRequest } from "../../windows/toolbox";
  import { diffPromptField } from "../../utils/prompt-diff";
  import { modelVersionBadge } from "../../utils/model-version";
  import { rowFileName } from "../../utils/row-display";
  import CompareLargeImage from "./CompareLargeImage.svelte";

  let {
    sample,
    target,
    onback,
  }: {
    sample: RowRecord;
    target: RowRecord;
    onback: () => void;
  } = $props();

  const sampleHasImage = $derived(
    Boolean(sample.imagePath?.trim() || sample.storedImagePath?.trim()),
  );
  const targetHasImage = $derived(
    Boolean(target.imagePath?.trim() || target.storedImagePath?.trim()),
  );
  const sampleBadge = $derived(modelVersionBadge(sample.generationModel));
  const targetBadge = $derived(modelVersionBadge(target.generationModel));

  const promptViews = $derived.by(() => {
    const fields: Array<{ key: string; label: string; left: string | null; right: string | null }> = [
      { key: "positive", label: "正向提示词", left: sample.positivePrompt, right: target.positivePrompt },
      { key: "character", label: "角色提示词", left: sample.characterPrompt, right: target.characterPrompt },
      { key: "negative", label: "负向提示词", left: sample.negativePrompt, right: target.negativePrompt },
    ];
    return fields
      .map(field => {
        const diff = diffPromptField(field.left, field.right);
        const identical =
          diff.onlyLeft.length === 0 && diff.onlyRight.length === 0;
        return { key: field.key, label: field.label, identical, diff };
      })
      .filter(view => view.diff.shared.length > 0 || !view.identical);
  });

  let showShared = $state<Set<string>>(new Set());

  function toggleShared(key: string): void {
    const next = new Set(showShared);
    if (next.has(key)) {
      next.delete(key);
    } else {
      next.add(key);
    }
    showShared = next;
  }

  interface ParamRow {
    label: string;
    left: string;
    right: string;
    leftBadgeClass: string | null;
    rightBadgeClass: string | null;
  }

  const paramRows = $derived.by(() => {
    const badgeOr = (row: RowRecord) =>
      modelVersionBadge(row.generationModel)?.className ?? null;
    const rows: ParamRow[] = [
      {
        label: "模型",
        left: sample.generationModel || "未知",
        right: target.generationModel || "未知",
        leftBadgeClass: badgeOr(sample),
        rightBadgeClass: badgeOr(target),
      },
      {
        label: "采样器",
        left: sample.generationSampler || "—",
        right: target.generationSampler || "—",
        leftBadgeClass: null,
        rightBadgeClass: null,
      },
      {
        label: "步数",
        left: sample.generationSteps != null ? String(sample.generationSteps) : "—",
        right: target.generationSteps != null ? String(target.generationSteps) : "—",
        leftBadgeClass: null,
        rightBadgeClass: null,
      },
      {
        label: "Guidance",
        left: sample.generationScale ?? "—",
        right: target.generationScale ?? "—",
        leftBadgeClass: null,
        rightBadgeClass: null,
      },
      {
        label: "种子",
        left: sample.generationSeed ?? "—",
        right: target.generationSeed ?? "—",
        leftBadgeClass: null,
        rightBadgeClass: null,
      },
      {
        label: "尺寸",
        left:
          sample.imageWidth != null && sample.imageHeight != null
            ? `${sample.imageWidth} × ${sample.imageHeight}`
            : "—",
        right:
          target.imageWidth != null && target.imageHeight != null
            ? `${target.imageWidth} × ${target.imageHeight}`
            : "—",
        leftBadgeClass: null,
        rightBadgeClass: null,
      },
    ];
    return rows;
  });

  function paramDiffers(row: ParamRow): boolean {
    return row.left.trim() !== row.right.trim();
  }

  let copiedKey = $state<string | null>(null);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function copyPrompt(): Promise<void> {
    const value = target.positivePrompt;
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      copiedKey = "prompt";
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copiedKey = null;
      }, 1500);
    } catch {
      copiedKey = null;
    }
  }

  async function locateInGallery(): Promise<void> {
    try {
      await emitTo("main", "toolbox://open-row", { rowId: target.id } satisfies ToolboxRowRequest);
      await focusMainWindow();
    } catch {
      // 主窗口不可用时忽略
    }
  }
</script>

<div class="side-by-side">
  <header class="sbs-head">
    <button type="button" class="btn btn-ghost back-btn" onclick={onback}>
      <ArrowLeft size={14} strokeWidth={1.7} />
      返回分区列表
    </button>
    <h2 class="sbs-title">并排对比</h2>
    <div class="sbs-actions">
      {#if target.positivePrompt?.trim()}
        <button type="button" class="btn btn-ghost" onclick={() => void copyPrompt()}>
          {#if copiedKey === "prompt"}<Check size={14} strokeWidth={1.7} />已复制{:else}<Copy size={14} strokeWidth={1.7} />复制该图 Prompt{/if}
        </button>
      {/if}
      <button type="button" class="btn btn-ghost" onclick={() => void locateInGallery()}>
        <SquareArrowOutUpRight size={14} strokeWidth={1.7} />
        在主画廊中查看
      </button>
    </div>
  </header>

  <div class="sbs-body">
    <div class="images">
      <figure class="image-col">
        <figcaption class="image-caption">
          <span class="caption-tag sample-tag">样本</span>
          <span class="caption-name" title={rowFileName(sample) ?? ""}>{rowFileName(sample) ?? `#${sample.sourceOrdinal}`}</span>
        </figcaption>
        <div class="image-frame">
          <CompareLargeImage rowId={sample.id} hasImage={sampleHasImage} alt={rowFileName(sample) ?? "样本"} />
        </div>
        {#if sampleBadge}
          <span class="version-badge {sampleBadge.className} caption-badge">{sampleBadge.label}</span>
        {/if}
      </figure>
      <figure class="image-col">
        <figcaption class="image-caption">
          <span class="caption-tag target-tag">对比</span>
          <span class="caption-name" title={rowFileName(target) ?? ""}>{rowFileName(target) ?? `#${target.sourceOrdinal}`}</span>
        </figcaption>
        <div class="image-frame">
          <CompareLargeImage rowId={target.id} hasImage={targetHasImage} alt={rowFileName(target) ?? "对比图"} />
        </div>
        {#if targetBadge}
          <span class="version-badge {targetBadge.className} caption-badge">{targetBadge.label}</span>
        {/if}
      </figure>
    </div>

    {#if promptViews.length > 0}
      <section class="diff-block">
        <h3 class="block-title">提示词差异</h3>
        {#each promptViews as view (view.key)}
          <div class="prompt-diff">
            <h4 class="diff-field-title">{view.label}</h4>
            {#if view.identical}
              <p class="identical-note">完全一致（{view.diff.shared.length} 项）</p>
            {:else}
              <div class="chip-rows">
                {#if view.diff.onlyLeft.length > 0}
                  <div class="chip-row">
                    <span class="chip-row-label only-sample">仅样本有</span>
                    <div class="chips">
                      {#each view.diff.onlyLeft as token, index (index)}
                        <span class="chip chip-left" title={token}>{token}</span>
                      {/each}
                    </div>
                  </div>
                {/if}
                {#if view.diff.onlyRight.length > 0}
                  <div class="chip-row">
                    <span class="chip-row-label only-target">仅该图有</span>
                    <div class="chips">
                      {#each view.diff.onlyRight as token, index (index)}
                        <span class="chip chip-right" title={token}>{token}</span>
                      {/each}
                    </div>
                  </div>
                {/if}
                {#if view.diff.shared.length > 0}
                  <div class="chip-row">
                    <button type="button" class="chip-row-label shared-toggle" onclick={() => toggleShared(view.key)}>
                      相同 {view.diff.shared.length} 项{showShared.has(view.key) ? " ▾" : " ▸"}
                    </button>
                    {#if showShared.has(view.key)}
                      <div class="chips">
                        {#each view.diff.shared as token, index (index)}
                          <span class="chip chip-shared" title={token}>{token}</span>
                        {/each}
                      </div>
                    {/if}
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </section>
    {/if}

    <section class="diff-block">
      <h3 class="block-title">生成参数</h3>
      <table class="param-table">
        <thead>
          <tr>
            <th></th>
            <th>样本</th>
            <th>该图</th>
          </tr>
        </thead>
        <tbody>
          {#each paramRows as row (row.label)}
            <tr class:is-diff={paramDiffers(row)}>
              <th scope="row">{row.label}</th>
              <td class="tabular">
                {#if row.leftBadgeClass}
                  <span class="version-badge {row.leftBadgeClass}">{modelVersionBadge(sample.generationModel)?.label}</span>
                {/if}
                <span title={row.left}>{row.left}</span>
              </td>
              <td class="tabular">
                {#if row.rightBadgeClass}
                  <span class="version-badge {row.rightBadgeClass}">{modelVersionBadge(target.generationModel)?.label}</span>
                {/if}
                <span title={row.right}>{row.right}</span>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  </div>
</div>

<style>
  .side-by-side {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .sbs-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 22px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  .back-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .sbs-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 700;
  }

  .sbs-actions {
    margin-left: auto;
    display: flex;
    gap: 8px;
  }

  .sbs-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 18px 22px 28px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }

  .images {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
  }

  .image-col {
    position: relative;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .image-caption {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .caption-tag {
    flex: none;
    font-size: var(--font-xs);
    font-weight: 700;
    border-radius: 999px;
    padding: 2px 10px;
  }

  .sample-tag {
    background: var(--primary);
    color: #ffffff;
  }

  .target-tag {
    background: var(--surface-3);
    color: var(--text);
  }

  .caption-name {
    font-size: var(--font-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .image-frame {
    height: min(46vh, 420px);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .caption-badge {
    position: absolute;
    bottom: 8px;
    left: 0;
  }

  .diff-block {
    border-top: 1px solid var(--border);
    padding-top: 16px;
  }

  .block-title {
    margin: 0 0 12px;
    font-size: 0.95rem;
    font-weight: 700;
  }

  .prompt-diff {
    padding: 10px 0;
  }

  .prompt-diff + .prompt-diff {
    border-top: 1px dashed var(--border);
  }

  .diff-field-title {
    margin: 0 0 8px;
    font-size: var(--font-sm);
    font-weight: 600;
    color: var(--text-2);
  }

  .identical-note {
    margin: 0;
    font-size: var(--font-sm);
    color: var(--text-3);
  }

  .chip-rows {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .chip-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  .chip-row-label {
    flex: none;
    width: 74px;
    font-size: var(--font-xs);
    font-weight: 600;
    padding-top: 3px;
    text-align: left;
  }

  .chip-row-label.only-sample {
    color: var(--warning);
  }

  .chip-row-label.only-target {
    color: var(--success);
  }

  .shared-toggle {
    border: none;
    background: transparent;
    padding: 3px 0 0;
    color: var(--text-3);
    cursor: pointer;
    font: inherit;
    font-size: var(--font-xs);
    font-weight: 600;
    width: 74px;
    text-align: left;
  }

  .shared-toggle:hover {
    color: var(--text);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    min-width: 0;
  }

  .chip {
    font-size: var(--font-xs);
    padding: 2px 9px;
    border-radius: 999px;
    max-width: 260px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chip-left {
    background: var(--warning-soft);
    color: var(--warning);
  }

  .chip-right {
    background: var(--success-soft);
    color: var(--success);
  }

  .chip-shared {
    background: var(--surface-2);
    color: var(--text-3);
  }

  .param-table {
    border-collapse: collapse;
    width: 100%;
    max-width: 720px;
  }

  .param-table th,
  .param-table td {
    text-align: left;
    padding: 7px 14px 7px 0;
    font-size: var(--font-sm);
    font-weight: 400;
    vertical-align: middle;
  }

  .param-table thead th {
    color: var(--text-3);
    font-size: var(--font-xs);
    font-weight: 600;
  }

  .param-table tbody th {
    color: var(--text-3);
    font-weight: 500;
    white-space: nowrap;
  }

  .param-table tbody td {
    max-width: 320px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .param-table tbody tr {
    border-top: 1px solid var(--border);
  }

  .param-table tbody tr.is-diff td {
    font-weight: 600;
    color: var(--text);
  }

  .param-table tbody tr.is-diff td span {
    color: var(--accent);
  }
</style>
