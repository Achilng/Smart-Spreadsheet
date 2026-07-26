<script lang="ts">
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";

  import {
    addTagsToSelection,
    getRecentTags,
    listSelectionTags,
    removeTagsFromSelection,
    setRecentTags,
    type RowSelection,
  } from "../../api";
  import { errorText, formatCount } from "../../stores/app-state.svelte";
  import { captureSelectionStates, recordRowStateChange } from "../../stores/history-actions";
  import { resetRows } from "../../stores/row-store.svelte";
  import { loadTags, tagStore } from "../../stores/tag-store.svelte";
  import Modal from "../../ui/Modal.svelte";
  import { softFade } from "../../ui/motion";

  interface Props {
    selection: RowSelection;
    count: number;
    onclose: () => void;
  }

  let { selection, count, onclose }: Props = $props();

  type Coverage = "all" | "partial" | "none";

  let busy = $state(false);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let result = $state<string | null>(null);
  let error = $state<string | null>(null);
  let query = $state("");
  let recent = $state<string[]>([]);
  let coverage = $state(new Map<string, number>());
  // 暂存的修改：应用前不写库，取消零副作用
  let staged = $state(new Map<string, "add" | "remove">());
  // 会话内新建（库里还不存在）的 Tag 名，按新建顺序展示
  let created = $state<string[]>([]);

  const RECENT_LIMIT = 10;

  $effect(() => {
    void refresh();
  });

  async function refresh(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const [summaries, recentTags] = await Promise.all([
        listSelectionTags(selection),
        getRecentTags(),
        tagStore.loaded ? Promise.resolve() : loadTags(),
      ]);
      coverage = new Map(summaries.map(summary => [summary.name, summary.selectedRows]));
      recent = recentTags;
    } catch (cause) {
      loadError = errorText(cause);
    } finally {
      loading = false;
    }
  }

  function baseCoverage(name: string): Coverage {
    const matched = coverage.get(name) ?? 0;
    if (count > 0 && matched >= count) {
      return "all";
    }
    return matched > 0 ? "partial" : "none";
  }

  /** 暂存修改后的显示态 */
  function displayState(name: string): Coverage {
    const stagedOp = staged.get(name);
    if (stagedOp === "add") return "all";
    if (stagedOp === "remove") return "none";
    return baseCoverage(name);
  }

  /**
   * 点击循环：无 → 添加；全有 → 移除；
   * 部分 → 添加 → 移除 → 回部分（三态循环）。
   */
  function cycle(name: string): void {
    if (busy) return;
    result = null;
    const base = baseCoverage(name);
    const stagedOp = staged.get(name);
    const next = new Map(staged);
    if (stagedOp === undefined) {
      next.set(name, base === "all" ? "remove" : "add");
    } else if (stagedOp === "add" && base !== "none") {
      // 部分/全有：add 之后还能切到 remove
      next.set(name, "remove");
    } else if (stagedOp === "add") {
      next.delete(name);
      // 新建的伪条目取消暂存时一并移除
      created = created.filter(item => item !== name);
    } else {
      next.delete(name);
    }
    staged = next;
  }

  const trimmedQuery = $derived(query.trim());

  /** 全部候选：库内 Tag + 会话内新建（去重） */
  const allNames = $derived.by(() => {
    const known = new Set(tagStore.list.map(tag => tag.name));
    return [...created.filter(name => !known.has(name)), ...tagStore.list.map(tag => tag.name)];
  });

  const filtered = $derived.by(() => {
    if (trimmedQuery === "") return allNames;
    const needle = trimmedQuery.toLocaleLowerCase();
    return allNames.filter(name => name.toLocaleLowerCase().includes(needle));
  });

  /** 无搜索时把最近使用的置顶为独立区 */
  const recentSection = $derived.by(() => {
    if (trimmedQuery !== "") return [];
    const available = new Set(allNames);
    return recent.filter(name => available.has(name));
  });

  const restSection = $derived.by(() => {
    if (trimmedQuery !== "") return filtered;
    const top = new Set(recentSection);
    return filtered.filter(name => !top.has(name));
  });

  /** 输入的名称尚不存在时提供"新建并勾选" */
  const canCreate = $derived(
    trimmedQuery !== "" && !allNames.some(name => name === trimmedQuery),
  );

  const stagedAdds = $derived([...staged.entries()].filter(([, op]) => op === "add").map(([name]) => name));
  const stagedRemoves = $derived([...staged.entries()].filter(([, op]) => op === "remove").map(([name]) => name));

  function splitNames(value: string): string[] {
    return [...new Set(value.split(/[,，\n\r]/).map(item => item.trim()).filter(Boolean))];
  }

  /** 新建并暂存为添加；支持逗号/换行分隔批量输入 */
  function createAndStage(): void {
    if (busy || trimmedQuery === "") return;
    result = null;
    const names = splitNames(trimmedQuery);
    const known = new Set(allNames);
    const next = new Map(staged);
    for (const name of names) {
      if (!known.has(name)) {
        created = [name, ...created];
      }
      next.set(name, "add");
    }
    staged = next;
    query = "";
  }

  function onQueryKeydown(event: KeyboardEvent): void {
    if (event.key !== "Enter") return;
    event.preventDefault();
    if (canCreate || /[,，\n]/.test(trimmedQuery)) {
      createAndStage();
    }
  }

  async function apply(): Promise<void> {
    if (busy || staged.size === 0) return;
    busy = true;
    error = null;
    result = null;
    try {
      const adds = stagedAdds;
      const removes = stagedRemoves;
      const before = await captureSelectionStates(selection);
      if (adds.length > 0) {
        await addTagsToSelection(selection, adds);
      }
      if (removes.length > 0) {
        await removeTagsFromSelection(selection, removes);
      }
      resetRows();
      await loadTags();
      await recordRowStateChange("编辑 Tag", before);
      if (adds.length > 0) {
        const merged = [...adds, ...recent.filter(name => !adds.includes(name))].slice(0, RECENT_LIMIT);
        recent = merged;
        void setRecentTags(merged).catch(() => {});
      }
      const parts = [
        adds.length > 0 ? `添加 ${adds.length} 个 Tag` : "",
        removes.length > 0 ? `移除 ${removes.length} 个 Tag` : "",
      ].filter(Boolean);
      result = `已对 ${formatCount(count)} 张图片${parts.join("、")}。`;
      staged = new Map();
      created = [];
      const summaries = await listSelectionTags(selection);
      coverage = new Map(summaries.map(summary => [summary.name, summary.selectedRows]));
    } catch (cause) {
      error = errorText(cause);
    } finally {
      busy = false;
    }
  }

  function requestClose(): void {
    if (busy) return;
    if (
      staged.size > 0 &&
      !window.confirm("有未应用的 Tag 修改，确定放弃吗？")
    ) {
      return;
    }
    onclose();
  }
</script>

<Modal open={true} onclose={requestClose} {busy} width="440px">
  <div class="dialog-content">
    <header>
      <h3>编辑 Tag（{formatCount(count)} 张）</h3>
      <button type="button" class="close-btn" aria-label="关闭" disabled={busy} onclick={requestClose}>
        <X size={15} strokeWidth={2} />
      </button>
    </header>

    <div class="body">
      <input
        type="search"
        class="query"
        placeholder="搜索或输入新 Tag（逗号可分隔多个）"
        bind:value={query}
        disabled={busy}
        autocomplete="off"
        onkeydown={onQueryKeydown}
      />

      {#if loading}
        <div class="state-line"><span class="spinner" aria-hidden="true"></span>正在读取 Tag 状态…</div>
      {:else if loadError}
        <p class="state-line is-error">加载失败：{loadError}</p>
      {:else}
        <div class="tag-scroll" role="listbox" aria-label="Tag 列表" aria-multiselectable="true">
          {#if canCreate}
            <button type="button" class="create-line" disabled={busy} onclick={createAndStage}>
              <Plus size={14} aria-hidden="true" />
              新建并勾选「{trimmedQuery}」
            </button>
          {/if}

          {#if recentSection.length > 0}
            <div class="overline section-label">最近使用</div>
            {#each recentSection as name (name)}
              {@render tagRow(name)}
            {/each}
            <div class="overline section-label">全部 Tag</div>
          {/if}

          {#each restSection as name (name)}
            {@render tagRow(name)}
          {/each}

          {#if restSection.length === 0 && recentSection.length === 0 && !canCreate}
            <p class="state-line faint">
              {trimmedQuery === "" ? "还没有 Tag，输入名称即可新建。" : "没有匹配的 Tag。"}
            </p>
          {/if}
        </div>

        <footer class="apply-row">
          <span class="staged-summary tabular" class:faint={staged.size === 0}>
            {#if staged.size === 0}
              点击 Tag 暂存修改，应用前不写入
            {:else}
              {[
                stagedAdds.length > 0 ? `${stagedAdds.length} 项添加` : "",
                stagedRemoves.length > 0 ? `${stagedRemoves.length} 项移除` : "",
              ].filter(Boolean).join(" · ")}
            {/if}
          </span>
          <div class="apply-actions">
            {#if staged.size > 0}
              <button
                type="button"
                class="txt-opt"
                disabled={busy}
                transition:softFade={{ duration: 120 }}
                onclick={() => { staged = new Map(); created = []; result = null; }}
              >还原</button>
            {/if}
            <button
              type="button"
              class="btn btn-primary"
              disabled={busy || staged.size === 0}
              onclick={() => void apply()}
            >{busy ? "应用中…" : "应用修改"}</button>
          </div>
        </footer>

        {#if result}<p class="state-line is-ok" transition:softFade={{ duration: 140 }}>{result}</p>{/if}
        {#if error}<p class="state-line is-error" transition:softFade={{ duration: 140 }}>{error}</p>{/if}
      {/if}
    </div>
  </div>
</Modal>

{#snippet tagRow(name: string)}
  {@const shown = displayState(name)}
  {@const stagedOp = staged.get(name)}
  {@const isNew = created.includes(name)}
  <button
    type="button"
    class="tag-row"
    role="option"
    aria-selected={shown === "all"}
    disabled={busy}
    onclick={() => cycle(name)}
  >
    <span
      class="cbox"
      class:on={shown === "all"}
      class:is-partial={shown === "partial"}
      aria-hidden="true"
    ></span>
    <span class="tag-name" title={name}>{name}</span>
    {#if isNew}
      <span class="chip mini-chip">新</span>
    {/if}
    {#if stagedOp}
      <span class="staged-mark" class:is-remove={stagedOp === "remove"}>
        {stagedOp === "add" ? "+" : "−"}
      </span>
    {:else}
      <span class="row-count tabular">{coverage.get(name) ?? 0}/{formatCount(count)}</span>
    {/if}
  </button>
{/snippet}

<style>
  .dialog-content {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  header h3 {
    font-size: var(--font-base);
    font-weight: 600;
  }

  .close-btn {
    display: grid;
    place-items: center;
    border: none;
    background: none;
    color: var(--text-2);
    padding: 4px;
    border-radius: var(--radius-s);
  }

  .close-btn:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }

  .body {
    padding: 14px 16px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }

  .query {
    width: 100%;
  }

  .tag-scroll {
    max-height: min(46vh, 380px);
    min-height: 120px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin: 0 -6px;
    padding: 0 6px;
  }

  .section-label {
    padding: 8px 8px 4px;
  }

  .create-line {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 8px;
    border: none;
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--accent);
    font-size: var(--font-sm);
    text-align: left;
    transition: background var(--motion-fast) var(--ease-responsive);
  }

  .create-line:hover:not(:disabled) {
    background: var(--accent-soft);
  }

  .tag-row {
    display: flex;
    align-items: center;
    gap: 9px;
    min-height: 32px;
    padding: 4px 8px;
    border: none;
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--text-2);
    font-size: var(--font-md);
    text-align: left;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive);
  }

  .tag-row:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }

  .tag-row:disabled {
    opacity: 0.6;
  }

  .cbox {
    width: 16px;
    height: 16px;
    border-radius: 5px;
    flex: none;
    border: 1.5px solid var(--border-strong);
    background: var(--surface);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive);
  }

  .cbox.on {
    background-color: var(--primary);
    background-image: url('data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M4 8.5 6.8 11 12 5.5" fill="none" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>');
    background-position: center;
    background-size: 12px;
    background-repeat: no-repeat;
    border-color: var(--primary);
  }

  .cbox.is-partial {
    background-color: var(--primary);
    background-image: linear-gradient(#ffffff, #ffffff);
    background-position: center;
    background-size: 8px 2px;
    background-repeat: no-repeat;
    border-color: var(--primary);
  }

  .tag-name {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mini-chip {
    font-size: 10px;
    padding: 0 7px;
    color: var(--accent);
    background: var(--accent-soft);
  }

  .staged-mark {
    flex: none;
    min-width: 16px;
    text-align: center;
    color: var(--accent);
    font-size: var(--font-md);
    font-weight: 700;
  }

  .staged-mark.is-remove {
    color: var(--danger);
  }

  .row-count {
    flex: none;
    color: var(--text-4);
    font-size: var(--font-xs);
  }

  .apply-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding-top: 4px;
    border-top: 1px solid var(--border);
  }

  .staged-summary {
    font-size: var(--font-sm);
    color: var(--text-2);
  }

  .staged-summary.faint {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .apply-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }

  .state-line {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--font-sm);
    color: var(--text-2);
  }

  .state-line.faint {
    color: var(--text-3);
    padding: 12px 8px;
  }

  .state-line.is-ok {
    color: var(--success);
  }

  .state-line.is-error {
    color: var(--danger);
  }
</style>
