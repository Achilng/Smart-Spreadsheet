<script lang="ts">
  import CompareCard from "./CompareCard.svelte";
  import type { RowRecord } from "../../api";
  import type { CompareModelSection } from "../../api/compare";
  import { formatCount } from "../../stores/app-state.svelte";
  import {
    modelComparisonTier,
    modelVersionBadge,
  } from "../../utils/model-version";

  let {
    section,
    sampleModel,
    sampleUnavailable,
    onactivate,
  }: {
    section: CompareModelSection;
    sampleModel: string | null;
    sampleUnavailable: boolean;
    onactivate: (row: RowRecord) => void;
  } = $props();

  interface ModelGroup {
    /** 徽章 label；无法识别版本时为 null。 */
    badgeLabel: string | null;
    badgeClassName: string | null;
    /** 组名：优先完整模型名；NULL/空模型归入“未知模型”。 */
    title: string;
    /** 用于与样本比较“同档位”。 */
    tier: string;
    versionRank: number;
    rows: RowRecord[];
  }

  /**
   * 按模型档位分组：能识别版本的用徽章档位（同版本 Full/Curated 分开）；
   * 无法识别的退化为 generation_model 原始字符串比较；NULL/空串归入
   * “未知模型”组。与样本同档位的组剔除，组间按版本从新到旧。
   */
  const groups = $derived.by(() => {
    const byTier = new Map<string, ModelGroup>();
    for (const row of section.rows) {
      const badge = modelVersionBadge(row.generationModel);
      const rawModel = row.generationModel?.trim() || null;
      const tier = modelComparisonTier(row.generationModel);
      let group = byTier.get(tier);
      if (!group) {
        group = {
          badgeLabel: badge?.label ?? null,
          badgeClassName: badge?.className ?? null,
          title: rawModel ?? badge?.label ?? "未知模型",
          tier,
          versionRank: badge ? versionRankOf(badge.label) : -1,
          rows: [],
        };
        byTier.set(tier, group);
      }
      group.rows.push(row);
    }
    const sampleTier = modelComparisonTier(sampleModel);
    return [...byTier.values()]
      .filter(group => group.tier !== sampleTier)
      .sort((a, b) => b.versionRank - a.versionRank || a.title.localeCompare(b.title));
  });

  /** 标题只显示过滤掉样本同模型组之后，当前真正可见的图片数。 */
  const visibleCount = $derived(
    groups.reduce((total, group) => total + group.rows.length, 0),
  );

  function versionRankOf(label: string): number {
    const match = /^v(\d+(?:\.\d+)?)/.exec(label);
    return match ? Number(match[1]) : 0;
  }
</script>

<section class="compare-section">
  <header class="section-head">
    <h2 class="section-title">相同画风 × 不同模型</h2>
    {#if visibleCount > 0}
      <span class="section-count">{formatCount(visibleCount)} 张</span>
    {/if}
    <span class="section-desc">提示词相同、按作画模型分组；与样本同模型的组不显示</span>
  </header>

  {#if section.truncated}
    <div class="truncate-note">匹配的图片超过 500 张，仅显示最新的前 500 张。</div>
  {/if}

  {#if groups.length > 0}
    {#each groups as group (group.tier)}
      <div class="model-group">
        <header class="group-head">
          {#if group.badgeLabel}
            <span class="version-badge {group.badgeClassName ?? ''}">{group.badgeLabel}</span>
          {/if}
          <span class="group-title">{group.title}</span>
          <span class="group-count">{formatCount(group.rows.length)} 张</span>
        </header>
        <div class="section-grid">
          {#each group.rows as row (row.id)}
            <CompareCard {row} onactivate={() => onactivate(row)} />
          {/each}
        </div>
      </div>
    {/each}
  {:else if sampleUnavailable}
    <div class="section-state">这张图没有可比较的提示词。</div>
  {:else if section.totalCount === 0}
    <div class="section-state">没有找到符合条件的图片。</div>
  {:else}
    <div class="section-state">找到的相同画风图片都与样本使用同一模型。</div>
  {/if}
</section>

<style>
  .compare-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 22px 26px 6px;
    border-top: 1px solid var(--border);
  }

  .section-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }

  .section-title {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 700;
    color: var(--text);
  }

  .section-count {
    flex: none;
    font-size: var(--font-xs);
    font-weight: 600;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    padding: 2px 8px;
    border-radius: 999px;
  }

  .section-desc {
    font-size: var(--font-xs);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .truncate-note {
    font-size: var(--font-xs);
    color: var(--warning, #8a6100);
    background: color-mix(in srgb, var(--warning, #8a6100) 10%, transparent);
    border-radius: var(--radius-s);
    padding: 6px 10px;
  }

  .model-group {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .group-head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .group-title {
    font-size: var(--font-sm);
    font-weight: 700;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .group-count {
    flex: none;
    font-size: var(--font-xs);
    color: var(--text-3);
  }

  .section-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(128px, 1fr));
    gap: 14px;
  }

  .section-state {
    font-size: var(--font-sm);
    color: var(--text-3);
    background: var(--surface);
    border-radius: var(--radius-m);
    padding: 18px 16px;
    text-align: center;
  }
</style>
