<script lang="ts">
  import { setDedupeAlias } from "../../api";
  import ContextMenuShell from "../../ui/ContextMenuShell.svelte";
  import { setNotice } from "../../stores/app-state.svelte";
  import { groupStore, removeGroup, renameExistingGroup } from "../../stores/group-store.svelte";
  import { requestGroupDelete } from "../../stores/group-delete-confirm.svelte";
  import {
    sectionMenu,
    hideSectionMenu,
  } from "../../stores/section-context-menu.svelte";

  const target = $derived(sectionMenu.target);
  const isGroup = $derived(target?.kind === "group");

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
      } else {
        setNotice({
          tone: "error",
          text: `重命名失败：${groupStore.error ?? "未知错误"}`,
        });
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

  function handleDelete(): void {
    if (!target || target.kind !== "group") return;
    hideSectionMenu();
    const group = groupStore.list.find(g => g.id === target.groupId);
    if (!group) return;
    requestGroupDelete(group, async () => {
      const ok = await removeGroup(group.id);
      if (ok) {
        setNotice({ tone: "success", text: `已删除分组「${group.name}」，组内图片已回到未分组。` });
      } else {
        setNotice({
          tone: "error",
          text: `删除分组失败：${groupStore.error ?? "未知错误"}`,
        });
      }
    });
  }
</script>

{#if target}
  <ContextMenuShell open={sectionMenu.open} x={sectionMenu.x} y={sectionMenu.y} onclose={hideSectionMenu}>
    <button type="button" role="menuitem" onclick={() => void handleRename()}>
      重命名
    </button>
    {#if isGroup}
      <div class="separator"></div>
      <button
        type="button"
        role="menuitem"
        class="danger"
        onclick={handleDelete}
      >
        删除分组
      </button>
    {/if}
  </ContextMenuShell>
{/if}
