<script lang="ts">
  import { tagStore } from "../stores/tag-store.svelte";

  let { tags }: { tags: string[] } = $props();

  /**
   * 定性分类色板：各组使用接近的明度与彩度，避免随机 HSL 产生脏色或荧光色。
   * 顺序依次为靛蓝、薰衣草、灰粉、陶土、琥珀、鼠尾草、青绿、雾蓝、
   * 灰紫、橄榄、珊瑚和海蓝。
   */
  const TAG_PALETTE = [
    { background: "#eef1fa", border: "#d8dff2", text: "#46557f", secondary: "#7180a5" },
    { background: "#f3eff9", border: "#e0d7ef", text: "#625080", secondary: "#8879a4" },
    { background: "#f9eff2", border: "#eed6de", text: "#7f4c60", secondary: "#a47787" },
    { background: "#f8f0eb", border: "#ead7cc", text: "#7c5141", secondary: "#a07968" },
    { background: "#f7f3e8", border: "#e8ddbe", text: "#725b2f", secondary: "#998252" },
    { background: "#eef4ed", border: "#d6e4d3", text: "#4d684f", secondary: "#748c75" },
    { background: "#ebf4f2", border: "#cee4df", text: "#426962", secondary: "#6d8d87" },
    { background: "#edf3f7", border: "#d2e1ea", text: "#43687d", secondary: "#6e8da0" },
    { background: "#f5eff5", border: "#e5d6e6", text: "#6d4f70", secondary: "#927696" },
    { background: "#f2f3ea", border: "#dee1c6", text: "#5f6639", secondary: "#82895e" },
    { background: "#faefed", border: "#efd6d1", text: "#805249", secondary: "#a4776e" },
    { background: "#ebf4f5", border: "#cfe4e8", text: "#3f6870", secondary: "#6c8d93" },
  ] as const;

  const visibleTags = $derived(tags.filter(tag => tag.trim().length > 0));
  const firstTag = $derived(visibleTags[0] ?? null);
  const remainingCount = $derived(Math.max(0, visibleTags.length - 1));
  const tagSummary = $derived(visibleTags.join("\n"));
  const tagTone = $derived(
    TAG_PALETTE[firstTag ? paletteIndexForTag(firstTag) : 0],
  );

  /**
   * Tag 库由后端按名称稳定排序：依序分配可避免常用 Tag 之间发生哈希撞色。
   * 尚未载入 Tag 库时保留确定性哈希作为短暂兜底。
   */
  function paletteIndexForTag(tag: string): number {
    const normalized = tag.trim().toLocaleLowerCase();
    const libraryIndex = tagStore.list.findIndex(
      entry => entry.name.trim().toLocaleLowerCase() === normalized,
    );
    if (libraryIndex >= 0) {
      return libraryIndex % TAG_PALETTE.length;
    }

    let hash = 2166136261;
    for (const character of normalized) {
      hash ^= character.codePointAt(0) ?? 0;
      hash = Math.imul(hash, 16777619);
    }
    return (hash >>> 0) % TAG_PALETTE.length;
  }
</script>

{#if firstTag}
  <span
    class="card-tag-summary"
    title={tagSummary}
    aria-label={`已有 ${visibleTags.length} 个 Tag：${visibleTags.join("、")}`}
    style:--tag-bg={tagTone.background}
    style:--tag-border={tagTone.border}
    style:--tag-text={tagTone.text}
    style:--tag-secondary={tagTone.secondary}
  >
    <span class="tag-label">{firstTag}</span>
    {#if remainingCount > 0}
      <span class="tag-more">+{remainingCount}</span>
    {/if}
  </span>
{/if}

<style>
  .card-tag-summary {
    display: inline-flex;
    align-items: center;
    min-width: 0;
    max-width: 58%;
    height: 18px;
    padding: 0 7px;
    border-radius: var(--radius-full);
    background: var(--tag-bg);
    box-shadow: inset 0 0 0 1px var(--tag-border);
    color: var(--tag-text);
    font-size: 10.5px;
    font-weight: 600;
    line-height: 1;
    white-space: nowrap;
  }

  .tag-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tag-more {
    margin-left: 4px;
    color: var(--tag-secondary);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    flex: none;
  }
</style>
