import { useState } from "react";
import { Button, Checkbox, Modal } from "../ui/controls";
import { executeJsonExport, useJsonExport } from "../state/export-actions";
import { errorText } from "../../lib/utils/format";

export function JsonExportDialog() {
  const request = useJsonExport(state => state.request);
  const [options, setOptions] = useState({ noteNumberNames: true, includeArtists: true, deduplicate: true });
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const close = () => { useJsonExport.setState({ request: null }); setError(null); };
  const start = async () => {
    if (!request || working) return;
    setWorking(true); setError(null);
    try { if (await executeJsonExport(request.selection, options)) close(); }
    catch (failure) { setError(errorText(failure)); }
    finally { setWorking(false); }
  };
  return <Modal open={Boolean(request)} onClose={close} title="导出智绘姬 JSON" description={`导出${request?.label ?? ""}`} busy={working} width={460}
    footer={<><Button disabled={working} onClick={close}>取消</Button><Button variant="primary" disabled={working} onClick={() => void start()}>{working ? "导出中…" : "选择位置并导出"}</Button></>}>
    {([
      ["noteNumberNames", "名称使用“备注_序号”", "例如“夏日白裙_1”；没有备注时只使用数字序号"],
      ["includeArtists", "补齐画师串", "把资料库中已有、但正向提示词里缺少的画师追加到末尾"],
      ["deduplicate", "按最终正向提示词去重", "补齐画师后再比较；重复时优先保留有备注的记录"],
    ] as const).map(([key, title, hint]) => <label key={key} className="r-option-row"><Checkbox checked={options[key]} disabled={working} onCheckedChange={value => setOptions(previous => ({ ...previous, [key]: value === true }))} /><span><strong>{title}</strong><small>{hint}</small></span></label>)}
    <p className="r-muted">执行顺序：补齐画师 → 去重 → 连续编号</p>
    {error && <p className="r-field-error" role="alert">导出失败：{error}</p>}
  </Modal>;
}
