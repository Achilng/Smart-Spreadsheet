<script lang="ts">
  import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
  import { SvelteSet } from "svelte/reactivity";

  import { addTagsToSelection, removeTagsFromSelection } from "../../api";
  import { errorText, formatCount, setNotice } from "../app-state.svelte";
  import { deletion, requestDelete } from "../delete-actions.svelte";
  import { resetRows } from "../row-store.svelte";
  import {
    clearSelection,
    getSelectedCount,
    selection,
    selectionDto,
    selectionIds,
  } from "../selection-store.svelte";
  import { loadTags, tagStore } from "../tag-store.svelte";

  type Mutation = "add" | "remove";

  const count = $derived(getSelectedCount());

  let popover = $state<Mutation | null>(null);
  let search = $state("");
  let applying = $state(false);
  const picked = new SvelteSet<string>();

  const visibleTags = $derived.by(() => {
    const query = search.trim().toLocaleLowerCase();
    return tagStore.list
      .filter(tag => query === "" || tag.name.toLocaleLowerCase().includes(query))
      .map(tag => tag.name);
  });

  function openPopover(mutation: Mutation): void {
    popover = popover === mutation ? null : mutation;
    search = "";
    picked.clear();
  }

  function togglePick(name: string): void {
    if (picked.has(name)) {
      picked.delete(name);
    } else {
      picked.add(name);
    }
  }

  async function apply(): Promise<void> {
    const mutation = popover;
    const tags = [...picked];
    if (!mutation || tags.length === 0 || applying) {
      return;
    }
    if (mutation === "remove") {
      const confirmed = await confirmDialog(
        `将从 ${formatCount(count)} 行移除 ${tags.length} 个 Tag。是否继续？`,
        { title: "批量移除 Tag", kind: "warning", okLabel: "移除", cancelLabel: "取消" },
      );
      if (!confirmed) {
        return;
      }
    }
    applying = true;
    try {
      const dto = selectionDto();
      const result =
        mutation === "add"
          ? await addTagsToSelection(dto, tags)
          : await removeTagsFromSelection(dto, tags);
      setNotice({
        tone: "success",
        text: `已处理 ${formatCount(result.affectedRows)} 行，实际变更 ${formatCount(result.associationsChanged)} 个 Tag 关联。`,
      });
      popover = null;
      clearSelection();
      resetRows();
      await loadTags();
    } catch (error) {
      setNotice({ tone: "error", text: `批量操作失败：${errorText(error)}` });
    } finally {
      applying = false;
    }
  }

  function dismiss(): void {
    if (!applying) {
      popover = null;
      clearSelection();
    }
  }
</script>

<svelte:window
  onkeydown={event => {
    if (event.key === "Escape" && count > 0 && !applying && !deletion.open) {
      if (popover) {
        popover = null;
      } else {
        clearSelection();
      }
    }
  }}
/>

{#if count > 0}
  <div class="selection-bar-wrap">
    {#if popover}
      <div class="tag-popover">
        <input
          type="search"
          placeholder="搜索 Tag…"
          bind:value={search}
          disabled={applying}
          autocomplete="off"
        />
        <div class="popover-list">
          {#if tagStore.list.length === 0}
            <p class="faint">还没有 Tag，请先在左侧 Tag 库新建。</p>
          {:else if visibleTags.length === 0}
            <p class="faint">没有匹配的 Tag。</p>
          {:else}
            {#each visibleTags as name (name)}
              <label class="popover-item">
                <input
                  type="checkbox"
                  checked={picked.has(name)}
                  disabled={applying}
                  onchange={() => togglePick(name)}
                />
                <span title={name}>{name}</span>
              </label>
            {/each}
          {/if}
        </div>
        <div class="popover-actions">
          <span class="faint">{picked.size > 0 ? `已选 ${picked.size} 个 Tag` : "选择要操作的 Tag"}</span>
          <button
            type="button"
            class="btn {popover === 'remove' ? 'btn-danger' : 'btn-primary'}"
            disabled={applying || picked.size === 0}
            onclick={() => void apply()}
          >
            {applying ? "处理中…" : popover === "add" ? "添加到所选行" : "从所选行移除"}
          </button>
        </div>
      </div>
    {/if}

    <div class="selection-bar">
      <span class="count">
        已选 {formatCount(count)} 行
        {#if selection.kind === "filtered"}
          <small class="faint">（筛选全选{selectionIds.size > 0 ? `，排除 ${formatCount(selectionIds.size)} 行` : ""}）</small>
        {/if}
      </span>
      <button
        type="button"
        class="btn"
        class:is-open={popover === "add"}
        disabled={applying}
        onclick={() => openPopover("add")}
      >
        + 添加 Tag
      </button>
      <button
        type="button"
        class="btn"
        class:is-open={popover === "remove"}
        disabled={applying}
        onclick={() => openPopover("remove")}
      >
        − 移除 Tag
      </button>
      <button
        type="button"
        class="btn btn-danger"
        disabled={applying}
        onclick={() => requestDelete(selectionDto(), count)}
      >
        删除
      </button>
      <button type="button" class="btn btn-ghost" disabled={applying} onclick={dismiss}>
        清除
      </button>
    </div>
  </div>
{/if}

<style>
  .selection-bar-wrap {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 40;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .selection-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    box-shadow: var(--shadow-2);
    padding: 8px 10px 8px 16px;
    white-space: nowrap;
  }

  .count {
    font-size: 13px;
    font-weight: 600;
    margin-right: 4px;
  }

  .count small {
    font-weight: 400;
    font-size: 12px;
  }

  .selection-bar .btn {
    padding: 4px 12px;
    font-size: 12.5px;
    border-radius: 999px;
  }

  .selection-bar .btn.is-open {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }

  .tag-popover {
    width: 300px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-2);
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .tag-popover > input {
    padding: 6px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    font-size: 13px;
  }

  .tag-popover > input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .popover-list {
    max-height: 200px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 13px;
  }

  .popover-list p {
    padding: 6px;
    font-size: 12px;
  }

  .popover-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    border-radius: var(--radius-s);
    cursor: pointer;
  }

  .popover-item:hover {
    background: var(--surface-2);
  }

  .popover-item span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .popover-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
  }
</style>
