import { useEffect, useState } from "react";
import { Trash2 } from "lucide-react";
import type { CompareSample, CompareSectionPage } from "../../../lib/api/compare";
import { compareFields, diffTabs, newRule, queryCompareCustom, sampleValue, startingConditions, type CompareField, type CompareOptions, type CompareRule, type DiffTab } from "../../../lib/api/compare-conditions";
import { createRequestQueue } from "../../../lib/utils/request-queue";
import { libraryFilterLabel } from "../../../lib/utils/library-filters";
import { formatCount, errorText } from "../../../lib/utils/format";
import { applyCompareOptions, COMPARE_PAGE_SIZE } from "../../state/compare";
import { refreshTags } from "../../state/library";
import { Button, Checkbox, Input, Modal, Select } from "../../ui/controls";
import { FilterPanel } from "../FilterPanel";

const fields = Object.keys(compareFields) as CompareField[];
const inspectionTabs = Object.entries(diffTabs) as [DiffTab, string][];
const previewQueue = createRequestQueue();

export function CompareConditionsEditor({ sample, initial, onClose }: { sample: CompareSample; initial: CompareOptions; onClose: () => void }) {
  const [draft, setDraft] = useState(initial);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [retry, setRetry] = useState(0);
  const { rules, filters } = draft;
  const [result, setResult] = useState<{ rules: CompareRule[]; filters: CompareOptions["filters"]; page?: CompareSectionPage; error?: string } | null>(null);
  const missingWords = draft.rules.some(rule => rule.enabled && rule.relation === "onlyWords" && !rule.words.split(/[,\n]/).some(word => word.trim()));
  const available = fields.filter(field => !draft.rules.some(rule => rule.field === field));
  const favorite = draft.filters.some(filter => filter.type === "favorite");
  useEffect(() => {
    if (missingWords) return;
    let active = true;
    const timer = setTimeout(() => {
      void previewQueue(() => active, () => queryCompareCustom(sample, { rules, filters }, 0, COMPARE_PAGE_SIZE)).then(page => {
        if (active && page) setResult({ rules, filters, page });
      }).catch(cause => { if (active) setResult({ rules, filters, error: errorText(cause) }); });
    }, 180);
    return () => { active = false; clearTimeout(timer); };
  }, [sample, rules, filters, missingWords, retry]);

  const updateRule = (field: CompareField, patch: Partial<CompareRule>) =>
    setDraft(current => ({ ...current, rules: current.rules.map(rule => rule.field === field ? { ...rule, ...patch } : rule) }));
  const preview = result?.rules === rules && result.filters === filters ? result : null;
  if (filtersOpen) return <FilterPanel initialFilters={draft.filters} onApply={filters => setDraft({ ...draft, filters })} onClose={() => setFiltersOpen(false)} />;
  return <Modal open onClose={onClose} title="调整对比条件" description="目标 B 需满足所有勾选的条件。更换 A 时会重新匹配。" width={660}
    footer={<><span className="r-footer-summary" role="status">{missingWords ? "请填写允许变化的词" : preview?.page ? `匹配 ${formatCount(preview.page.totalCount)} 张` : preview?.error ? "查询失败" : "正在匹配…"}</span>
      <Button onClick={onClose}>取消</Button><Button variant="primary" disabled={!preview?.page} onClick={() => { if (preview?.page) { applyCompareOptions(draft, preview.page); onClose(); } }}>查看结果</Button></>}>
    <div className="compare-editor">
      <section><div className="ce-heading"><h3>与参考图 A 比较</h3><Button variant="ghost" onClick={() => setDraft(startingConditions("blank"))}>从空白开始</Button></div>
        {!draft.rules.length && <p>添加要比较的字段，其余字段不限。</p>}
        {draft.rules.map(rule => {
          const prompt = ["positive", "character", "negative"].includes(rule.field);
          const value = sampleValue(sample, rule.field);
          return <div className="ce-rule" key={rule.field}>
            <label className="ce-field"><Checkbox checked={rule.enabled} onCheckedChange={checked => updateRule(rule.field, { enabled: !!checked })} />{compareFields[rule.field]}</label>
            <Select label={`${compareFields[rule.field]}的关系`} value={rule.relation} disabled={!rule.enabled} onChange={relation => updateRule(rule.field, { relation })}
              options={prompt ? [["same", "与 A 相同"], ["different", "与 A 不同"], ["onlyWords", "仅指定词有变化"]] : [["same", "与 A 相同"], ["different", "与 A 不同"]]} />
            <Button size="icon" variant="ghost" aria-label={`移除${compareFields[rule.field]}条件`} onClick={() => setDraft({ ...draft, rules: draft.rules.filter(item => item !== rule) })}><Trash2 size={15} /></Button>
            {value.length > 64 || value.includes("\n")
              ? <details className="ce-value"><summary title={value}>A · {value}</summary><p>{value}</p></details>
              : <span className="ce-value">A · {value}</span>}
            {prompt && <small>按词项比较，忽略顺序和大小写，重复次数计入。</small>}
            {rule.field === "style" && <small>沿用画风匹配规则，保留词序，忽略官方质量词。</small>}
            {rule.field === "model" && <small>按模型版本和变体比较，缺少记录的图片不参与。</small>}
            {rule.field === "vibe" && <small>按整组引用比较；不可读的引用不参与。「相同」要求两图都有引用。</small>}
            {prompt && rule.relation === "onlyWords" && <Input aria-label="允许变化的提示词" disabled={!rule.enabled} placeholder="例如 blurry，多个词用英文逗号分隔" value={rule.words} onChange={event => updateRule(rule.field, { words: event.target.value })} />}
          </div>;
        })}
        {available.length > 0 && <Select label="添加对比条件" value="add" onChange={field => { if (field !== "add") setDraft({ ...draft, rules: [...draft.rules, newRule(field)] }); }}
          options={[["add", "＋ 添加条件"], ...available.map(field => [field, compareFields[field]] as const)]} />}
      </section>
      <section><div className="ce-heading"><h3>目标 B 的筛选</h3><Button onClick={() => { void refreshTags(); setFiltersOpen(true); }}>设置筛选</Button></div>
        <label className="ce-field"><Checkbox checked={favorite} onCheckedChange={checked => setDraft({ ...draft, filters: checked ? [...draft.filters, { type: "favorite" }] : draft.filters.filter(filter => filter.type !== "favorite") })} />仅收藏图片</label>
        {draft.filters.some(filter => filter.type !== "favorite") && <p>{draft.filters.filter(filter => filter.type !== "favorite").map(filter => libraryFilterLabel(filter, [])).join(" · ")}</p>}
      </section>
      <section className="ce-heading"><h3>并排时先看</h3><Select label="并排时先看" value={draft.initialTab} onChange={initialTab => setDraft({ ...draft, initialTab })} options={inspectionTabs} /></section>
      {preview?.error && <div role="alert">{preview.error} <Button onClick={() => { setResult(null); setRetry(retry + 1); }}>重试</Button></div>}
    </div>
  </Modal>;
}
