<script lang="ts">
  import { suggestGroups, assignRowsToGroup, type SimilarityMode, type SuggestedGroup } from "../../api";
  import { app, errorText } from "../../stores/app-state.svelte";
  import { bumpGroupMembership, createNewGroup, loadGroups } from "../../stores/group-store.svelte";
  import { resetRows } from "../../stores/row-store.svelte";
  import { captureRowStates, recordRowStateChange } from "../../stores/history-actions";
  import { beginHistoryGroup, commitHistoryGroup } from "../../stores/history.svelte";

  let mode = $state<SimilarityMode>("artists");
  let threshold = $state(0.85);
  let running = $state(false);
  let suggestions = $state<SuggestedGroup[]>([]);
  let selected = $state<Set<number>>(new Set());
  let error = $state<string | null>(null);
  let applied = $state(false);
  let applyResult = $state<string | null>(null);

  function close(): void {
    app.groupSuggestOpen = false;
  }

  async function runSuggest(): Promise<void> {
    if (running) return;
    running = true;
    error = null;
    suggestions = [];
    selected = new Set();
    applied = false;
    applyResult = null;
    try {
      suggestions = await suggestGroups(mode, threshold);
      selected = new Set(suggestions.map((_, i) => i));
    } catch (e) {
      error = errorText(e);
    } finally {
      running = false;
    }
  }

  function toggleAll(checked: boolean): void {
    if (checked) {
      selected = new Set(suggestions.map((_, i) => i));
    } else {
      selected = new Set();
    }
  }

  function toggleOne(index: number): void {
    const next = new Set(selected);
    if (next.has(index)) {
      next.delete(index);
    } else {
      next.add(index);
    }
    selected = next;
  }

  async function applySelected(): Promise<void> {
    if (running || selected.size === 0) return;
    running = true;
    error = null;
    applyResult = null;
    let created = 0;
    let totalAssigned = 0;
    let before = [] as Awaited<ReturnType<typeof captureRowStates>>;
    let historyGroupStarted = false;
    try {
      const affectedRowIds = [...selected]
        .flatMap(index => suggestions[index].rowIds);
      before = await captureRowStates(affectedRowIds);
      historyGroupStarted = beginHistoryGroup("应用建议分组");
      for (const idx of selected) {
        const sg = suggestions[idx];
        const group = await createNewGroup(sg.name);
        if (!group) {
          throw new Error(`创建分组「${sg.name}」失败`);
        }
        await assignRowsToGroup(
          { kind: "explicit", rowIds: sg.rowIds },
          group.id,
        );
        created++;
        totalAssigned += sg.rowIds.length;
      }
      applyResult = `已创建 ${created} 个分组，分配 ${totalAssigned} 行`;
      applied = true;
      bumpGroupMembership();
      await loadGroups();
      resetRows();
      await recordRowStateChange("分配建议分组成员", before);
    } catch (e) {
      error = errorText(e);
      if (totalAssigned > 0 && before.length > 0) {
        try {
          await recordRowStateChange("分配建议分组成员（部分完成）", before);
        } catch (historyError) {
          error += `；部分变更未能记录撤销历史：${errorText(historyError)}`;
        }
      }
    } finally {
      if (historyGroupStarted) {
        commitHistoryGroup();
      }
      running = false;
    }
  }
</script>

<div class="overlay">
  <div class="panel">
    <header>
      <h2>建议分组</h2>
      <button type="button" class="close-btn" onclick={close}>&times;</button>
    </header>

    <div class="config">
      <label class="config-item">
        <span>比较模式</span>
        <select bind:value={mode} disabled={running}>
          <option value="artists">画师串（Jaro-Winkler）</option>
          <option value="positivePrompt">正向提示词（Token Jaccard）</option>
        </select>
      </label>
      <label class="config-item">
        <span>相似度阈值</span>
        <input type="number" bind:value={threshold} min="0.1" max="1.0" step="0.05" disabled={running} />
      </label>
      <button type="button" class="btn btn-primary" disabled={running} onclick={() => void runSuggest()}>
        {running ? "分析中…" : "开始分析"}
      </button>
    </div>

    {#if error}
      <p class="error">{error}</p>
    {/if}
    {#if applyResult}
      <p class="success">{applyResult}</p>
    {/if}

    {#if suggestions.length > 0 && !applied}
      <div class="results-header">
        <label>
          <input type="checkbox" checked={selected.size === suggestions.length} onchange={e => toggleAll((e.target as HTMLInputElement).checked)} />
          全选（{suggestions.length} 组，已选 {selected.size}）
        </label>
        <button type="button" class="btn btn-primary" disabled={running || selected.size === 0} onclick={() => void applySelected()}>
          创建选中的分组
        </button>
      </div>
      <div class="results-list">
        {#each suggestions as sg, i (i)}
          <label class="suggestion-row" class:is-selected={selected.has(i)}>
            <input type="checkbox" checked={selected.has(i)} onchange={() => toggleOne(i)} />
            <span class="sg-name">{sg.name}</span>
            <span class="sg-count">{sg.rowIds.length} 行</span>
          </label>
        {/each}
      </div>
    {:else if !running && suggestions.length === 0 && !error && !applied}
      {#if threshold > 0}
        <p class="empty">点击「开始分析」以查找相似行</p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-overlay);
    background: rgb(15 20 28 / 50%);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .panel {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-2);
    width: 560px;
    max-width: 95vw;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  header h2 {
    font-size: var(--font-lg);
    font-weight: 600;
  }

  .close-btn {
    border: none;
    background: none;
    font-size: var(--font-xl);
    color: var(--text-2);
    cursor: pointer;
  }

  .config {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  .config-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }

  .config-item span {
    font-size: var(--font-sm);
    color: var(--text-2);
    font-weight: 600;
  }

  .config-item select,
  .config-item input {
    padding: 5px 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
    font-size: var(--font-md);
  }

  .btn-primary {
    padding: 6px 16px;
    font-size: var(--font-md);
    background: var(--accent);
    color: white;
    border: 1px solid var(--accent);
    border-radius: var(--radius-s);
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .error {
    padding: 8px 18px;
    font-size: var(--font-md);
    color: var(--danger);
  }

  .success {
    padding: 8px 18px;
    font-size: var(--font-md);
    color: var(--success, #22c55e);
  }

  .empty {
    padding: 24px 18px;
    font-size: var(--font-md);
    color: var(--text-3);
    text-align: center;
  }

  .results-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 18px;
    border-bottom: 1px solid var(--border);
    flex: none;
    font-size: var(--font-md);
  }

  .results-header label {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .results-list {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding: 4px 18px 14px;
  }

  .suggestion-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
    font-size: var(--font-md);
    cursor: pointer;
  }

  .suggestion-row:hover {
    background: var(--surface-2);
  }

  .sg-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sg-count {
    font-size: var(--font-sm);
    color: var(--text-3);
    flex: none;
  }
</style>
