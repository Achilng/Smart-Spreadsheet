<script lang="ts">
  import { tagStore } from "../stores/tag-store.svelte";

  let { tags }: { tags: string[] } = $props();

  /**
   * ColorBrewer Paired 12 定性分类色板：用浅/深成对和冷暖交替强化类别区分。
   * 每组单独选择深色或白色文字，最小对比度为 4.66:1。
   */
  const TAG_PALETTE = [
    { background: "#a6cee3", text: "#16313f" },
    { background: "#1f78b4", text: "#ffffff" },
    { background: "#b2df8a", text: "#213615" },
    { background: "#33a02c", text: "#10280e" },
    { background: "#fb9a99", text: "#4a1515" },
    { background: "#e31a1c", text: "#ffffff" },
    { background: "#fdbf6f", text: "#3b2505" },
    { background: "#ff7f00", text: "#3d1c00" },
    { background: "#cab2d6", text: "#321f3c" },
    { background: "#6a3d9a", text: "#ffffff" },
    { background: "#ffff99", text: "#333300" },
    { background: "#b15928", text: "#ffffff" },
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
    style:--tag-text={tagTone.text}
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
    height: 20px;
    padding: 0 8px;
    border-radius: var(--radius-full);
    background: var(--tag-bg);
    box-shadow:
      inset 0 0 0 1px rgb(0 0 0 / 10%),
      0 1px 2px rgb(0 0 0 / 12%);
    color: var(--tag-text);
    font-size: 10.5px;
    font-weight: 700;
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
    color: currentColor;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    flex: none;
    opacity: 0.72;
  }
</style>
