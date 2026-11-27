import { useState } from "react";
import { AlertTriangle, Check, ChevronDown, ChevronRight, FileText, Layers, ListChecks, MonitorUp, RotateCcw, Undo2, Wand2 } from "lucide-react";
import {
  agentChangePage, revealAgentContent, showAgentChangedRows, undoAgentOperation,
  type AgentChangeGroup, type AgentContentReport, type AgentMatchAnalysis, type AgentOperation, type AgentPreviewReport, type AgentRowChange, type AgentSample,
} from "../../../lib/api/agent";
import { errorText, formatCount } from "../../../lib/utils/format";
import { Button } from "../../ui/controls";
import { notify } from "../../state/notices";
import { RowThumb, ThumbStrip, useImageViewer } from "./AgentMedia";
import { DiffPair, FIELD_LABELS, describeAction, describeTarget, formatTime, targetArtistStrings } from "./agent-format";

const PAGE = 40;
const ARTIST_PREVIEW = 3;
const ARTIST_CLIP = 72;
const FRAGMENT_CLIP = 32;
/** Change groups shown before the rest fold away. */
const GROUPS_SHOWN = 4;

const clip = (text: string, max = ARTIST_CLIP) => { const chars = Array.from(text); return chars.length > max ? `${chars.slice(0, max).join("")}…` : text; };
const quote = (text: string) => (text.trim() ? `「${clip(text.trim(), FRAGMENT_CLIP)}」` : "空白");
const tagList = (tags: string[]) => tags.map(tag => `「${tag}」`).join("");
const TAGS_NAMED = 3;
/** Repeated tags (a row may hold the same tag twice) as one entry with a count. */
const tallyTags = (tags: string[]) => {
  const counts = new Map<string, number>();
  for (const tag of tags) counts.set(tag, (counts.get(tag) ?? 0) + 1);
  return [...counts].map(([tag, count]) => ({ tag, count }));
};
const shortTags = (tags: string[]) => {
  const tally = tallyTags(tags);
  return tally.slice(0, TAGS_NAMED).map(({ tag, count }) => quote(tag) + (count > 1 ? `×${count}` : "")).join("")
    + (tally.length > TAGS_NAMED ? ` 等 ${tally.length} 个` : "");
};
const CHIPS_SHOWN = 16;
/** New groups / TAGs named in a preview before the rest are counted. */
const NEW_NAMED = 8;
const namedList = (names: string[]) => names.slice(0, NEW_NAMED).map(name => `「${clip(name, FRAGMENT_CLIP)}」`).join("")
  + (names.length > NEW_NAMED ? ` 等 ${names.length} 个` : "");

/** Plain-language lines the backend wrote for group / TAG operations and rule trial runs. */
function SummaryLines({ lines }: { lines?: string[] }) {
  const [open, setOpen] = useState(false);
  if (!lines?.length) return null;
  const shown = open ? lines : lines.slice(0, GROUPS_SHOWN);
  return <div className="ag-summary">
    <ul>{shown.map((line, index) => <li key={index}>{line}</li>)}</ul>
    {lines.length > GROUPS_SHOWN && <button type="button" className="ag-link" onClick={() => setOpen(!open)}>
      {open ? <><ChevronDown size={14} />收起</> : <><ChevronRight size={14} />还有 {lines.length - GROUPS_SHOWN} 条</>}
    </button>}
  </div>;
}

/** Plain-language parts of one change group, e.g. ["加上 TAG「双人」", "移到分组「A」"]. */
export function describeGroup(group: AgentChangeGroup): string[] {
  const parts: string[] = [];
  for (const edit of group.text ?? []) {
    const field = FIELD_LABELS[edit.field] ?? edit.field;
    const removedTags = edit.removedTags ?? [], addedTags = edit.addedTags ?? [];
    if (edit.afterEmpty) parts.push(`${field}清空`);
    else if (removedTags.length && addedTags.length) parts.push(`${field}里${shortTags(removedTags)}改成${shortTags(addedTags)}`);
    else if (removedTags.length) parts.push(`${field}删掉${shortTags(removedTags)}`);
    else if (addedTags.length) parts.push(`${field}加上${shortTags(addedTags)}`);
    else if (edit.formatOnly) parts.push(`${field}只调整写法`);
    else if (edit.removed && edit.added) parts.push(`${field}里${quote(edit.removed)}改成${quote(edit.added)}`);
    else if (edit.removed) parts.push(`${field}删掉${quote(edit.removed)}`);
    else if (edit.added) parts.push(`${field}加上${quote(edit.added)}`);
    else parts.push(`改${field}`);
  }
  if (group.tagsAdded?.length) parts.push(`加上 TAG${tagList(group.tagsAdded)}`);
  if (group.tagsRemoved?.length) parts.push(`去掉 TAG${tagList(group.tagsRemoved)}`);
  if (group.group) {
    const [before, after] = group.group;
    parts.push(after ? `移到分组「${after}」${before ? `（原来在「${before}」）` : ""}` : `移出分组「${before ?? ""}」`);
  }
  if (group.favorite) parts.push(group.favorite[1] ? "设为收藏" : "取消收藏");
  return parts;
}

/** Older reports kept only a few samples; identical ones are merged, and text edits stay one per row. */
function samplesAsGroups(samples: AgentSample[]): AgentChangeGroup[] {
  const groups = new Map<string, AgentChangeGroup>();
  for (const sample of samples) {
    const group: AgentChangeGroup = {
      count: 1, rowIds: [sample.rowId],
      text: sample.text.map(item => ({ field: item.field, removed: "", added: "", before: item.before, after: item.after, afterEmpty: item.afterEmpty })),
      group: sample.group ? [sample.group.before, sample.group.after] : undefined,
      favorite: sample.favorite, tagsAdded: sample.tagsAdded, tagsRemoved: sample.tagsRemoved,
    };
    const key = sample.text.length ? `row-${sample.rowId}` : JSON.stringify([group.group, group.favorite, group.tagsAdded, group.tagsRemoved]);
    const seen = groups.get(key);
    if (seen) { seen.count += 1; seen.rowIds.push(sample.rowId); } else groups.set(key, group);
  }
  return [...groups.values()];
}

function TagChips({ tags, kind }: { tags: string[]; kind: "del" | "ins" }) {
  const tally = tallyTags(tags);
  return <>
    {tally.slice(0, CHIPS_SHOWN).map(({ tag, count }) => <span key={`${kind}-${tag}`} className={`ag-tag is-${kind}`} title={tag}>{kind === "del" ? "−" : "+"} {clip(tag, 48)}{count > 1 && <small> ×{count}</small>}</span>)}
    {tally.length > CHIPS_SHOWN && <span className="ag-tag">还有 {tally.length - CHIPS_SHOWN} 个</span>}
  </>;
}

/**
 * What a group changes in the text. Prompt edits show the tags that go and come as chips;
 * free text (notes) and old reports show the edited excerpt. `context` (the viewer) always lists the tags.
 */
function GroupDiffs({ group, context = false }: { group: AgentChangeGroup; context?: boolean }) {
  if (!group.text?.length) return null;
  const labelled = group.text.length > 1;
  return <div className="ag-group-diffs">
    {group.text.map(edit => {
      const removedTags = edit.removedTags ?? [], addedTags = edit.addedTags ?? [];
      const tagged = removedTags.length + addedTags.length > 0;
      // The title already names up to TAGS_NAMED tags; chips are for the full list.
      const named = tallyTags(removedTags).length <= TAGS_NAMED && tallyTags(addedTags).length <= TAGS_NAMED;
      const chips = tagged && (context || !named);
      // The excerpt is the group's first row only, so tag edits leave it out.
      const excerpt = !tagged && !!(edit.before || edit.after);
      if (!chips && !excerpt && !edit.formatOnly) return null;
      return <div key={edit.field} className="ag-change-field">
        {labelled && <span className="ag-field-label">{FIELD_LABELS[edit.field] ?? edit.field}</span>}
        {chips && <div className="ag-tags"><TagChips tags={removedTags} kind="del" /><TagChips tags={addedTags} kind="ins" /></div>}
        {edit.formatOnly && !excerpt && <span className="ag-note">TAG 没有增减，只是逗号、空格或权重符号的位置变了</span>}
        {excerpt && <DiffPair before={edit.before} after={edit.after} afterEmpty={edit.afterEmpty} />}
      </div>;
    })}
  </div>;
}

type Source = { source: "preview" | "operation"; id: string };

function ChangeGroupItem({ group, index, source, single, compact }: { group: AgentChangeGroup; index: number; source: Source; single: boolean; compact: boolean }) {
  const view = useImageViewer();
  const summary = describeGroup(group).join("，");
  const open = (at: number) => view({ title: summary, ids: group.rowIds, total: group.count, index: at, detail: <><p className="ag-viewer-summary">这一组 {formatCount(group.count)} 张：{summary}</p><GroupDiffs group={group} context /></> });
  const show = async () => {
    try { await showAgentChangedRows(source.source, source.id, index); }
    catch (error) { notify(`没能在主窗口显示：${errorText(error)}`, "error"); }
  };
  // A one-row group reads best as thumbnail beside its edit.
  if (group.count === 1 && group.text?.length) return <div className="ag-group is-row">
    <RowThumb id={group.rowIds[0]} size={64} onOpen={() => open(0)} />
    <div className="ag-group-body">
      <p className="ag-group-title"><span>{summary}</span><span className="ag-group-count">#{group.rowIds[0]}</span></p>
      {!compact && <GroupDiffs group={group} />}
    </div>
  </div>;
  return <div className={`ag-group${single ? " is-single" : ""}`}>
    {!single && <p className="ag-group-title">
      <span>{summary}</span>
      <span className="ag-group-count">{formatCount(group.count)} 张</span>
      {group.count > group.rowIds.length && <button type="button" className="ag-link ag-group-show" onClick={() => void show()}>在主窗口查看</button>}
    </p>}
    {!compact && <GroupDiffs group={group} />}
    <ThumbStrip ids={group.rowIds} total={group.count} max={single ? 12 : 10} size={single ? 64 : 56} onOpen={open} />
  </div>;
}

/** What changes, grouped so identical edits show once with the images they apply to. */
function ChangeGroups({ groups, otherRows = 0, source, compact = false, legacy = false }: { groups: AgentChangeGroup[]; otherRows?: number; source: Source; compact?: boolean; legacy?: boolean }) {
  const [expanded, setExpanded] = useState(false);
  if (!groups.length) return null;
  const limit = compact ? 2 : GROUPS_SHOWN;
  const shown = expanded ? groups : groups.slice(0, limit);
  const hidden = groups.slice(shown.length);
  const hiddenRows = hidden.reduce((sum, group) => sum + group.count, 0);
  const single = groups.length === 1 && !otherRows;
  return <div className="ag-groups">
    {legacy && <p className="ag-note">这是旧版助手做的预演，只保存了 {formatCount(groups.reduce((sum, group) => sum + group.count, 0))} 张样例；完整改动请看「详细信息」。</p>}
    {single && <p className="ag-lede-change">{legacy ? "样例" : "全部"} {formatCount(groups[0].count)} 张：<b>{describeGroup(groups[0]).join("，")}</b></p>}
    {shown.map((group, index) => <ChangeGroupItem key={index} group={group} index={index} source={source} single={single} compact={compact} />)}
    {hidden.length > 0 && <button type="button" className="ag-link" onClick={() => setExpanded(true)}><ChevronRight size={14} />还有 {hidden.length} 种改法，共 {formatCount(hiddenRows)} 张</button>}
    {expanded && groups.length > limit && <button type="button" className="ag-link" onClick={() => setExpanded(false)}><ChevronDown size={14} />收起</button>}
    {otherRows > 0 && (expanded || !hidden.length) && <p className="ag-note">另有 {formatCount(otherRows)} 张的改法各不相同，没有逐组列出，可在「详细信息」的完整列表里查看。</p>}
  </div>;
}

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
  const view = useImageViewer();
  return <div className="ag-change">
    <RowThumb id={change.id} size={44} onOpen={() => view({ title: `图片 #${change.id}`, ids: [change.id], index: 0 })} />
    <div className="ag-change-body">
      {change.text?.filter(item => item.field !== "artists").map(item => <div key={item.field} className="ag-change-field">
        <span className="ag-field-label">{FIELD_LABELS[item.field] ?? item.field}</span>
        <DiffPair before={item.before ?? ""} after={item.after ?? ""} afterEmpty={item.after === null} />
      </div>)}
      {change.group && <div className="ag-change-meta">分组：{change.group.beforeName ?? "无"} → {change.group.afterName ?? "无"}</div>}
      {change.favorite && <div className="ag-change-meta">收藏：{change.favorite[0] ? "是" : "否"} → {change.favorite[1] ? "是" : "否"}</div>}
      {!!change.tagsAdded?.length && <div className="ag-change-meta">加上 TAG：{change.tagsAdded.join("、")}</div>}
      {!!change.tagsRemoved?.length && <div className="ag-change-meta">去掉 TAG：{change.tagsRemoved.join("、")}</div>}
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
      {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}{open ? "收起逐张列表" : `逐张查看全部 ${formatCount(total)} 张`}
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
      <span>{action ? `第 ${(analysis.actionIndex ?? 0) + 1} 步` : FIELD_LABELS[analysis.field]}：在{FIELD_LABELS[analysis.field] ?? analysis.field}里找到 <b>{formatCount(analysis.rowsMatched)}</b> 张、共 {formatCount(analysis.occurrences)} 处</span>
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
      <button type="button" className="ag-link" onClick={() => setOpen(!open)}>{open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}找到的具体文字</button>
      {open && <ul className="ag-distinct">
        {analysis.distinctMatches.map(item => <li key={item.text}><span className="ag-count">{item.count}×</span><code>{item.text}</code><span className="ag-context">例 #{item.sampleRowId}：{item.context}</span></li>)}
      </ul>}
    </>}
  </div>;
}

/** Scope, steps and match statistics: what the assistant asked for, folded away by default. */
function Details({ report, source, total }: { report: AgentPreviewReport; source: Source; total: number }) {
  return <div className="ag-details">
    <div className="ag-params"><ul>
      <TargetItems target={report.target} />
      <TargetNotes notes={report.targetNotes ?? []} />
      {(report.actions ?? []).map((action, index) => <li key={index}><b>第 {index + 1} 步：</b>{describeAction(action)}</li>)}
      {report.scannedRows !== undefined && <li>共检查 {formatCount(report.scannedRows)} 张，其中 {formatCount(report.unchangedRows)} 张不用改</li>}
    </ul></div>
    {(report.matchAnalysis ?? []).map((analysis, index) => <MatchSummary key={index} analysis={analysis} actions={report.actions ?? []} />)}
    <ChangeList source={source.source} id={source.id} total={total} />
  </div>;
}

function useUndo(operation: AgentOperation | undefined, onChanged: () => void) {
  const [busy, setBusy] = useState(false);
  const undo = async () => {
    if (!operation) return;
    setBusy(true);
    try { const outcome = await undoAgentOperation(operation.id); notify(outcome.message); onChanged(); }
    catch (error) { notify(`撤销失败：${errorText(error)}`, "error"); }
    finally { setBusy(false); }
  };
  return { busy, undo };
}

async function showInMain(source: Source) {
  try { const count = await showAgentChangedRows(source.source, source.id); notify(`已在主窗口显示这 ${formatCount(count)} 张图`); }
  catch (error) { notify(`没能在主窗口显示：${errorText(error)}`, "error"); }
}


/** A text excerpt on a content card; long ones fold. */
function ContentText({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const long = Array.from(text).length > 240 || text.split("\n").length > 8;
  return <div className="ag-content-text">
    <pre className={long && !open ? "is-clipped" : undefined}>{text || "（空）"}</pre>
    {long && <button type="button" className="ag-link" onClick={() => setOpen(!open)}>{open ? <><ChevronDown size={14} />收起</> : <><ChevronRight size={14} />展开全文</>}</button>}
  </div>;
}

/**
 * A material or prompt doc the assistant creates or edits. Previews of edits turn into the
 * applied operation on the same card, like row previews do.
 */
function ContentCard({ content, title, warnings, operation, onApply, onChanged }: {
  content: AgentContentReport; title: string; warnings: string[];
  operation?: AgentOperation; onApply?: () => Promise<void>; onChanged: () => void;
}) {
  const [applying, setApplying] = useState(false);
  const { busy: undoing, undo } = useUndo(operation, onChanged);
  const view = useImageViewer();
  const undone = operation?.state === "undone";
  const thing = content.type === "material" ? "素材" : "文档";
  const badge = !operation ? "预演 · 还没改动" : undone ? "已撤销" : content.action === "create" ? "已新建" : "已修改";
  const apply = async () => {
    if (!onApply) return;
    setApplying(true);
    try { await onApply(); }
    catch (error) { notify(`执行失败：${errorText(error)}`, "error"); }
    finally { setApplying(false); }
  };
  const reveal = async () => {
    try { await revealAgentContent(content.type, content.id); }
    catch (error) { notify(`没能在主窗口打开：${errorText(error)}`, "error"); }
  };
  const coverRow = content.cover?.rowId;
  const undoLabel = content.action === "create" ? `撤销（删除这个${thing}）` : "撤销（改回原样）";
  return <section className={`ag-card${operation ? " is-applied" : ""}${undone ? " is-undone" : ""}`}>
    <header className="ag-card-head">
      {operation ? <Check size={16} /> : content.type === "material" ? <Layers size={16} /> : <FileText size={16} />}
      <strong>{title || (operation?.title ?? `${thing}修改预演`)}</strong>
      <span className={`ag-badge${operation ? (undone ? " is-muted" : " is-ok") : ""}`}>{badge}</span>
      {operation && <span className="ag-time">#{operation.id} · {formatTime(operation.createdAt)}</span>}
    </header>
    <div className={coverRow !== undefined ? "ag-content-head has-cover" : "ag-content-head"}>
      {coverRow !== undefined && <RowThumb id={coverRow} size={64} onOpen={() => view({ title: `展示图 · 图片 #${coverRow}`, ids: [coverRow], index: 0 })} />}
      <SummaryLines lines={content.lines} />
    </div>
    {undone && operation?.undoMessage && <p className="ag-op-line is-muted">{operation.undoMessage}</p>}
    {warnings.length > 0 && <div className="ag-warnings">{warnings.map(text => <p key={text}><AlertTriangle size={14} />{text}</p>)}</div>}
    {!undone && content.diffs.map((diff, index) => <div key={index} className="ag-change-field">
      <span className="ag-field-label">{diff.label}</span>
      {diff.before !== undefined && diff.after !== undefined
        ? <DiffPair before={diff.before} after={diff.after} />
        : <ContentText text={diff.after ?? diff.before ?? ""} />}
    </div>)}
    <div className="ag-card-actions">
      {!operation && <Button variant="primary" size="sm" disabled={!onApply || applying} title={onApply ? undefined : "助手正在处理，等它停下来再执行"} onClick={() => void apply()}>
        <Check size={14} />{applying ? "执行中…" : "确认修改"}
      </Button>}
      {operation && !undone && <Button size="sm" disabled={undoing} onClick={() => void undo()}><Undo2 size={14} />{undoing ? "撤销中…" : undoLabel}</Button>}
      {!undone && <Button size="sm" variant="ghost" onClick={() => void reveal()}><MonitorUp size={14} />在主窗口打开</Button>}
    </div>
    {operation && !undone && <p className="ag-hint">如果之后你又改过这个{thing}，撤销会被拒绝，以免丢掉你的修改</p>}
  </section>;
}

/**
 * A planned change, and later its result: the same card turns into the applied operation,
 * with undo, instead of a second card appearing.
 */
export function PreviewCard({ report, applied, onApply, onChanged }: {
  report: AgentPreviewReport;
  applied?: AgentOperation;
  /** Missing when it cannot be run from here (another conversation, or the assistant is busy). */
  onApply?: () => Promise<void>;
  onChanged: () => void;
}) {
  if (report.content) return <ContentCard content={report.content} title={report.title} warnings={report.warnings ?? []} operation={applied} onApply={onApply} onChanged={onChanged} />;
  return <RowPreviewCard report={report} applied={applied} onApply={onApply} onChanged={onChanged} />;
}

function RowPreviewCard({ report, applied, onApply, onChanged }: {
  report: AgentPreviewReport; applied?: AgentOperation; onApply?: () => Promise<void>; onChanged: () => void;
}) {
  const [applying, setApplying] = useState(false);
  const [details, setDetails] = useState(false);
  const { busy: undoing, undo } = useUndo(applied, onChanged);
  const undone = applied?.state === "undone";
  const source: Source = applied ? { source: "operation", id: String(applied.id) } : { source: "preview", id: report.previewId };
  const groups = report.changeGroups ?? samplesAsGroups(report.samples ?? []);
  const apply = async () => {
    if (!onApply) return;
    setApplying(true);
    try { await onApply(); }
    catch (error) { notify(`执行失败：${errorText(error)}`, "error"); }
    finally { setApplying(false); }
  };
  const badge = !applied ? "预演 · 还没改动" : undone ? "已撤销" : "已执行";
  const library = Boolean(report.library);
  const extras = [
    report.newTags.length > 0 && `${namedList(report.newTags)}是新 TAG，执行时会自动创建`,
    report.newGroups.length > 0 && `分组${namedList(report.newGroups)}还不存在，执行时会自动创建`,
  ].filter(Boolean) as string[];
  const lede = library
    ? !applied ? report.changedRows > 0 ? <>会影响 <b>{formatCount(report.changedRows)}</b> 张图片的分组或 TAG，图片本身不会被删除</> : <>不涉及图片的分组或 TAG</>
      : undone ? <>这次整理已撤销</> : <>已完成{applied.changedRows > 0 && <>，影响了 <b>{formatCount(applied.changedRows)}</b> 张图片</>}，原图文件没有被改动</>
    : !applied ? <>将修改 <b>{formatCount(report.changedRows)}</b> 张图片</> : undone ? <>这次修改已撤销</> : <>已修改 <b>{formatCount(applied.changedRows)}</b> 张图片，原图文件没有被改动</>;
  const canApply = !applied && (report.changedRows > 0 || library);
  return <section className={`ag-card${applied ? " is-applied" : ""}${undone ? " is-undone" : ""}`}>
    <header className="ag-card-head">
      {applied ? <Check size={16} /> : <Wand2 size={16} />}
      <strong>{report.title || "修改预演"}</strong>
      <span className={`ag-badge${applied ? (undone ? " is-muted" : " is-ok") : ""}`}>{badge}</span>
      {applied && <span className="ag-time">#{applied.id} · {formatTime(applied.createdAt)}</span>}
    </header>
    <p className="ag-lede">
      {lede}
      {!applied && report.unchangedRows > 0 && <span className="ag-muted">，另有 {formatCount(report.unchangedRows)} 张本来就符合，不用改</span>}
    </p>
    <SummaryLines lines={report.summary} />
    {undone && applied?.undoMessage && <p className="ag-op-line is-muted">{applied.undoMessage}</p>}
    {report.warnings.length > 0 && <div className="ag-warnings">{report.warnings.map(text => <p key={text}><AlertTriangle size={14} />{text}</p>)}</div>}
    {extras.length > 0 && !applied && <p className="ag-note">{extras.join("；")}</p>}
    <ChangeGroups groups={groups} otherRows={report.otherRows} source={source} compact={!!applied} legacy={!report.changeGroups} />
    <div className="ag-card-actions">
      {canApply && <Button variant="primary" size="sm" disabled={!onApply || applying} title={onApply ? undefined : "助手正在处理，等它停下来再执行"} onClick={() => void apply()}>
        <Check size={14} />{applying ? "执行中…" : report.changedRows > 0 ? `确认执行（${formatCount(report.changedRows)} 张）` : "确认执行"}
      </Button>}
      {applied && !undone && <Button size="sm" disabled={undoing} onClick={() => void undo()}><Undo2 size={14} />{undoing ? "撤销中…" : "撤销这次修改"}</Button>}
      {report.changedRows > 0 && !undone && <Button size="sm" variant="ghost" onClick={() => void showInMain(source)}><MonitorUp size={14} />在主窗口查看</Button>}
      <button type="button" className="ag-link ag-details-toggle" aria-expanded={details} onClick={() => setDetails(!details)}>{details ? <ChevronDown size={14} /> : <ChevronRight size={14} />}详细信息</button>
    </div>
    {details && <Details report={report} source={source} total={applied?.changedRows ?? report.changedRows} />}
  </section>;
}

export function OperationCard({ operation, onChanged }: { operation: AgentOperation; onChanged: () => void }) {
  const content = operation.report?.content;
  if (content) return <ContentCard content={content} title={operation.title} warnings={operation.report.warnings ?? []} operation={operation} onChanged={onChanged} />;
  return <RowOperationCard operation={operation} onChanged={onChanged} />;
}

function RowOperationCard({ operation, onChanged }: { operation: AgentOperation; onChanged: () => void }) {
  const [details, setDetails] = useState(false);
  const { busy, undo } = useUndo(operation, onChanged);
  const undone = operation.state === "undone";
  const isRule = operation.kind === "rule" || operation.kind === "rule_update" || operation.kind === "rule_delete";
  const report = operation.report;
  const ruleName = report.after?.name ?? report.before?.name ?? report.rule?.name ?? "";
  const ruleLine = operation.kind === "rule_update"
    ? `修改了自动规则「${ruleName}」${report.changed?.length ? `（${report.changed.join("、")}）` : ""}。撤销 = 改回修改前的样子。`
    : operation.kind === "rule_delete" ? `删除了自动规则「${ruleName}」。撤销 = 恢复这条规则。`
      : `新建了自动规则「${ruleName}」，以后导入新图时会自动执行。撤销 = 删除这条规则。`;
  const undoLabel = operation.kind === "rule_update" ? "撤销（改回原样）" : operation.kind === "rule_delete" ? "撤销（恢复规则）" : isRule ? "撤销（删除规则）" : "撤销这次修改";
  const library = operation.kind === "library";
  const source: Source = { source: "operation", id: String(operation.id) };
  const groups = report.changeGroups ?? samplesAsGroups(report.samples ?? []);
  return <section className={`ag-card is-operation${undone ? " is-undone" : ""}`}>
    <header className="ag-card-head">
      {isRule ? <ListChecks size={16} /> : <RotateCcw size={16} />}
      <strong>{operation.title}</strong>
      <span className={`ag-badge ${undone ? "is-muted" : "is-ok"}`}>{undone ? "已撤销" : "已执行"}</span>
      <span className="ag-time">#{operation.id} · {formatTime(operation.createdAt)}</span>
    </header>
    {isRule
      ? <p className="ag-op-line">{ruleLine}</p>
      : library ? <p className="ag-lede">{operation.changedRows > 0 ? <>影响了 <b>{formatCount(operation.changedRows)}</b> 张图片的分组或 TAG，</> : null}原图文件没有被改动</p>
        : <p className="ag-lede">修改了 <b>{formatCount(operation.changedRows)}</b> 张图片，原图文件没有被改动</p>}
    {library && <SummaryLines lines={report.summary} />}
    {undone && operation.undoMessage && <p className="ag-op-line is-muted">{operation.undoMessage}</p>}
    {!isRule && <ChangeGroups groups={groups} otherRows={report.otherRows} source={source} compact legacy={!report.changeGroups} />}
    <div className="ag-card-actions">
      {!undone && <Button size="sm" disabled={busy} onClick={() => void undo()}><Undo2 size={14} />{busy ? "撤销中…" : undoLabel}</Button>}
      {!isRule && !undone && operation.changedRows > 0 && <Button size="sm" variant="ghost" onClick={() => void showInMain(source)}><MonitorUp size={14} />在主窗口查看</Button>}
      {!isRule && <button type="button" className="ag-link ag-details-toggle" aria-expanded={details} onClick={() => setDetails(!details)}>{details ? <ChevronDown size={14} /> : <ChevronRight size={14} />}详细信息</button>}
    </div>
    {!isRule && details && <Details report={report} source={source} total={operation.changedRows} />}
    {!undone && !isRule && <p className="ag-hint">撤销时，之后又被改过的图片会保留现状</p>}
  </section>;
}
