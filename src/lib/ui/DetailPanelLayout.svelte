<script lang="ts">
  import type { Snippet } from "svelte";
  import ChevronsRight from "@lucide/svelte/icons/chevrons-right";
  import "./detail-panel.css";

  let { title = "详情", subtitle, empty = false, emptyText = "点击图片或行查看详情", onedit, ondelete, oncollapse, children }: {
    title?: string; subtitle?: string; empty?: boolean; emptyText?: string;
    onedit?: () => void; ondelete?: () => void; oncollapse: () => void; children: Snippet;
  } = $props();
</script>

<div class="detail-panel" data-detail-panel>
  <header class="panel-header">
    <div class="header-copy">
      <h3 title={title}>{title}</h3>
      {#if subtitle}<p class="header-sub tabular">{subtitle}</p>{/if}
    </div>
    <div class="panel-actions">
      {#if !empty && onedit}<button type="button" class="btn btn-ghost delete-btn" onclick={onedit}>编辑</button>{/if}
      {#if !empty && ondelete}<button type="button" class="btn btn-danger delete-btn" onclick={ondelete}>删除</button>{/if}
      <button type="button" class="btn btn-ghost collapse-btn" title="收起详情面板" aria-label="收起详情面板" onclick={oncollapse}><ChevronsRight size={15} strokeWidth={1.8} /></button>
    </div>
  </header>
  {#if empty}
    <div class="panel-empty"><p class="faint">{emptyText}</p></div>
  {:else}
    <div class="panel-scroll">{@render children()}</div>
  {/if}
</div>
