import { useEffect, useState } from "react";
import type { CompareSample } from "../../../lib/api/compare";
import { compareFields, customPage, queryCompareCustom, type CompareField, type CompareOptions, type CustomPage } from "../../../lib/api/compare-conditions";
import { createRequestQueue } from "../../../lib/utils/request-queue";
import { formatCount, errorText } from "../../../lib/utils/format";
import { applyCompareOptions, COMPARE_PAGE_SIZE } from "../../state/compare";
import { Button, Checkbox, Modal, Select } from "../../ui/controls";

const fields = Object.entries(compareFields) as [CompareField, string][];
const previewQueue = createRequestQueue();

export function CompareConditionsEditor({ sample, initial, onClose }: { sample: CompareSample; initial: CompareOptions; onClose: () => void }) {
  const [draft, setDraft] = useState(initial);
  const [retry, setRetry] = useState(0);
  const [result, setResult] = useState<{ options: CompareOptions; page?: CustomPage; error?: string } | null>(null);
  useEffect(() => {
    let active = true;
    const timer = setTimeout(() => {
      void previewQueue(() => active, async () => {
        const groups = await queryCompareCustom(sample, draft);
        if (active) return customPage(groups, 0, COMPARE_PAGE_SIZE);
      }).then(page => {
        if (active && page) setResult({ options: draft, page });
      }).catch(cause => { if (active) setResult({ options: draft, error: errorText(cause) }); });
    }, 160);
    return () => { active = false; clearTimeout(timer); };
  }, [sample, draft, retry]);
  const preview = result?.options === draft ? result : null;
  const page = preview?.page;
  return <Modal open onClose={onClose} title="自定义对比" width={520}
    footer={<><span className="r-footer-summary" role="status">{page ? `匹配 ${formatCount(page.totalCount)} ${draft.dedupe ? "组" : "张"}` : preview?.error ? "查询失败" : "正在匹配…"}</span>
      <Button onClick={onClose}>取消</Button><Button variant="primary" disabled={!page} onClick={() => { if (page) { applyCompareOptions(draft, page); onClose(); } }}>查看结果</Button></>}>
    <div className="compare-editor">
      <section><h3>要比较什么？</h3><Select label="要比较什么？" value={draft.observe} options={fields} onChange={observe => setDraft({ ...draft, observe })} /></section>
      <section><label className="ce-field"><Checkbox checked={draft.dedupe} onCheckedChange={checked => setDraft({ ...draft, dedupe: checked === true })} />对结果去重</label>
        {draft.dedupe && <fieldset className="ce-allowed"><legend>还允许哪些条件不同？</legend>
          <p>未勾选项须与 A 一致。两边均未记录也算一致。</p>
          <div>{fields.filter(([field]) => field !== draft.observe).map(([field, label]) =>
            <label className="ce-field" key={field}><Checkbox checked={draft.allowed.includes(field)} onCheckedChange={checked =>
              setDraft({ ...draft, allowed: checked ? [...draft.allowed, field] : draft.allowed.filter(value => value !== field) })} />允许{label}不同</label>
          )}</div>
        </fieldset>}
      </section>
      {preview?.error && <div role="alert">{preview.error} <Button onClick={() => { setResult(null); setRetry(retry + 1); }}>重试</Button></div>}
    </div>
  </Modal>;
}
