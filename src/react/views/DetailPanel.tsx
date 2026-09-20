import { ChevronsLeft, ChevronsRight, Copy } from "lucide-react";
import { useRows } from "../state/library";
import { useWorkspace } from "../state/workspace";
import { notify } from "../state/notices";
import { Button, Hint } from "../ui/controls";
import { Thumbnail } from "../ui/Thumbnail";
import { errorText } from "../../lib/utils/format";
import { rowFileName, rowResolution } from "../../lib/utils/row-display";

export function DetailPanel() {
  const row = useRows(state => state.activeRow);
  const open = useWorkspace(state => state.detailOpen);
  const setOpen = useWorkspace(state => state.setDetailOpen);
  const copy = async (value: string) => {
    try { await navigator.clipboard.writeText(value); notify("已复制"); }
    catch (error) { notify(`复制失败：${errorText(error)}`, "error"); }
  };
  return <aside className="r-detail" data-open={open}>
    {!open ? <button type="button" className="r-detail-strip" aria-label="展开详情面板" onClick={() => setOpen(true)}><ChevronsLeft size={14} /></button> : <div className="r-detail-inner">
      <header className="r-detail-header"><div><h3 title={row ? rowFileName(row) ?? "详情" : "详情"}>{row ? rowFileName(row) : "详情"}</h3>{row && <small>第 {row.sourceOrdinal} 行 · {rowResolution(row)}</small>}</div><Hint text="收起详情面板"><Button variant="ghost" size="icon" aria-label="收起详情面板" onClick={() => setOpen(false)}><ChevronsRight size={15} /></Button></Hint></header>
      {!row ? <div className="r-detail-empty">点击图片或行查看详情</div> : <div className="r-detail-scroll" key={row.id}>
        <Thumbnail rowId={row.id} detail alt={rowFileName(row) ?? "图片预览"} className="r-detail-preview" />
        <section><h4>生成参数</h4><dl className="r-parameters">{[
          ["模型", row.generationModel], ["采样器", row.generationSampler], ["步数", row.generationSteps], ["Seed", row.generationSeed], ["提示词相关性", row.generationScale],
        ].map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value ?? "—"}</dd></div>)}</dl></section>
        {([ ["正向提示词", row.positivePrompt], ["角色提示词", row.characterPrompt], ["负向提示词", row.negativePrompt], ["备注", row.note] ] as const).map(([label, value]) => <section key={label}><div className="r-section-heading"><h4>{label}</h4><Hint text={`复制${label}`}><Button variant="ghost" size="icon" disabled={!value} aria-label={`复制${label}`} onClick={() => void copy(value ?? "")}><Copy size={13} /></Button></Hint></div><p className={value ? "r-prompt" : "r-muted"}>{value || "暂无内容"}</p></section>)}
        <section><h4>Tag</h4><div className="r-detail-tags">{row.tags.length ? row.tags.map(tag => <span key={tag}>{tag}</span>) : <p className="r-muted">暂无 Tag</p>}</div></section>
      </div>}
    </div>}
  </aside>;
}
