<script lang="ts">
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import type { RowRecord } from "../../api";
  import { closeSideBySide } from "../../stores/compare-store.svelte";
  import { rowFileName } from "../../utils/row-display";

  let {
    sample,
    target,
  }: {
    sample: RowRecord;
    target: RowRecord;
  } = $props();
</script>

<div class="side-by-side">
  <header class="head">
    <button type="button" class="back" onclick={closeSideBySide}>
      <ArrowLeft size={15} strokeWidth={2} />
      返回分区列表
    </button>
    <span class="titles">
      {rowFileName(sample) ?? "样本"} 对比 {rowFileName(target) ?? "目标"}
    </span>
  </header>
  <div class="panes">
    <figure class="pane">
      <div class="pane-head">样本</div>
      <figcaption>{rowFileName(sample) ?? "样本"}</figcaption>
    </figure>
    <figure class="pane">
      <div class="pane-head">目标</div>
      <figcaption>{rowFileName(target) ?? "目标"}</figcaption>
    </figure>
  </div>
</div>
