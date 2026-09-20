import { useEffect, useRef, useState, type ReactNode } from "react";
import { ArrowDown, ArrowUp, Check, RefreshCw, X } from "lucide-react";
import { defaultComparison, defaultRuleCondition, type GroupSummary, type NumericComparison, type RuleAction, type RuleCondition, type TagSummary } from "../../../lib/api";
import { defaultAction } from "../../../lib/features/automation/rule-defaults";
import { splitListText } from "../../../lib/utils/list-text";
import { Button, Checkbox, Input, Select, Textarea } from "../../ui/controls";

type Options = readonly (readonly [string, string])[];
export function RuleSelect({ label, value, options, onChange, disabled }: { label: string; value: string | number; options: Options; onChange: (value: string) => void; disabled?: boolean }) {
  return <label className="automation-field"><span>{label}</span><Select label={label} value={String(value) || "__empty__"} options={options.map(([key, text]) => [key || "__empty__", text] as const)} onChange={next => onChange(next === "__empty__" ? "" : next)} disabled={disabled} /></label>;
}
export function RuleCheck({ children, value, onChange }: { children: ReactNode; value: boolean; onChange: (value: boolean) => void }) {
  return <label className="automation-check"><Checkbox checked={value} onCheckedChange={next => onChange(next === true)} />{children}</label>;
}
function TextField({ label = "内容", value, onChange, multiline = false, placeholder }: { label?: string; value: string; onChange: (value: string) => void; multiline?: boolean; placeholder?: string }) {
  return <label className="automation-field automation-wide"><span>{label}</span>{multiline ? <Textarea rows={2} value={value} placeholder={placeholder} onChange={event => onChange(event.target.value)} /> : <Input value={value} placeholder={placeholder} onChange={event => onChange(event.target.value)} />}</label>;
}
function ListField({ label, values, onChange }: { label: string; values: string[]; onChange: (values: string[]) => void }) {
  const [text, setText] = useState(values.join(", "));
  const focused = useRef(false);
  const serialized = JSON.stringify(values);
  useEffect(() => { if (!focused.current) setText((JSON.parse(serialized) as string[]).join(", ")); }, [serialized]);
  return <label className="automation-field automation-wide"><span>{label}</span><Textarea rows={2} value={text} onFocus={() => { focused.current = true; }} onBlur={() => { focused.current = false; setText(values.join(", ")); }} onChange={event => { setText(event.target.value); onChange(splitListText(event.target.value)); }} /></label>;
}
export function RuleNumericEditor({ comparison, unit = "", onChange }: { comparison: NumericComparison; unit?: string; onChange: (value: NumericComparison) => void }) {
  return <div className="automation-numeric"><RuleSelect label="比较方式" value={comparison.operator} options={[["equal", "等于"], ["notEqual", "不等于"], ["greaterThan", "大于"], ["greaterOrEqual", "大于或等于"], ["lessThan", "小于"], ["lessOrEqual", "小于或等于"], ["between", "介于（含边界）"]]} onChange={operator => onChange({ ...comparison, operator: operator as NumericComparison["operator"], secondValue: operator === "between" ? comparison.secondValue ?? comparison.value : null })} />
    <label className="automation-field"><span>{comparison.operator === "between" ? "起始值" : "数值"}</span><Input type="number" step="any" value={comparison.value} onChange={event => onChange({ ...comparison, value: Number(event.target.value) || 0 })} /></label>
    {comparison.operator === "between" && <label className="automation-field"><span>结束值</span><Input type="number" step="any" value={comparison.secondValue ?? comparison.value} onChange={event => onChange({ ...comparison, secondValue: Number(event.target.value) || 0 })} /></label>}{unit && <span>{unit}</span>}
  </div>;
}

const conditionTypes: Options = [["prompt", "提示词"], ["tag", "Tag"], ["group", "分组"], ["artist", "画师"], ["note", "备注"], ["fileText", "文件名或路径"], ["fileSize", "文件大小"], ["sourceType", "导入类型"], ["vibe", "VIBE"], ["metadata", "元数据状态"], ["imageDimension", "图片尺寸或比例"], ["orientation", "横竖构图"], ["generationText", "模型、采样器或种子"], ["generationNumber", "生成数值参数"]];
const promptFields: Options = [["positive", "正向提示词"], ["character", "角色提示词"], ["negative", "负向提示词"]];
const textOperators: Options = [["contains", "包含"], ["equals", "完全一致"], ["regex", "正则表达式"]];

export function RuleConditionEditor({ condition: c, groups, onReplace, onRemove }: { condition: RuleCondition; groups: GroupSummary[]; onReplace: (condition: RuleCondition) => void; onRemove: () => void }) {
  const patch = (values: Record<string, unknown>) => onReplace({ ...c, ...values } as RuleCondition);
  const filled = JSON.stringify(c) !== JSON.stringify(defaultRuleCondition(c.type));
  const match = (options: Options) => <RuleSelect label="匹配方式" value={"operator" in c ? c.operator : ""} options={options} onChange={operator => patch({ operator })} />;
  const caseSensitive = "caseSensitive" in c && <RuleCheck value={c.caseSensitive} onChange={value => patch({ caseSensitive: value })}>区分大小写</RuleCheck>;
  let body: ReactNode;
  switch (c.type) {
    case "prompt": body = <><div className="automation-field-row"><RuleSelect label="提示词范围" value={c.scope} options={[...promptFields, ["positiveAndCharacter", "正向＋角色提示词"], ["all", "所有提示词"]]} onChange={scope => patch({ scope })} />{match([["containsAll", "包含全部指定提示词"], ["containsAny", "包含任意指定提示词"], ["containsNone", "不包含任何指定提示词"], ["textContains", "整段文本包含"], ["textEquals", "整段文本完全一致"], ["regex", "高级正则表达式"]])}</div><TextField label={c.operator.startsWith("contains") ? "提示词（半角/全角逗号或换行分隔）" : "文本"} value={c.value} multiline onChange={value => patch({ value })} />{["textContains", "textEquals", "regex"].includes(c.operator) && caseSensitive}</>; break;
    case "tag": body = <>{match([["hasAll", "拥有全部 Tag"], ["hasAny", "拥有任意 Tag"], ["hasNone", "不拥有这些 Tag"], ["isEmpty", "没有任何 Tag"]])}{c.operator !== "isEmpty" && <ListField label="Tag（逗号或换行分隔，名称精确匹配）" values={c.tags} onChange={tags => patch({ tags })} />}</>; break;
    case "group": body = <div className="automation-field-row"><RuleSelect label="匹配方式" value={c.operator} options={[["is", "属于分组"], ["isNot", "不属于分组"], ["isEmpty", "尚未分组"]]} onChange={operator => patch({ operator, groupId: operator === "isEmpty" ? null : c.groupId })} />{c.operator !== "isEmpty" && <RuleSelect label="分组" value={c.groupId ?? ""} options={[["", "请选择分组"], ...groups.map(group => [String(group.id), group.name] as const)]} onChange={value => patch({ groupId: Number(value) || null })} />}</div>; break;
    case "artist": body = <>{match([["containsAny", "包含任意指定画师"], ["containsNone", "不包含指定画师"], ["isSingle", "只有一位画师"], ["isMultiple", "有多位画师"], ["isEmpty", "没有画师"]])}{["containsAny", "containsNone"].includes(c.operator) && <ListField label="画师名（可省略 artist:，逗号或换行分隔）" values={c.artists} onChange={artists => patch({ artists })} />}</>; break;
    case "note": body = <>{match([["contains", "包含内容"], ["isEmpty", "备注为空"]])}{c.operator === "contains" && <><TextField value={c.value} onChange={value => patch({ value })} />{caseSensitive}</>}</>; break;
    case "fileText": case "generationText": body = <><div className="automation-field-row"><RuleSelect label="字段" value={c.field} options={c.type === "fileText" ? [["fileName", "文件名"], ["originalPath", "原路径"], ["importSource", "导入来源路径"]] : [["model", "模型"], ["sampler", "采样器"], ["noiseSchedule", "噪声调度"], ["seed", "种子"]]} onChange={field => patch({ field })} />{match(textOperators)}</div><TextField value={c.value} onChange={value => patch({ value })} />{caseSensitive}</>; break;
    case "fileSize": body = <><RuleNumericEditor comparison={c.comparison} unit="字节" onChange={comparison => patch({ comparison })} /><p className="automation-hint">1 MB = 1,048,576 字节。</p></>; break;
    case "sourceType": body = <div className="automation-field-row"><RuleSelect label="判断" value={c.negate ? "not" : "is"} options={[["is", "导入自"], ["not", "不是导入自"]]} onChange={value => patch({ negate: value === "not" })} /><RuleSelect label="来源" value={c.sourceType} options={[["folder", "文件夹／单张 PNG"], ["archive", "压缩包"]]} onChange={sourceType => patch({ sourceType })} /></div>; break;
    case "vibe": body = <><RuleSelect label="匹配方式" value={c.operator} options={[["hasAny", "存在 VIBE"], ["hasNone", "不存在 VIBE"], ["count", "VIBE 数量"]]} onChange={operator => patch({ operator, comparison: operator === "count" ? c.comparison ?? defaultComparison() : null })} />{c.operator === "count" && <RuleNumericEditor comparison={c.comparison ?? defaultComparison()} unit="个" onChange={comparison => patch({ comparison })} />}</>; break;
    case "metadata": body = <RuleSelect label="元数据状态" value={c.parsed ? "parsed" : "failed"} options={[["parsed", "解析成功"], ["failed", "解析失败"]]} onChange={value => patch({ parsed: value === "parsed" })} />; break;
    case "imageDimension": case "generationNumber": body = <><RuleSelect label="字段" value={c.field} options={c.type === "imageDimension" ? [["width", "宽度"], ["height", "高度"], ["aspectRatio", "宽高比（宽 ÷ 高）"]] : [["steps", "步数"], ["scale", "Prompt Guidance"], ["cfgRescale", "CFG Rescale"]]} onChange={field => patch({ field })} /><RuleNumericEditor comparison={c.comparison} unit={c.type === "imageDimension" && c.field !== "aspectRatio" ? "px" : ""} onChange={comparison => patch({ comparison })} /></>; break;
    case "orientation": body = <div className="automation-field-row"><RuleSelect label="判断" value={c.negate ? "not" : "is"} options={[["is", "构图是"], ["not", "构图不是"]]} onChange={value => patch({ negate: value === "not" })} /><RuleSelect label="构图" value={c.orientation} options={[["landscape", "横图"], ["portrait", "竖图"], ["square", "正方形"]]} onChange={orientation => patch({ orientation })} /></div>; break;
  }
  return <article className="automation-condition"><header><RuleSelect label="检查内容" value={c.type} options={conditionTypes} onChange={type => { if (type !== c.type && (!filled || window.confirm("切换检查内容会清空这个条件里已填写的值，确定切换吗？"))) onReplace(defaultRuleCondition(type as RuleCondition["type"])); }} /><Button variant="ghost" size="icon" aria-label="删除条件" onClick={() => { if (!filled || window.confirm("这个条件已填写内容，确定删除吗？")) onRemove(); }}><X size={16} /></Button></header><div className="automation-editor-fields">{body}</div></article>;
}

function RuleTagPicker({ tags, selected, loading, onChange, onRefresh }: { tags: TagSummary[]; selected: string[]; loading: boolean; onChange: (tags: string[]) => void; onRefresh: () => void | Promise<void> }) {
  const [query, setQuery] = useState("");
  const names = new Set(tags.map(tag => tag.name));
  const normalized = [...new Set(selected.flatMap(splitListText))];
  const missing = normalized.filter(name => !names.has(name));
  const allMatches = tags.filter(tag => tag.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  const toggle = (name: string) => onChange(normalized.includes(name) ? normalized.filter(item => item !== name) : [...normalized, name]);
  return <div className="automation-tag-picker"><div className="automation-picker-head"><label className="automation-field"><span>从已有 Tag 中选择</span><Input type="search" value={query} aria-label="搜索已有 Tag" placeholder="搜索 Tag" onChange={event => setQuery(event.target.value)} /></label><Button size="sm" disabled={loading} onClick={() => void onRefresh()}><RefreshCw size={14} />{loading ? "读取中…" : "刷新"}</Button></div>
    {normalized.length > 0 && <div className="automation-chips" aria-label="已选择的 Tag">{normalized.map(name => <button type="button" className={!names.has(name) ? "is-missing" : ""} key={name} onClick={() => toggle(name)} title={`移除 Tag「${name}」`}>{name}<X size={12} /></button>)}</div>}
    {missing.length > 0 && <p className="automation-warning">旧规则中有 {missing.length} 个 Tag 已不在当前 Tag 库中。保留它们仍可能在执行时被重新创建；请移除，或先在主窗口创建对应 Tag。</p>}
    <div className="automation-tag-options" aria-label="可选择的已有 Tag">{allMatches.slice(0, 80).map(tag => <button type="button" key={tag.name} aria-pressed={normalized.includes(tag.name)} onClick={() => toggle(tag.name)}><Check size={13} style={{ opacity: normalized.includes(tag.name) ? 1 : 0 }} /><span>{tag.name}</span><small>{tag.rowCount.toLocaleString("zh-CN")}</small></button>)}{allMatches.length === 0 && <p className="automation-hint">{tags.length ? "没有匹配的 Tag。" : "暂无已有 Tag，请先在主窗口的 Tag 库中创建。"}</p>}</div>{allMatches.length > 80 && <p className="automation-hint">结果较多，仅显示前 80 项；可继续输入关键词缩小范围。</p>}</div>;
}

const actionTypes: Options = [["addTags", "添加 Tag"], ["removeTags", "移除 Tag"], ["setGroup", "移入分组"], ["clearGroup", "清除分组"], ["appendPrompt", "追加提示词"], ["deletePromptTags", "删除指定提示词"], ["replacePrompt", "查找替换提示词"], ["prefixArtist", "修正 artist: 前缀"], ["setNote", "设置备注"], ["setNoteSequence", "备注自动编号"], ["appendNote", "追加备注"], ["clearNote", "清空备注"], ["stopProcessing", "停止这张图片的后续规则"]];
export function RuleActionEditor({ action: a, groups, tags, tagsLoading, onRefreshTags, onReplace, onRemove, onMove, canMoveUp, canMoveDown }: { action: RuleAction; groups: GroupSummary[]; tags: TagSummary[]; tagsLoading: boolean; onRefreshTags: () => void | Promise<void>; onReplace: (action: RuleAction) => void; onRemove: () => void; onMove: (direction: number) => void; canMoveUp: boolean; canMoveDown: boolean }) {
  const patch = (values: Record<string, unknown>) => onReplace({ ...a, ...values } as RuleAction);
  const filled = JSON.stringify(a) !== JSON.stringify(defaultAction(a.type));
  const promptField = "field" in a && <RuleSelect label="提示词字段" value={a.field} options={promptFields} onChange={field => patch({ field })} />;
  let body: ReactNode;
  switch (a.type) {
    case "addTags": case "removeTags": body = <><RuleTagPicker tags={tags} selected={a.tags} loading={tagsLoading} onChange={tags => patch({ tags })} onRefresh={onRefreshTags} /><p className="automation-hint">命中后{a.type === "addTags" ? "添加" : "移除"}所选 Tag。这里只能选择当前 Tag 库中已有的项目，避免输入错字。</p></>; break;
    case "setGroup": { const missing = !!a.groupId && !groups.some(group => group.id === a.groupId); body = <><RuleSelect label="目标分组" value={a.groupId || ""} options={[["", groups.length ? "请选择已有分组" : "暂无已有分组"], ...(missing ? [[String(a.groupId), `已不存在的分组（#${a.groupId}）`] as const] : []), ...groups.map(group => [String(group.id), group.name] as const)]} disabled={!groups.length && !a.groupId} onChange={id => patch({ groupId: Number(id) || 0 })} /><RuleCheck value={a.onlyIfUngrouped} onChange={onlyIfUngrouped => patch({ onlyIfUngrouped })}>仅处理尚未分组的图片</RuleCheck>{missing ? <p className="automation-warning">旧规则选择的分组已经不存在，请重新选择一个已有分组后再保存。</p> : !groups.length && <p className="automation-hint">暂无可选分组，请先在主窗口创建分组。</p>}</>; break; }
    case "clearGroup": body = <p className="automation-hint">移除命中图片当前所属的分组，不会删除分组本身。</p>; break;
    case "appendPrompt": case "deletePromptTags": body = <>{promptField}<TextField multiline label={a.type === "appendPrompt" ? "追加提示词（逗号或换行分隔）" : "要删除的完整提示词（逗号或换行分隔）"} value={a.value} onChange={value => patch({ value })} /></>; break;
    case "replacePrompt": body = <>{promptField}<div className="automation-field-row"><TextField label="查找" value={a.find} onChange={find => patch({ find })} /><TextField label="替换为（可留空）" value={a.replace} onChange={replace => patch({ replace })} /></div><RuleCheck value={a.caseSensitive} onChange={caseSensitive => patch({ caseSensitive })}>区分大小写</RuleCheck></>; break;
    case "prefixArtist": body = <><ListField label="需要修正的画师名（可省略 artist:，逗号或换行分隔）" values={a.artists} onChange={artists => patch({ artists })} /><p className="automation-hint">同时检查正向、角色和负向提示词；画师串只根据正向和角色提示词重算。</p></>; break;
    case "setNote": case "appendNote": body = <><TextField multiline label={a.type === "setNote" ? "新备注" : "追加内容"} value={a.value} onChange={value => patch({ value })} />{a.type === "appendNote" && <RuleSelect label="分隔符" value={a.separator} options={[["\n", "换行"], ["，", "中文逗号"], [" | ", "竖线"], [" ", "空格"]]} onChange={separator => patch({ separator })} />}</>; break;
    case "setNoteSequence": body = <><TextField label="前缀" value={a.prefix} placeholder="例如：水彩" onChange={prefix => patch({ prefix })} /><p className="automation-hint">命中图片会按“前缀 + 数字”依次编号，例如水彩1、水彩2。编号从整库已有同前缀备注的最大数字后面继续，已经是这个格式的备注不会改。</p></>; break;
    case "clearNote": body = <p className="automation-hint">清空命中图片的备注。</p>; break;
    case "stopProcessing": body = <p className="automation-hint">只让命中的图片停止，不影响同一批次中的其他图片；本规则中排在它后面的任务仍会执行。</p>; break;
  }
  return <article className="automation-action"><div className="automation-action-order" aria-label="调整任务顺序"><Button variant="ghost" size="icon" aria-label="上移任务" disabled={!canMoveUp} onClick={() => onMove(-1)}><ArrowUp size={14} /></Button><Button variant="ghost" size="icon" aria-label="下移任务" disabled={!canMoveDown} onClick={() => onMove(1)}><ArrowDown size={14} /></Button></div><div className="automation-action-content"><header><RuleSelect label="执行任务" value={a.type} options={actionTypes} onChange={type => { if (type !== a.type && (!filled || window.confirm("切换任务类型会清空这个任务里已填写的内容，确定切换吗？"))) onReplace(defaultAction(type as RuleAction["type"])); }} /><Button variant="ghost" size="icon" aria-label="删除任务" onClick={() => { if (!filled || window.confirm("这个任务已填写内容，确定删除吗？")) onRemove(); }}><X size={16} /></Button></header><div className="automation-editor-fields">{body}</div></div></article>;
}
