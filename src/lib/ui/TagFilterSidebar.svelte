<script lang="ts">
  import type { Snippet } from "svelte";
  import { formatCount } from "../stores/app-state.svelte";
  import { tagStore } from "../stores/tag-store.svelte";
  import { tagColorFor } from "../utils/tag-colors";
  import "./tag-filter-sidebar.css";

  let { entries, activeTags, summary, ontoggle, oncontextmenu, modeLabel, onmode,
    error, emptyText = "还没有 Tag。选中图片后点“编辑 Tag”即可创建。", filters, statusContent }: {
    entries: { name: string; rowCount: number }[]; activeTags: string[]; summary: string;
    ontoggle: (name: string) => void; oncontextmenu?: (event: MouseEvent, name: string) => void;
    modeLabel?: string; onmode?: () => void;
    error?: string | null; emptyText?: string; filters?: Snippet; statusContent?: Snippet;
  } = $props();
</script>

<div class="tag-sidebar" data-tag-filter>
  <header class="sidebar-header"><div class="header-copy"><h3>筛选</h3><p class="header-sub tabular">{summary}</p></div></header>
  {@render filters?.()}
  <div class="f-head tag-head">Tag
    {#if onmode}<button type="button" class="mode-link" title="切换 Tag 筛选的组合方式" onclick={onmode}>{modeLabel}</button>
    {:else if modeLabel}<span class="mode-note">{modeLabel}</span>{/if}
  </div>
  <div class="tag-list">
    {#if error}<p class="list-note">Tag 列表加载失败：{error}</p>
    {:else if entries.length === 0}<p class="list-note faint">{emptyText}</p>
    {:else}
      {#each entries as entry (entry.name)}
        {@const filterOn = activeTags.includes(entry.name)}
        {@const tone = tagColorFor(entry.name, tagStore.list)}
        <button type="button" class="tag-row check-row" class:on={filterOn} aria-pressed={filterOn}
          onclick={() => ontoggle(entry.name)} oncontextmenu={event => oncontextmenu?.(event, entry.name)}>
          <span class="cbox" aria-hidden="true"></span>
          <span class="tag-color-swatch" style:--tag-color={tone.background} aria-hidden="true"></span>
          <span class="tag-name" title={entry.name}>{entry.name}</span><span class="tag-count">{formatCount(entry.rowCount)}</span>
        </button>
      {/each}
    {/if}
  </div>
  {@render statusContent?.()}
</div>
