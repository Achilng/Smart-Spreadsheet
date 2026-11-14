import { useEffect, useState } from "react";
import { AlertTriangle, ChevronDown, ChevronRight, ListChecks, RotateCcw, Undo2, Wand2 } from "lucide-react";
import { agentChangePage, undoAgentOperation, type AgentMatchAnalysis, type AgentOperation, type AgentPreviewReport, type AgentRowChange } from "../../../lib/api/agent";
import { errorText, formatCount } from "../../../lib/utils/format";
import { Button } from "../../ui/controls";
import { notify } from "../../state/notices";
import { DiffPair, FIELD_LABELS, describeAction, describeTarget, formatTime, targetArtistStrings } from "./agent-format";

const PAGE = 40;
const ARTIST_PREVIEW = 3;
const ARTIST_CLIP = 72;

const clip = (text: string) => { const chars = Array.from(text); return chars.length > ARTIST_CLIP ? `${chars.slice(0, ARTIST_CLIP).join("")}…` : text; };

/** Target description; artist strings are long, so only a clipped few show until expanded. */
function TargetItems({ target }: { target: Record<string, unknown> | undefined }) {
  const [open, setOpen] = useState(false);
  const artists = targetArtistStrings(target);
  const collapsible = artists.length > ARTIST_PREVIEW || artists.some(item => Array.from(item).length > ARTIST_CLIP);
  const [scope, ...rest] = describeTarget(target);
  return <>
    {scope && <li>{scope}</li>}
    {artists.length > 0 && <li>
      只限这 {artists.length} 个画师串
      {collapsible && <button type="button" className="ag-link ag-inline" onClick={() => setOpen(!open)}>{open ? "收起" : "展开全部"}</button>}
      <ol className="ag-artists">
        {(open ? artists : artists.slice(0, ARTIST_PREVIEW)).map(item => <li key={item}><code>{open ? item : clip(item)}</code></li>)}
        {!open && artists.length > ARTIST_PREVIEW && <li className="ag-note">…还有 {artists.length - ARTIST_PREVIEW} 个</li>}
      </ol>
    </li>}
    {rest.map(line => <li key={line}>{line}</li>)}
  </>;
}

/** Backend remarks about the target (one per artist string); long lists stay folded. */
function TargetNotes({ notes }: { notes: string[] }) {
  const [open, setOpen] = useState(false);
  if (!notes.length) return null;
  const collapsible = notes.length > 2 || notes.some(line => Array.from(line).length > ARTIST_CLIP + 24);
  if (!collapsible) return <>{notes.map(line => <li key={line} className="ag-note">{line}</li>)}</>;
  return <li className="ag-note">
    范围说明共 {notes.length} 条
    <button type="button" className="ag-link ag-inline" onClick={() => setOpen(!open)}>{open ? "收起" : "展开查看"}</button>
    {open && <ol className="ag-notes">{notes.map(line => <li key={line}>{line}</li>)}</ol>}
  </li>;
}

function ChangeRow({ change }: { change: AgentRowChange }) {
  return <div className="ag-change">
    <div className="ag-change-id">#{change.id}</div>
    <div className="ag-change-body">
      {change.text?.filter(item => item.field !== "artists").map(item => <div key={item.field} className="ag-change-field">
        <span className="ag-field-label">{FIELD_LABELS[item.field] ?? item.field}</span>
        <DiffPair before={item.before ?? ""} after={item.after ?? ""} afterEmpty={item.after === null} />
      </div>)}
      {change.group && <div className="ag-change-meta">分组：{change.group.beforeName ?? "无"} → {change.group.afterName ?? "无"}</div>}
      {change.favorite && <div className="ag-change-meta">收藏：{change.favorite[0] ? "是" : "否"} → {change.favorite[1] ? "是" : "否"}</div>}
      {!!change.tagsAdded?.length && <div className="ag-change-meta">新增 TAG：{change.tagsAdded.join("、")}</div>}
      {!!change.tagsRemoved?.length && <div className="ag-change-meta">移除 TAG：{change.tagsRemoved.join("、")}</div>}
    </div>
  </div>;
}

/** Full, paged list of every row a preview or operation touches. */
function ChangeList({ source, id, total }: { source: "preview" | "operation"; id: string; total: number }) {
  const [open, setOpen] = useState(false);
  const [changes, setChanges] = useState<AgentRowChange[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const load = async (offset: number) => {
    setLoading(true); setError(null);
    try {
      const page = await agentChangePage(source, id, offset, PAGE);
      setChanges(previous => offset === 0 ? page.changes : [...previous, ...page.changes]);
    } catch (reason) { setError(errorText(reason)); }
    finally { setLoading(false); }
  };
  if (!total) return null;
  return <div className="ag-change-list">
    <button type="button" className="ag-link" onClick={() => { const next = !open; setOpen(next); if (next && !changes.length) void load(0); }}>
      {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}{open ? "收起完整列表" : `查看全部 ${formatCount(total)} 张的修改`}
    </button>
    {open && <div className="ag-change-items">
      {changes.map(change => <ChangeRow key={change.id} change={change} />)}
      {error && <p className="ag-error-text">{error}（预演只保留最近 60 次，过期后无法查看）</p>}
      {changes.length < total && !error && <Button size="sm" disabled={loading} onClick={() => void load(changes.length)}>{loading ? "加载中…" : `再加载 ${Math.min(PAGE, total - changes.length)} 张（已显示 ${changes.length}/${total}）`}</Button>}
    </div>}
  </div>;
}

function MatchSummary({ analysis, actions }: { analysis: AgentMatchAnalysis; actions: Record<string, unknown>[] }) {
  const [open, setOpen] = useState(false);
  const others = Object.entries(analysis.sameMatchInOtherFields ?? {}).filter(([, count]) => count > 0);
  const action = analysis.actionIndex !== undefined ? actions[analysis.actionIndex] : undefined;
  return <div className="ag-match">
    <div className="ag-match-head">
      <span>{action ? `第 ${(analysis.actionIndex ?? 0) + 1} 步` : FIELD_LABELS[analysis.field]}：在{FIELD_LABELS[analysis.field] ?? analysis.field}里命中 <b>{formatCount(analysis.rowsMatched)}</b> 张、共 {formatCount(analysis.occurrences)} 处</span>
    </div>
    <div className="ag-chips">
      <span className="ag-chip">开头 {analysis.positions.atStart}</span>
      <span className={`ag-chip${analysis.positions.inMiddle ? " is-warn" : ""}`}>中间 {analysis.positions.inMiddle}</span>
      <span className="ag-chip">结尾 {analysis.positions.atEnd}</span>
      {analysis.positions.spanningLines > 0 && <span className="ag-chip is-warn">跨行 {analysis.positions.spanningLines}</span>}
      {analysis.rowsWithMultipleMatches > 0 && <span className="ag-chip is-warn">一张多处 {analysis.rowsWithMultipleMatches} 张</span>}
      <span className={`ag-chip${analysis.distinctMatchCount > 1 ? " is-warn" : ""}`}>不同写法 {analysis.distinctMatchCount} 种</span>
      {others.map(([name, count]) => <span key={name} className="ag-chip is-warn">{FIELD_LABELS[name] ?? name}里也有 {count} 张</span>)}
    </div>
    {analysis.distinctMatches.length > 0 && <>
      <button type="button" className="ag-link" onClick={() => setOpen(!open)}>{open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}匹配到的具体文字</button>
      {open && <ul className="ag-distinct">
        {analysis.distinctMatches.map(item => <li key={item.text}><span className="ag-count">{item.count}×</span><code>{item.text}</code><span className="ag-context">例 #{item.sampleRowId}：{item.context}</span></li>)}
      </ul>}
    </>}
  </div>;
}

export function PreviewCard({ report, applied }: { report: AgentPreviewReport; applied?: AgentOperation }) {
  const fields = Object.entries(report.fieldCounts);
  return <section className="ag-card">
    <header className="ag-card-head">
      <Wand2 size={16} />
      <strong>{report.title || "修改预演"}</strong>
      <span className={`ag-badge${applied ? (applied.state === "undone" ? " is-muted" : " is-ok") : ""}`}>{applied ? (applied.state === "undone" ? "已执行，已撤销" : "已执行") : "预演 · 还没改动数据"}</span>
    </header>
    <div className="ag-stats">
      <div><span>检查</span><b>{formatCount(report.scannedRows)}</b><span>张</span></div>
      <div className="is-main"><span>将修改</span><b>{formatCount(report.changedRows)}</b><span>张</span></div>
      <div><span>不变</span><b>{formatCount(report.unchangedRows)}</b><span>张</span></div>
    </div>
    <div className="ag-params">
      <h5>参数</h5>
      <ul>
        <TargetItems target={report.target} />
        <TargetNotes notes={report.targetNotes} />
        {report.actions.map((action, index) => <li key={index}><b>第 {index + 1} 步：</b>{describeAction(action)}</li>)}
        {report.newGroups.length > 0 && <li>会新建分组：{report.newGroups.join("、")}</li>}
        {report.newTags.length > 0 && <li>会新建 TAG：{report.newTags.join("、")}</li>}
      </ul>
      {fields.length > 0 && <div className="ag-chips">{fields.map(([name, count]) => <span key={name} className="ag-chip">{FIELD_LABELS[name] ?? name} {formatCount(count)} 张</span>)}</div>}
    </div>
    {report.warnings.length > 0 && <div className="ag-warnings">{report.warnings.map(text => <p key={text}><AlertTriangle size={14} />{text}</p>)}</div>}
    {report.matchAnalysis.map((analysis, index) => <MatchSummary key={index} analysis={analysis} actions={report.actions} />)}
    {report.samples.length > 0 && <div className="ag-samples">
      <h5>修改前后样例</h5>
      {report.samples.map(sample => <div key={sample.rowId} className="ag-change">
        <div className="ag-change-id">#{sample.rowId}</div>
        <div className="ag-change-body">
          {sample.text.map(item => <div key={item.field} className="ag-change-field"><span className="ag-field-label">{FIELD_LABELS[item.field] ?? item.field}</span><DiffPair before={item.before} after={item.after} afterEmpty={item.afterEmpty} /></div>)}
          {sample.group && <div className="ag-change-meta">分组：{sample.group.before ?? "无"} → {sample.group.after ?? "无"}</div>}
          {sample.favorite && <div className="ag-change-meta">收藏：{sample.favorite[0] ? "是" : "否"} → {sample.favorite[1] ? "是" : "否"}</div>}
          {!!sample.tagsAdded?.length && <div className="ag-change-meta">新增 TAG：{sample.tagsAdded.join("、")}</div>}
          {!!sample.tagsRemoved?.length && <div className="ag-change-meta">移除 TAG：{sample.tagsRemoved.join("、")}</div>}
        </div>
      </div>)}
    </div>}
    <ChangeList source="preview" id={report.previewId} total={report.changedRows} />
  </section>;
}

export function OperationCard({ operation, onChanged, compact = false }: { operation: AgentOperation; onChanged: () => void; compact?: boolean }) {
  const [busy, setBusy] = useState(false);
  const [showDetail, setShowDetail] = useState(!compact);
  useEffect(() => { setShowDetail(!compact); }, [compact]);
  const undone = operation.state === "undone";
  const isRule = operation.kind === "rule";
  const undo = async () => {
    setBusy(true);
    try { const outcome = await undoAgentOperation(operation.id); notify(outcome.message); onChanged(); }
    catch (error) { notify(`撤销失败：${errorText(error)}`, "error"); }
    finally { setBusy(false); }
  };
  const report = operation.report;
  return <section className={`ag-card is-operation${undone ? " is-undone" : ""}`}>
    <header className="ag-card-head">
      {isRule ? <ListChecks size={16} /> : <RotateCcw size={16} />}
      <strong>{operation.title}</strong>
      <span className={`ag-badge ${undone ? "is-muted" : "is-ok"}`}>{undone ? "已撤销" : "已执行"}</span>
      <span className="ag-time">#{operation.id} · {formatTime(operation.createdAt)}</span>
    </header>
    {isRule
      ? <p className="ag-op-line">新建了自动规则「{report.rule?.name ?? ""}」，以后导入新图时会自动执行。撤销 = 删除这条规则。</p>
      : <p className="ag-op-line">修改了 <b>{formatCount(operation.changedRows)}</b> 张图片。原始图片文件没有被改动。</p>}
    {undone && operation.undoMessage && <p className="ag-op-line is-muted">{operation.undoMessage}</p>}
    {!isRule && <>
      <button type="button" className="ag-link" onClick={() => setShowDetail(!showDetail)}>{showDetail ? <ChevronDown size={14} /> : <ChevronRight size={14} />}参数</button>
      {showDetail && <div className="ag-params"><ul>
        <TargetItems target={report.target} />
        {(report.actions ?? []).map((action, index) => <li key={index}><b>第 {index + 1} 步：</b>{describeAction(action)}</li>)}
      </ul></div>}
      <ChangeList source="operation" id={String(operation.id)} total={operation.changedRows} />
    </>}
    {!undone && <div className="ag-card-actions">
      <Button size="sm" disabled={busy} onClick={() => void undo()}><Undo2 size={14} />{busy ? "撤销中…" : isRule ? "撤销（删除规则）" : "撤销这次修改"}</Button>
      {!isRule && <span className="ag-hint">撤销时，之后又被改过的图片会保留现状</span>}
    </div>}
  </section>;
}
