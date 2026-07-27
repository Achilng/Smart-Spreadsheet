<script lang="ts">
  import GripVertical from "@lucide/svelte/icons/grip-vertical";
  import X from "@lucide/svelte/icons/x";

  import type { GroupSummary, RuleAction, TagSummary } from "../../api";
  import RuleTagPicker from "./RuleTagPicker.svelte";

  let {
    action,
    groups,
    tags,
    tagsloading,
    onrefreshtags,
    onreplace,
    onremove,
    onmoveup,
    onmovedown,
    canmoveup,
    canmovedown,
  }: {
    action: RuleAction;
    groups: GroupSummary[];
    tags: TagSummary[];
    tagsloading: boolean;
    onrefreshtags: () => void | Promise<void>;
    onreplace: (action: RuleAction) => void;
    onremove: () => void;
    onmoveup: () => void;
    onmovedown: () => void;
    canmoveup: boolean;
    canmovedown: boolean;
  } = $props();

  const actionTypes: { value: RuleAction["type"]; label: string }[] = [
    { value: "addTags", label: "添加 Tag" },
    { value: "removeTags", label: "移除 Tag" },
    { value: "setGroup", label: "移入分组" },
    { value: "clearGroup", label: "清除分组" },
    { value: "appendPrompt", label: "追加提示词" },
    { value: "deletePromptTags", label: "删除指定提示词" },
    { value: "replacePrompt", label: "查找替换提示词" },
    { value: "prefixArtist", label: "修正 artist: 前缀" },
    { value: "setNote", label: "设置备注" },
    { value: "appendNote", label: "追加备注" },
    { value: "clearNote", label: "清空备注" },
    { value: "stopProcessing", label: "停止这张图片的后续规则" },
  ];

  function defaultAction(type: RuleAction["type"]): RuleAction {
    switch (type) {
      case "addTags": return { type, tags: [] };
      case "removeTags": return { type, tags: [] };
      case "setGroup": return { type, groupId: 0, onlyIfUngrouped: false };
      case "clearGroup": return { type };
      case "appendPrompt": return { type, field: "positive", value: "" };
      case "deletePromptTags": return { type, field: "positive", value: "" };
      case "replacePrompt": return { type, field: "positive", find: "", replace: "", caseSensitive: true };
      case "prefixArtist": return { type, artists: [] };
      case "setNote": return { type, value: "" };
      case "appendNote": return { type, value: "", separator: "\n" };
      case "clearNote": return { type };
      case "stopProcessing": return { type };
    }
  }

  function patch(values: Record<string, unknown>): void {
    onreplace({ ...action, ...values } as RuleAction);
  }

  function text(event: Event): string {
    return (event.currentTarget as HTMLInputElement | HTMLTextAreaElement).value;
  }

  function splitValues(value: string): string[] {
    return [...new Set(value.split(/[,，\n\r]/).map(item => item.trim()).filter(Boolean))];
  }

  function groupExists(groupId: number): boolean {
    return groups.some(group => group.id === groupId);
  }

  /** 当前任务是否已填了会因切换类型而丢失的内容 */
  function actionHasContent(value: RuleAction): boolean {
    switch (value.type) {
      case "addTags":
      case "removeTags": return value.tags.length > 0;
      case "appendPrompt":
      case "deletePromptTags":
      case "setNote":
      case "appendNote": return value.value.trim() !== "";
      case "replacePrompt": return value.find.trim() !== "" || value.replace.trim() !== "";
      case "prefixArtist": return value.artists.length > 0;
      default: return false;
    }
  }

  function onTypeChange(event: Event): void {
    const select = event.currentTarget as HTMLSelectElement;
    const nextType = select.value as RuleAction["type"];
    if (nextType === action.type) return;
    if (
      actionHasContent(action) &&
      !window.confirm("切换任务类型会清空这个任务里已填写的内容，确定切换吗？")
    ) {
      select.value = action.type;
      return;
    }
    onreplace(defaultAction(nextType));
  }

  function onRemoveClick(): void {
    if (
      actionHasContent(action) &&
      !window.confirm("这个任务已填写内容，确定删除吗？")
    ) {
      return;
    }
    onremove();
  }
</script>

<article class="action-card">
  <div class="action-order" aria-label="调整任务顺序">
    <GripVertical size={16} aria-hidden="true" />
    <button type="button" title="上移" disabled={!canmoveup} onclick={onmoveup}>↑</button>
    <button type="button" title="下移" disabled={!canmovedown} onclick={onmovedown}>↓</button>
  </div>
  <div class="action-content">
    <div class="action-head">
      <label><span>执行任务</span>
        <select value={action.type} onchange={onTypeChange}>
          {#each actionTypes as item}<option value={item.value}>{item.label}</option>{/each}
        </select>
      </label>
      <button type="button" class="icon-btn" title="删除任务" aria-label="删除任务" onclick={onRemoveClick}><X size={16} /></button>
    </div>

    <div class="action-body">
      {#if action.type === "addTags" || action.type === "removeTags"}
        <RuleTagPicker {tags} selected={action.tags} loading={tagsloading} onchange={value => patch({ tags: value })} onrefresh={onrefreshtags} />
        <p class="description">{action.type === "addTags" ? "命中后添加所选 Tag。" : "命中后移除所选 Tag。"}这里只能选择当前 Tag 库中已有的项目，避免输入错字。</p>
      {:else if action.type === "setGroup"}
        <label><span>目标分组</span><select disabled={groups.length === 0 && !action.groupId} value={action.groupId || ""} onchange={event => patch({ groupId: Number((event.currentTarget as HTMLSelectElement).value) || 0 })}><option value="">{groups.length === 0 ? "暂无已有分组" : "请选择已有分组"}</option>{#if action.groupId && !groupExists(action.groupId)}<option value={action.groupId}>已不存在的分组（#{action.groupId}）</option>{/if}{#each groups as group}<option value={group.id}>{group.name}</option>{/each}</select></label>
        <label class="check"><input type="checkbox" checked={action.onlyIfUngrouped} onchange={event => patch({ onlyIfUngrouped: (event.currentTarget as HTMLInputElement).checked })} />仅处理尚未分组的图片</label>
        {#if action.groupId && !groupExists(action.groupId)}
          <p class="warning" role="status">旧规则选择的分组已经不存在，请重新选择一个已有分组后再保存。</p>
        {:else if groups.length === 0}
          <p class="description">暂无可选分组，请先在主窗口创建分组。</p>
        {/if}
      {:else if action.type === "clearGroup"}
        <p class="description">移除命中图片当前所属的分组，不会删除分组本身。</p>
      {:else if action.type === "appendPrompt" || action.type === "deletePromptTags"}
        <label><span>提示词字段</span><select value={action.field} onchange={event => patch({ field: (event.currentTarget as HTMLSelectElement).value })}><option value="positive">正向提示词</option><option value="character">角色提示词</option><option value="negative">负向提示词</option></select></label>
        <label class="wide"><span>{action.type === "appendPrompt" ? "追加提示词（半角/全角逗号或换行分隔）" : "要删除的完整提示词（半角/全角逗号或换行分隔）"}</span><textarea rows="2" value={action.value} oninput={event => patch({ value: text(event) })}></textarea></label>
      {:else if action.type === "replacePrompt"}
        <label><span>提示词字段</span><select value={action.field} onchange={event => patch({ field: (event.currentTarget as HTMLSelectElement).value })}><option value="positive">正向提示词</option><option value="character">角色提示词</option><option value="negative">负向提示词</option></select></label>
        <div class="field-row"><label><span>查找</span><input value={action.find} oninput={event => patch({ find: text(event) })} /></label><label><span>替换为（可留空）</span><input value={action.replace} oninput={event => patch({ replace: text(event) })} /></label></div>
        <label class="check"><input type="checkbox" checked={action.caseSensitive} onchange={event => patch({ caseSensitive: (event.currentTarget as HTMLInputElement).checked })} />区分大小写</label>
      {:else if action.type === "prefixArtist"}
        <label class="wide"><span>需要修正的画师名（可省略 artist:，半角/全角逗号或换行分隔）</span><textarea rows="2" value={action.artists.join(", ")} oninput={event => patch({ artists: splitValues(text(event)) })}></textarea></label>
        <p class="description">同时检查正向、角色和负向提示词；画师串只根据正向和角色提示词重算。</p>
      {:else if action.type === "setNote" || action.type === "appendNote"}
        <label class="wide"><span>{action.type === "setNote" ? "新备注" : "追加内容"}</span><textarea rows="2" value={action.value} oninput={event => patch({ value: text(event) })}></textarea></label>
        {#if action.type === "appendNote"}<label><span>分隔符</span><select value={action.separator} onchange={event => patch({ separator: (event.currentTarget as HTMLSelectElement).value })}><option value={'\n'}>换行</option><option value="，">中文逗号</option><option value=" | ">竖线</option><option value=" ">空格</option></select></label>{/if}
      {:else if action.type === "clearNote"}
        <p class="description">清空命中图片的备注。</p>
      {:else if action.type === "stopProcessing"}
        <p class="description">只让命中的图片停止，不影响同一批次中的其他图片；本规则中排在它后面的任务仍会执行。</p>
      {/if}
    </div>
  </div>
</article>

<style>
  .action-card { display: flex; gap: 10px; padding: 12px; border: 1px solid var(--border); border-radius: var(--radius-s); background: var(--surface); }
  .action-order { width: 26px; flex: none; display: flex; flex-direction: column; align-items: center; gap: 3px; color: var(--text-3); }
  .action-order button { width: 24px; height: 23px; border: 0; border-radius: 5px; background: var(--surface-2); color: var(--text-2); }
  .action-order button:disabled { opacity: .3; }
  .action-content { min-width: 0; flex: 1; }
  .action-head { display: flex; align-items: end; justify-content: space-between; gap: 10px; }
  .action-body { display: grid; gap: 10px; margin-top: 10px; }
  .field-row { display: flex; flex-wrap: wrap; gap: 10px; }
  label { min-width: 170px; display: grid; gap: 4px; }
  label > span { color: var(--text-3); font-size: var(--font-xs); }
  .wide { width: 100%; }
  input, textarea { min-height: 32px; padding: 5px 9px; font: inherit; }
  textarea { resize: vertical; line-height: 1.5; }
  .icon-btn { width: 32px; height: 32px; display: grid; place-items: center; flex: none; border: 0; border-radius: 7px; background: transparent; color: var(--text-3); }
  .icon-btn:hover { background: var(--danger-soft); color: var(--danger); }
  .check { min-width: 0; display: flex; align-items: center; gap: 7px; color: var(--text-2); font-size: var(--font-sm); }
  .check input { min-height: 0; }
  .description { color: var(--text-3); font-size: var(--font-sm); line-height: 1.55; }
  .warning { padding: 8px 10px; border-radius: 7px; background: var(--warning-soft); color: var(--warning); font-size: var(--font-xs); line-height: 1.5; }
</style>
