<script lang="ts">
  import X from "@lucide/svelte/icons/x";

  import {
    defaultComparison,
    defaultRuleCondition,
    type GroupSummary,
    type RuleCondition,
  } from "../../api";
  import RuleNumericEditor from "./RuleNumericEditor.svelte";

  let {
    condition,
    groups,
    onreplace,
    onremove,
  }: {
    condition: RuleCondition;
    groups: GroupSummary[];
    onreplace: (condition: RuleCondition) => void;
    onremove: () => void;
  } = $props();

  const conditionTypes: { value: RuleCondition["type"]; label: string }[] = [
    { value: "prompt", label: "提示词" },
    { value: "tag", label: "Tag" },
    { value: "group", label: "分组" },
    { value: "artist", label: "画师" },
    { value: "note", label: "备注" },
    { value: "fileText", label: "文件名或路径" },
    { value: "fileSize", label: "文件大小" },
    { value: "sourceType", label: "导入类型" },
    { value: "vibe", label: "VIBE" },
    { value: "metadata", label: "元数据状态" },
    { value: "imageDimension", label: "图片尺寸或比例" },
    { value: "orientation", label: "横竖构图" },
    { value: "generationText", label: "模型、采样器或种子" },
    { value: "generationNumber", label: "生成数值参数" },
  ];

  function patch(values: Record<string, unknown>): void {
    onreplace({ ...condition, ...values } as RuleCondition);
  }

  function text(event: Event): string {
    return (event.currentTarget as HTMLInputElement | HTMLTextAreaElement).value;
  }

  function splitValues(value: string): string[] {
    return [...new Set(value.split(/[,，\n\r]/).map(item => item.trim()).filter(Boolean))];
  }

  /** 当前条件是否已填了会因切换类型而丢失的内容 */
  function conditionHasContent(value: RuleCondition): boolean {
    switch (value.type) {
      case "prompt": return value.value.trim() !== "";
      case "tag": return value.tags.length > 0;
      case "group": return value.groupId !== null;
      case "artist": return value.artists.length > 0;
      case "note": return value.operator === "contains" && value.value.trim() !== "";
      case "fileText": return value.value.trim() !== "";
      case "generationText": return value.value.trim() !== "";
      default: return false;
    }
  }

  function onTypeChange(event: Event): void {
    const select = event.currentTarget as HTMLSelectElement;
    const nextType = select.value as RuleCondition["type"];
    if (nextType === condition.type) return;
    if (
      conditionHasContent(condition) &&
      !window.confirm("切换检查内容会清空这个条件里已填写的值，确定切换吗？")
    ) {
      // 用户取消：把 select 显示值拨回原类型
      select.value = condition.type;
      return;
    }
    onreplace(defaultRuleCondition(nextType));
  }

  function onRemoveClick(): void {
    if (
      conditionHasContent(condition) &&
      !window.confirm("这个条件已填写内容，确定删除吗？")
    ) {
      return;
    }
    onremove();
  }
</script>

<article class="condition-card">
  <div class="condition-head">
    <label>
      <span>检查内容</span>
      <select value={condition.type} onchange={onTypeChange}>
        {#each conditionTypes as item}
          <option value={item.value}>{item.label}</option>
        {/each}
      </select>
    </label>
    <button type="button" class="icon-btn" title="删除条件" aria-label="删除条件" onclick={onRemoveClick}>
      <X size={16} />
    </button>
  </div>

  <div class="condition-body">
    {#if condition.type === "prompt"}
      <div class="field-row">
        <label><span>提示词范围</span>
          <select value={condition.scope} onchange={event => patch({ scope: (event.currentTarget as HTMLSelectElement).value })}>
            <option value="positive">正向提示词</option>
            <option value="character">角色提示词</option>
            <option value="negative">负向提示词</option>
            <option value="positiveAndCharacter">正向＋角色提示词</option>
            <option value="all">所有提示词</option>
          </select>
        </label>
        <label><span>匹配方式</span>
          <select value={condition.operator} onchange={event => patch({ operator: (event.currentTarget as HTMLSelectElement).value })}>
            <option value="containsAll">包含全部指定提示词</option>
            <option value="containsAny">包含任意指定提示词</option>
            <option value="containsNone">不包含任何指定提示词</option>
            <option value="textContains">整段文本包含</option>
            <option value="textEquals">整段文本完全一致</option>
            <option value="regex">高级正则表达式</option>
          </select>
        </label>
      </div>
      <label class="wide"><span>{condition.operator.startsWith("contains") ? "提示词（半角/全角逗号或换行分隔）" : "文本"}</span>
        <textarea rows="2" value={condition.value} placeholder="输入要检查的内容" oninput={event => patch({ value: text(event) })}></textarea>
      </label>
      {#if ["textContains", "textEquals", "regex"].includes(condition.operator)}
        <label class="check"><input type="checkbox" checked={condition.caseSensitive} onchange={event => patch({ caseSensitive: (event.currentTarget as HTMLInputElement).checked })} />区分大小写</label>
      {/if}
    {:else if condition.type === "tag"}
      <label><span>匹配方式</span>
        <select value={condition.operator} onchange={event => patch({ operator: (event.currentTarget as HTMLSelectElement).value })}>
          <option value="hasAll">拥有全部 Tag</option><option value="hasAny">拥有任意 Tag</option>
          <option value="hasNone">不拥有这些 Tag</option><option value="isEmpty">没有任何 Tag</option>
        </select>
      </label>
      {#if condition.operator !== "isEmpty"}
        <label class="wide"><span>Tag（半角/全角逗号或换行分隔，名称精确匹配）</span>
          <textarea rows="2" value={condition.tags.join(", ")} oninput={event => patch({ tags: splitValues(text(event)) })}></textarea>
        </label>
      {/if}
    {:else if condition.type === "group"}
      <div class="field-row">
        <label><span>匹配方式</span>
          <select value={condition.operator} onchange={event => patch({ operator: (event.currentTarget as HTMLSelectElement).value, groupId: (event.currentTarget as HTMLSelectElement).value === "isEmpty" ? null : condition.groupId })}>
            <option value="is">属于分组</option><option value="isNot">不属于分组</option><option value="isEmpty">尚未分组</option>
          </select>
        </label>
        {#if condition.operator !== "isEmpty"}
          <label><span>分组</span>
            <select value={condition.groupId ?? ""} onchange={event => patch({ groupId: Number((event.currentTarget as HTMLSelectElement).value) || null })}>
              <option value="">请选择分组</option>{#each groups as group}<option value={group.id}>{group.name}</option>{/each}
            </select>
          </label>
        {/if}
      </div>
    {:else if condition.type === "artist"}
      <label><span>匹配方式</span>
        <select value={condition.operator} onchange={event => patch({ operator: (event.currentTarget as HTMLSelectElement).value })}>
          <option value="containsAny">包含任意指定画师</option><option value="containsNone">不包含指定画师</option>
          <option value="isSingle">只有一位画师</option><option value="isMultiple">有多位画师</option><option value="isEmpty">没有画师</option>
        </select>
      </label>
      {#if ["containsAny", "containsNone"].includes(condition.operator)}
        <label class="wide"><span>画师名（可省略 artist:，半角/全角逗号或换行分隔）</span>
          <textarea rows="2" value={condition.artists.join(", ")} oninput={event => patch({ artists: splitValues(text(event)) })}></textarea>
        </label>
      {/if}
    {:else if condition.type === "note"}
      <label><span>匹配方式</span><select value={condition.operator} onchange={event => patch({ operator: (event.currentTarget as HTMLSelectElement).value })}><option value="contains">备注包含</option><option value="isEmpty">备注为空</option></select></label>
      {#if condition.operator === "contains"}
        <label class="wide"><span>内容</span><input value={condition.value} oninput={event => patch({ value: text(event) })} /></label>
        <label class="check"><input type="checkbox" checked={condition.caseSensitive} onchange={event => patch({ caseSensitive: (event.currentTarget as HTMLInputElement).checked })} />区分大小写</label>
      {/if}
    {:else if condition.type === "fileText"}
      <div class="field-row">
        <label><span>字段</span><select value={condition.field} onchange={event => patch({ field: (event.currentTarget as HTMLSelectElement).value })}><option value="fileName">文件名</option><option value="originalPath">原路径</option><option value="importSource">导入来源路径</option></select></label>
        <label><span>匹配方式</span><select value={condition.operator} onchange={event => patch({ operator: (event.currentTarget as HTMLSelectElement).value })}><option value="contains">包含</option><option value="equals">完全一致</option><option value="regex">正则表达式</option></select></label>
      </div>
      <label class="wide"><span>内容</span><input value={condition.value} oninput={event => patch({ value: text(event) })} /></label>
      <label class="check"><input type="checkbox" checked={condition.caseSensitive} onchange={event => patch({ caseSensitive: (event.currentTarget as HTMLInputElement).checked })} />区分大小写</label>
    {:else if condition.type === "fileSize"}
      <RuleNumericEditor comparison={condition.comparison} unit="字节" onchange={comparison => patch({ comparison })} />
      <p class="hint">1 MB = 1,048,576 字节。</p>
    {:else if condition.type === "sourceType"}
      <div class="field-row">
        <label><span>判断</span><select value={condition.negate ? "not" : "is"} onchange={event => patch({ negate: (event.currentTarget as HTMLSelectElement).value === "not" })}><option value="is">导入自</option><option value="not">不是导入自</option></select></label>
        <label><span>来源</span><select value={condition.sourceType} onchange={event => patch({ sourceType: (event.currentTarget as HTMLSelectElement).value })}><option value="folder">文件夹／单张 PNG</option><option value="archive">压缩包</option></select></label>
      </div>
    {:else if condition.type === "vibe"}
      <label><span>匹配方式</span><select value={condition.operator} onchange={event => patch({ operator: (event.currentTarget as HTMLSelectElement).value, comparison: (event.currentTarget as HTMLSelectElement).value === "count" ? (condition.comparison ?? defaultComparison()) : null })}><option value="hasAny">存在 VIBE</option><option value="hasNone">不存在 VIBE</option><option value="count">VIBE 数量</option></select></label>
      {#if condition.operator === "count"}<RuleNumericEditor comparison={condition.comparison ?? defaultComparison()} unit="个" onchange={comparison => patch({ comparison })} />{/if}
    {:else if condition.type === "metadata"}
      <label><span>元数据状态</span><select value={condition.parsed ? "parsed" : "failed"} onchange={event => patch({ parsed: (event.currentTarget as HTMLSelectElement).value === "parsed" })}><option value="parsed">解析成功</option><option value="failed">解析失败</option></select></label>
    {:else if condition.type === "imageDimension"}
      <label><span>字段</span><select value={condition.field} onchange={event => patch({ field: (event.currentTarget as HTMLSelectElement).value })}><option value="width">宽度</option><option value="height">高度</option><option value="aspectRatio">宽高比（宽 ÷ 高）</option></select></label>
      <RuleNumericEditor comparison={condition.comparison} unit={condition.field === "aspectRatio" ? "" : "px"} onchange={comparison => patch({ comparison })} />
    {:else if condition.type === "orientation"}
      <div class="field-row"><label><span>判断</span><select value={condition.negate ? "not" : "is"} onchange={event => patch({ negate: (event.currentTarget as HTMLSelectElement).value === "not" })}><option value="is">构图是</option><option value="not">构图不是</option></select></label><label><span>构图</span><select value={condition.orientation} onchange={event => patch({ orientation: (event.currentTarget as HTMLSelectElement).value })}><option value="landscape">横图</option><option value="portrait">竖图</option><option value="square">正方形</option></select></label></div>
    {:else if condition.type === "generationText"}
      <div class="field-row"><label><span>字段</span><select value={condition.field} onchange={event => patch({ field: (event.currentTarget as HTMLSelectElement).value })}><option value="model">模型</option><option value="sampler">采样器</option><option value="noiseSchedule">噪声调度</option><option value="seed">种子</option></select></label><label><span>匹配方式</span><select value={condition.operator} onchange={event => patch({ operator: (event.currentTarget as HTMLSelectElement).value })}><option value="contains">包含</option><option value="equals">完全一致</option><option value="regex">正则表达式</option></select></label></div>
      <label class="wide"><span>内容</span><input value={condition.value} oninput={event => patch({ value: text(event) })} /></label>
      <label class="check"><input type="checkbox" checked={condition.caseSensitive} onchange={event => patch({ caseSensitive: (event.currentTarget as HTMLInputElement).checked })} />区分大小写</label>
    {:else if condition.type === "generationNumber"}
      <label><span>字段</span><select value={condition.field} onchange={event => patch({ field: (event.currentTarget as HTMLSelectElement).value })}><option value="steps">步数</option><option value="scale">Prompt Guidance</option><option value="cfgRescale">CFG Rescale</option></select></label>
      <RuleNumericEditor comparison={condition.comparison} onchange={comparison => patch({ comparison })} />
    {/if}
  </div>
</article>

<style>
  .condition-card { padding: 12px; border: 1px solid var(--border); border-radius: var(--radius-s); background: var(--surface); }
  .condition-head { display: flex; align-items: end; justify-content: space-between; gap: 10px; }
  .condition-body { display: grid; gap: 10px; margin-top: 10px; }
  .field-row { display: flex; flex-wrap: wrap; gap: 10px; }
  label { min-width: 170px; display: grid; gap: 4px; }
  label > span { color: var(--text-3); font-size: var(--font-xs); }
  .wide { width: 100%; }
  input, textarea { min-height: 34px; padding: 6px 9px; border: 1px solid var(--border); border-radius: var(--radius-s); background: var(--surface); color: var(--text); font: inherit; }
  textarea { resize: vertical; line-height: 1.5; }
  .icon-btn { width: 32px; height: 32px; display: grid; place-items: center; flex: none; border: 0; border-radius: 7px; background: transparent; color: var(--text-3); }
  .icon-btn:hover { background: var(--danger-soft, var(--surface-2)); color: var(--danger, #c53d4a); }
  .check { min-width: 0; display: flex; align-items: center; gap: 7px; color: var(--text-2); font-size: var(--font-sm); }
  .check input { min-height: 0; }
  .hint { color: var(--text-3); font-size: var(--font-xs); }
</style>
