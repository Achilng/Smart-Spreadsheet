<script lang="ts">
  import { setDedupeAlias } from "../../api";
  import { setNotice } from "../../stores/app-state.svelte";
  import { removeGroup, renameExistingGroup } from "../../stores/group-store.svelte";
  import {
    sectionMenu,
    hideSectionMenu,
  } from "../../stores/section-context-menu.svelte";

  const target = $derived(sectionMenu.target);
  const isGroup = $derived(target?.kind === "group");

  let menuEl = $state<HTMLDivElement | null>(null);

  function handlePointerDown(event: PointerEvent): void {
    if (sectionMenu.open && menuEl && !menuEl.contains(event.target as Node)) {
      hideSectionMenu();
    }
  }

  async function handleRename(): Promise<void> {
    if (!target) return;
    hideSectionMenu();
    const currentName =
      target.kind === "group" ? target.name : target.displayName;
    const newName = window.prompt("输入新名称", currentName);
    if (newName === null || newName.trim() === "" || newName === currentName)
      return;

    if (target.kind === "group") {
      const ok = await renameExistingGroup(target.groupId, newName.trim());
      if (ok) {
        setNotice({ tone: "success", text: `已重命名分组为「${newName.trim()}」` });
      }
    } else {
      try {
        await setDedupeAlias(target.mode, target.key, newName.trim());
        setNotice({
          tone: "success",
          text: `已设置别名「${newName.trim()}」`,
        });
        sectionMenu.aliasVersion += 1;
      } catch (e) {
        setNotice({
          tone: "error",
          text: `设置别名失败：${e instanceof Error ? e.message : String(e)}`,
        });
      }
    }
  }

  async function handleDelete(): Promise<void> {
    if (!target || target.kind !== "group") return;
    hideSectionMenu();
    const ok = await removeGroup(target.groupId);
    if (ok) {
      setNotice({ tone: "success", text: `已删除分组「${target.name}」` });
    }
  }
</script>

<svelte:window
  onpointerdown={handlePointerDown}
  onkeydown={(e) => {
    if (e.key === "Escape" && sectionMenu.open) hideSectionMenu();
  }}
/>

{#if sectionMenu.open && target}
  <div
    class="section-context-menu"
    bind:this={menuEl}
    role="menu"
    style:left="{sectionMenu.x}px"
    style:top="{sectionMenu.y}px"
  >
    <button type="button" role="menuitem" onclick={() => void handleRename()}>
      重命名
    </button>
    {#if isGroup}
      <div class="separator"></div>
      <button
        type="button"
        role="menuitem"
        class="danger"
        onclick={() => void handleDelete()}
      >
        删除分组
      </button>
    {/if}
  </div>
{/if}

<style>
  .section-context-menu {
    position: fixed;
    z-index: 9999;
    min-width: 160px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-2);
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .section-context-menu button {
    display: flex;
    align-items: center;
    width: 100%;
    border: none;
    background: transparent;
    border-radius: 4px;
    padding: 6px 10px;
    text-align: left;
    font-size: 13px;
    color: var(--text);
    cursor: default;
  }

  .section-context-menu button:hover {
    background: var(--surface-2);
  }

  .section-context-menu button.danger {
    color: var(--danger);
  }

  .separator {
    height: 1px;
    background: var(--border);
    margin: 3px 4px;
  }
</style>
