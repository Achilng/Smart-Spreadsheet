<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Search from "@lucide/svelte/icons/search";
  import X from "@lucide/svelte/icons/x";

  import type { TagSummary } from "../../api";

  let {
    tags,
    selected,
    loading = false,
    onchange,
    onrefresh,
  }: {
    tags: TagSummary[];
    selected: string[];
    loading?: boolean;
    onchange: (tags: string[]) => void;
    onrefresh?: () => void | Promise<void>;
  } = $props();

  let query = $state("");

  const normalizedSelected = $derived(normalize(selected));
  const availableNames = $derived(new Set(tags.map(tag => tag.name)));
  const missingSelected = $derived(normalizedSelected.filter(name => !availableNames.has(name)));
  const filteredTags = $derived.by(() => {
    const keyword = query.trim().toLocaleLowerCase();
    const matches = keyword
      ? tags.filter(tag => tag.name.toLocaleLowerCase().includes(keyword))
      : tags;
    return matches.slice(0, 80);
  });

  function normalize(values: string[]): string[] {
    return [...new Set(values.flatMap(value => value.split(/[,，\n\r]/)).map(value => value.trim()).filter(Boolean))];
  }

  function toggle(name: string): void {
    onchange(normalizedSelected.includes(name)
      ? normalizedSelected.filter(value => value !== name)
      : [...normalizedSelected, name]);
  }
</script>

<div class="tag-picker">
  <div class="picker-head">
    <label>
      <span>从已有 Tag 中选择</span>
      <span class="search-box">
        <Search size={15} aria-hidden="true" />
        <input bind:value={query} type="search" placeholder="搜索 Tag" aria-label="搜索已有 Tag" />
      </span>
    </label>
    {#if onrefresh}
      <button type="button" class="refresh-btn" disabled={loading} title="重新读取 Tag 列表" onclick={() => void onrefresh?.()}>
        <span class:spin={loading}><RefreshCw size={15} aria-hidden="true" /></span>
        {loading ? "读取中…" : "刷新"}
      </button>
    {/if}
  </div>

  {#if normalizedSelected.length > 0}
    <div class="selected-list" aria-label="已选择的 Tag">
      {#each normalizedSelected as name (name)}
        <button
          type="button"
          class:is-missing={!availableNames.has(name)}
          title={`移除 Tag「${name}」`}
          onclick={() => toggle(name)}
        >
          {name}<X size={13} aria-hidden="true" />
        </button>
      {/each}
    </div>
  {/if}

  {#if missingSelected.length > 0}
    <p class="warning" role="status">
      旧规则中有 {missingSelected.length} 个 Tag 已不在当前 Tag 库中。保留它们仍可能在执行时被重新创建；请移除，或先在主窗口创建对应 Tag。
    </p>
  {/if}

  <div class="tag-options" aria-label="可选择的已有 Tag">
    {#each filteredTags as tag (tag.name)}
      <button
        type="button"
        class:is-selected={normalizedSelected.includes(tag.name)}
        aria-pressed={normalizedSelected.includes(tag.name)}
        onclick={() => toggle(tag.name)}
      >
        <span class="check"><Check size={13} aria-hidden="true" /></span>
        <span class="tag-name">{tag.name}</span>
        <span class="tag-count">{tag.rowCount.toLocaleString("zh-CN")}</span>
      </button>
    {:else}
      <p class="empty">
        {tags.length === 0 ? "暂无已有 Tag，请先在主窗口的 Tag 库中创建。" : "没有匹配的 Tag。"}
      </p>
    {/each}
  </div>
  {#if filteredTags.length === 80 && tags.length > 80}
    <p class="limit-hint">结果较多，仅显示前 80 项；可继续输入关键词缩小范围。</p>
  {/if}
</div>

<style>
  .tag-picker { display: grid; gap: 8px; }
  .picker-head { display: flex; align-items: end; gap: 8px; }
  .picker-head label { min-width: 0; flex: 1; display: grid; gap: 4px; }
  .picker-head label > span:first-child { color: var(--text-3); font-size: var(--font-xs); }
  .search-box { min-height: 34px; display: flex; align-items: center; gap: 7px; padding: 0 9px; border: 1px solid var(--border); border-radius: 7px; background: var(--surface); color: var(--text-3); }
  .search-box:focus-within { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-soft); }
  .search-box input { min-width: 0; min-height: 0; flex: 1; padding: 0; border: 0; outline: 0; background: transparent; font: inherit; color: var(--text); }
  .refresh-btn { min-height: 34px; display: inline-flex; align-items: center; gap: 5px; padding: 5px 9px; border: 1px solid var(--border); border-radius: 7px; background: var(--surface); color: var(--text-2); font-size: var(--font-sm); }
  .refresh-btn:hover:not(:disabled) { background: var(--surface-2); }
  .selected-list { display: flex; flex-wrap: wrap; gap: 6px; }
  .selected-list button { min-height: 27px; display: inline-flex; align-items: center; gap: 5px; padding: 3px 7px 3px 9px; border: 1px solid var(--accent); border-radius: var(--radius-full); background: var(--accent-soft); color: var(--accent); font-size: var(--font-sm); }
  .selected-list button.is-missing { border-color: var(--warning); background: var(--warning-soft); color: var(--warning); }
  .warning { padding: 8px 10px; border-radius: 7px; background: var(--warning-soft); color: var(--warning); font-size: var(--font-xs); line-height: 1.5; }
  .tag-options { max-height: 170px; overflow-y: auto; display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 5px; padding: 7px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface-2); }
  .tag-options button { min-width: 0; min-height: 31px; display: flex; align-items: center; gap: 6px; padding: 5px 7px; border: 1px solid transparent; border-radius: 6px; background: var(--surface); color: var(--text-2); text-align: left; }
  .tag-options button:hover { border-color: var(--border-strong); }
  .tag-options button.is-selected { border-color: var(--accent); background: var(--accent-soft); color: var(--accent); }
  .check { width: 16px; height: 16px; display: grid; place-items: center; flex: none; border: 1px solid var(--border-strong); border-radius: 4px; color: transparent; }
  .is-selected .check { border-color: var(--accent); background: var(--accent); color: white; }
  .tag-name { min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tag-count { flex: none; color: var(--text-4); font-size: var(--font-xs); }
  .empty { grid-column: 1 / -1; padding: 15px 8px; color: var(--text-3); font-size: var(--font-sm); text-align: center; }
  .limit-hint { color: var(--text-4); font-size: var(--font-xs); }
  .refresh-btn > span { width: 15px; height: 15px; display: grid; place-items: center; }
  .spin { animation: spin .8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
