<script lang="ts">
  let { tags }: { tags: string[] } = $props();

  const visibleTags = $derived(tags.filter(tag => tag.trim().length > 0));
  const displayedTags = $derived(visibleTags.slice(0, 2));
  const tagSummary = $derived(visibleTags.join("\n"));
</script>

{#if displayedTags.length > 0}
  <span
    class="card-tag-list"
    title={tagSummary}
    aria-label={`已有 ${visibleTags.length} 个 Tag：${visibleTags.join("、")}`}
  >
    {#each displayedTags as tag (tag)}
      <span class="card-tag-pill"><span class="tag-label">{tag}</span></span>
    {/each}
  </span>
{/if}

<style>
  .card-tag-list {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    min-width: 0;
    max-width: 72%;
    overflow: hidden;
  }

  .card-tag-pill {
    display: inline-flex;
    align-items: center;
    min-width: 0;
    max-width: 68px;
    height: 20px;
    padding: 0 8px;
    border-radius: var(--radius-full);
    background: #3c66bf;
    box-shadow:
      inset 0 0 0 1px rgb(0 0 0 / 10%),
      0 1px 2px rgb(0 0 0 / 12%);
    color: #fff;
    font-size: 10.5px;
    font-weight: 700;
    line-height: 1;
    white-space: nowrap;
    flex: 0 1 auto;
  }

  .tag-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

</style>
