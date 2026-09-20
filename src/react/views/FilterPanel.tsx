import { useEffect, useId, useState, type ReactNode } from "react";
import { listGroups, listDistinctArtists, type GroupSummary, type FilterNumericComparison, type LibraryFilter } from "../../lib/api";
import { buildFilters, hydrateFilterDraft, type FilterDraft } from "../state/filter-draft";
import { setQuery, useLibrary, useRows } from "../state/library";
import { errorText } from "../../lib/utils/format";
import { Button, Checkbox, Input, Modal, Select } from "../ui/controls";

function Section({ title, hint, children }: { title: string; hint: string; children: ReactNode }) {
  return <section className="r-filter-section"><div><h4>{title}</h4><p>{hint}</p></div>{children}</section>;
}

export function NumericEditor({ value, onChange, label, step = "any" }: { value: FilterNumericComparison; onChange: (value: FilterNumericComparison) => void; label: string; step?: string }) {
  return <div className="r-numeric-row"><Select label={`${label}比较方式`} value={value.operator} onChange={operator => onChange({ ...value, operator })} options={[["equal", "等于"], ["notEqual", "不等于"], ["greaterThan", "大于"], ["greaterOrEqual", "大于等于"], ["lessThan", "小于"], ["lessOrEqual", "小于等于"], ["between", "介于"]]} />
    <Input type="number" min={0} step={step} aria-label={`${label}数值`} value={Number.isFinite(value.value) ? value.value : ""} onChange={event => onChange({ ...value, value: event.currentTarget.valueAsNumber })} />
    {value.operator === "between" && <Input type="number" min={0} step={step} placeholder="到" aria-label={`${label}上限`} value={value.secondValue !== null && Number.isFinite(value.secondValue) ? value.secondValue : ""} onChange={event => onChange({ ...value, secondValue: event.currentTarget.valueAsNumber })} />}
  </div>;
}

export function FilterPanel({ onClose, initialFilters, onApply }: { onClose: () => void; initialFilters?: LibraryFilter[]; onApply?: (filters: LibraryFilter[]) => void }) {
  const [draft, setDraft] = useState(() => hydrateFilterDraft(initialFilters ?? useRows.getState().query.filters));
  const [error, setError] = useState<string | null>(null);
  const [lookupError, setLookupError] = useState<string | null>(null);
  const [groups, setGroups] = useState<GroupSummary[]>([]);
  const [artists, setArtists] = useState<string[]>([]);
  const tags = useLibrary(state => state.tags);
  const artistListId = useId();
  const update = <K extends keyof FilterDraft>(key: K, value: FilterDraft[K]) => setDraft(previous => ({ ...previous, [key]: value }));
  useEffect(() => {
    let disposed = false;
    void Promise.all([listGroups(), listDistinctArtists()]).then(([groupList, artistList]) => { if (!disposed) { setGroups(groupList); setArtists(artistList); } }).catch(failure => { if (!disposed) setLookupError(`候选列表加载失败：${errorText(failure)}`); });
    return () => { disposed = true; };
  }, []);
  const applyFilters = (filters: LibraryFilter[]) => { if (onApply) onApply(filters); else setQuery({ filters }); };
  const apply = () => { try { applyFilters(buildFilters(draft)); onClose(); } catch (failure) { setError(errorText(failure)); } };
  return <Modal open onClose={onClose} title="过滤" description="图片需同时满足所有已选择的条件" width={520} footer={<><Button variant="ghost" className="r-footer-summary" onClick={() => { applyFilters([]); onClose(); }}>清除过滤</Button><Button onClick={onClose}>取消</Button><Button variant="primary" onClick={apply}>应用过滤</Button></>}>
    <div className="r-filter-sections">
      <Section title="Tag" hint="按资料库 Tag 过滤"><Select label="Tag 条件" value={draft.tagMode} onChange={value => update("tagMode", value)} options={[["any", "任何 Tag 状态"], ["hasAll", "拥有全部所选 Tag"], ["hasAny", "拥有任意所选 Tag"], ["hasNone", "不拥有所选 Tag"], ["isEmpty", "没有任何 Tag"]]} />
        {!["any", "isEmpty"].includes(draft.tagMode) && <div className="r-filter-choices"><Input aria-label="搜索过滤 Tag" placeholder="搜索 Tag" value={draft.tagSearch} onChange={event => update("tagSearch", event.target.value)} /><div>
          {tags.filter(tag => tag.name.toLocaleLowerCase().includes(draft.tagSearch.trim().toLocaleLowerCase())).map(tag => <label className="r-tag-pick" key={tag.name}><Checkbox checked={draft.tagValues.includes(tag.name)} onCheckedChange={checked => update("tagValues", checked ? [...draft.tagValues, tag.name] : draft.tagValues.filter(name => name !== tag.name))} /><span>{tag.name}</span><small>{tag.rowCount}</small></label>)}
        </div></div>}
      </Section>
      <Section title="分组" hint="按图片所属分组过滤"><div className="r-inline-fields"><Select label="分组条件" value={draft.groupMode} onChange={value => update("groupMode", value)} options={[["any", "任何分组状态"], ["is", "属于"], ["isNot", "不属于"], ["isEmpty", "尚未分组"]]} />
        {["is", "isNot"].includes(draft.groupMode) && <Select label="选择分组" value={draft.groupId === null ? "none" : String(draft.groupId)} onChange={value => update("groupId", value === "none" ? null : Number(value))} options={[["none", "选择分组"], ...groups.map(group => [String(group.id), group.name] as const)]} />}
      </div></Section>
      <Section title="画师" hint="按画师名称或画师数量过滤"><Select label="画师条件" value={draft.artistMode} onChange={value => update("artistMode", value)} options={[["any", "任何画师状态"], ["containsAny", "包含任意指定画师"], ["containsNone", "不包含指定画师"], ["isSingle", "单画师"], ["isMultiple", "多画师"], ["isEmpty", "没有画师"]]} />
        {["containsAny", "containsNone"].includes(draft.artistMode) && <><Input list={artistListId} aria-label="画师名称" placeholder="输入画师名；多个名称用逗号分隔" value={draft.artistText} onChange={event => update("artistText", event.target.value)} /><datalist id={artistListId}>{artists.map(artist => <option key={artist} value={artist} />)}</datalist></>}
      </Section>
      <Section title="提示词" hint="同时查找正面、角色与负面提示词"><Select label="提示词条件" value={draft.promptMode} onChange={value => update("promptMode", value)} options={[["any", "任何提示词状态"], ["containsAll", "包含全部输入内容"], ["containsAny", "包含任意输入内容"], ["containsNone", "均不包含输入内容"], ["isEmpty", "没有任何提示词"]]} />
        {!["any", "isEmpty"].includes(draft.promptMode) && <Input aria-label="过滤提示词" placeholder="例如：girl, long hair；用逗号分隔" value={draft.promptText} onChange={event => update("promptText", event.target.value)} />}
      </Section>
      <Section title="VIBE" hint="按 NovelAI VIBE 引用过滤"><Select label="VIBE 条件" value={draft.vibeMode} onChange={value => update("vibeMode", value)} options={[["any", "任何 VIBE 状态"], ["hasAny", "存在 VIBE"], ["hasNone", "不存在 VIBE"], ["count", "按 VIBE 数量"]]} />
        {draft.vibeMode === "count" && <NumericEditor label="VIBE" value={draft.vibeComparison} onChange={value => update("vibeComparison", value)} step="1" />}
      </Section>
      <Section title="备注" hint="按备注是否存在或包含内容过滤"><Select label="备注条件" value={draft.noteMode} onChange={value => update("noteMode", value)} options={[["any", "任何备注状态"], ["contains", "备注包含"], ["isNotEmpty", "有备注"], ["isEmpty", "无备注"]]} />
        {draft.noteMode === "contains" && <Input aria-label="备注过滤内容" placeholder="输入要查找的备注文字" value={draft.noteText} onChange={event => update("noteText", event.target.value)} />}
      </Section>
      <div className="r-filter-pair"><Section title="元数据" hint="NovelAI 元数据解析状态"><Select label="元数据条件" value={draft.metadataMode} onChange={value => update("metadataMode", value)} options={[["any", "任何状态"], ["parsed", "解析成功"], ["failed", "解析失败"]]} /></Section>
      <Section title="构图" hint="根据图片宽高判断"><Select label="构图条件" value={draft.orientation} onChange={value => update("orientation", value)} options={[["any", "任何构图"], ["landscape", "横图"], ["portrait", "竖图"], ["square", "正方形"]]} /></Section></div>
      <Section title="图片尺寸" hint="按宽度、高度或宽高比过滤"><Select label="图片尺寸条件" value={draft.dimensionField} onChange={value => update("dimensionField", value)} options={[["any", "不限尺寸"], ["width", "宽度"], ["height", "高度"], ["aspectRatio", "宽高比"]]} />
        {draft.dimensionField !== "any" && <NumericEditor label="图片尺寸" value={draft.dimensionComparison} onChange={value => update("dimensionComparison", value)} step={draft.dimensionField === "aspectRatio" ? "0.01" : "1"} />}
      </Section>
      <Section title="生成参数（文字）" hint="模型、采样器、噪声调度或种子"><div className="r-inline-fields"><Select label="生成文字参数" value={draft.generationTextField} onChange={value => update("generationTextField", value)} options={[["any", "不限文字参数"], ["model", "模型"], ["sampler", "采样器"], ["noiseSchedule", "噪声调度"], ["seed", "种子"]]} />
        {draft.generationTextField !== "any" && <Select label="生成文字匹配方式" value={draft.generationTextOperator} onChange={value => update("generationTextOperator", value)} options={[["contains", "包含"], ["equals", "完全一致"]]} />}</div>
        {draft.generationTextField !== "any" && <Input aria-label="生成文字匹配内容" placeholder="输入要匹配的内容" value={draft.generationTextValue} onChange={event => update("generationTextValue", event.target.value)} />}
      </Section>
      <Section title="生成参数（数值）" hint="步数、Prompt Guidance 或 CFG Rescale"><Select label="生成数值参数" value={draft.generationNumberField} onChange={value => update("generationNumberField", value)} options={[["any", "不限数值参数"], ["steps", "步数"], ["scale", "Prompt Guidance"], ["cfgRescale", "CFG Rescale"]]} />
        {draft.generationNumberField !== "any" && <NumericEditor label="生成参数" value={draft.generationNumberComparison} onChange={value => update("generationNumberComparison", value)} />}
      </Section>
    </div>{lookupError && <p className="r-field-error" role="alert">{lookupError}</p>}{error && <p className="r-field-error" role="alert">{error}</p>}
  </Modal>;
}
