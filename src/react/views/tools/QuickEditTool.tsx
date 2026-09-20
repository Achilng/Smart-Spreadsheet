import { useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { Check, Search, Undo2, Redo2 } from "lucide-react";
import * as api from "../../../lib/api";
import { errorText, formatCount } from "../../../lib/utils/format";
import { notifyMainStateChanged } from "../../../lib/windows/library-events";
import { openRowInMainWindow } from "../../../lib/windows/toolbox";
import { recordHistory, redoLastAction, undoLastAction, useHistory } from "../../state/history";
import { notify } from "../../state/notices";
import { runTask, useTasks } from "../../state/tasks";
import { Button, Checkbox, Input, Modal, Textarea } from "../../ui/controls";
import { Thumbnail } from "../../ui/Thumbnail";
import "./quick-edit-styles.css";

type Operation = "tag" | "group" | "artist";
type Preview = api.QuickTagPreview | api.QuickGroupPreview | api.QuickArtistPrefixPreview;
const isTag = (value: Preview): value is api.QuickTagPreview => "associationsToAdd" in value;
const isArtist = (value: Preview): value is api.QuickArtistPrefixPreview => "promptFieldsNeedingChanges" in value;
const isGroup = (value: Preview): value is api.QuickGroupPreview => "targetGroupId" in value;
const rowName = (row: api.RowRecord) => (row.imagePath ?? row.storedImagePath)?.split(/[\\/]/).pop() ?? `图片 #${row.id}`;
function tokensOf(value: string): string[] {
  const seen = new Set<string>();
  return value.split(/[,，\n\r]+/).map(token => token.trim()).filter(token => {
    const key = token.toLocaleLowerCase();
    if (!token || seen.has(key)) return false;
    seen.add(key); return true;
  });
}

export default function QuickEditTool() {
  const [operation, setOperation] = useState<Operation>("tag");
  const [prompt, setPrompt] = useState("");
  const [artist, setArtist] = useState("");
  const [tags, setTags] = useState<api.TagSummary[]>([]);
  const [groups, setGroups] = useState<api.GroupSummary[]>([]);
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [groupId, setGroupId] = useState<number | null>(null);
  const [onlyUngrouped, setOnlyUngrouped] = useState(false);
  const [tagSearch, setTagSearch] = useState("");
  const [groupSearch, setGroupSearch] = useState("");
  const [creator, setCreator] = useState<"tag" | "group" | null>(null);
  const [newName, setNewName] = useState("");
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [working, setWorking] = useState<"preview" | "apply" | "create" | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [samples, setSamples] = useState<api.RowRecord[]>([]);
  const [resultText, setResultText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [openingRow, setOpeningRow] = useState<number | null>(null);
  const alive = useRef(false);
  const lock = useRef(false);
  const generation = useRef(0);
  const listGeneration = useRef(0);
  const history = useHistory();
  const wasHistoryBusy = useRef(false);
  const taskBusy = useTasks(state => state.busy);
  const busy = Boolean(working) || history.busy || taskBusy;
  const requiredTokens = tokensOf(prompt);
  const canPreview = !busy && (operation === "artist" ? Boolean(artist.trim()) : !loading && !loadError && requiredTokens.length > 0 && (operation === "tag" ? selectedTags.length > 0 : groupId !== null));

  function invalidate() {
    generation.current += 1;
    if (!alive.current) return;
    setPreview(null); setSamples([]); setResultText(null); setError(null); setConfirmOpen(false);
  }
  async function refreshLists() {
    const request = ++listGeneration.current;
    if (alive.current) { setLoading(true); setLoadError(null); }
    const values = await Promise.allSettled([api.listTags(), api.listGroups()]);
    if (!alive.current || request !== listGeneration.current) return;
    const [nextTags, nextGroups] = values;
    const errors: string[] = [];
    if (nextTags.status === "fulfilled") {
      setTags(nextTags.value);
      setSelectedTags(current => current.filter(name => nextTags.value.some(tag => tag.name === name)));
    } else errors.push(`无法读取 Tag 库：${errorText(nextTags.reason)}`);
    if (nextGroups.status === "fulfilled") {
      setGroups(nextGroups.value);
      setGroupId(current => nextGroups.value.some(group => group.id === current) ? current : null);
    } else errors.push(`无法读取分组：${errorText(nextGroups.reason)}`);
    setLoadError(errors.length ? errors.join("；") : null); setLoading(false);
  }
  async function refreshAfterMutation() {
    // A successful write must stay successful even if a window has closed or its list refresh fails.
    await Promise.all([refreshLists(), notifyMainStateChanged("libraryEdited")]);
  }
  useEffect(() => {
    alive.current = true;
    void refreshLists();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("main://library-changed", () => { invalidate(); void refreshLists(); }).then(cleanup => {
      if (disposed) cleanup(); else unlisten = cleanup;
    }).catch(cause => { if (!disposed) notify(`无法同步资料库变化：${errorText(cause)}`, "error"); });
    return () => { disposed = true; alive.current = false; generation.current += 1; listGeneration.current += 1; unlisten?.(); };
  }, []);
  useEffect(() => {
    if (wasHistoryBusy.current && !history.busy) { invalidate(); void refreshLists(); }
    wasHistoryBusy.current = history.busy;
  }, [history.busy]);

  const condition = (): api.QuickEditCondition => ({ fields: ["positivePrompt", "characterPrompt", "negativePrompt", "artists", "note"], requiredTokens });
  async function runPreview() {
    if (!canPreview || lock.current) return;
    lock.current = true;
    const request = ++generation.current;
    setWorking("preview"); setError(null); setResultText(null); setPreview(null); setSamples([]);
    try {
      const next = operation === "tag" ? await api.previewQuickTag(condition(), selectedTags)
        : operation === "group" ? await api.previewQuickGroup(condition(), groupId!, onlyUngrouped)
          : await api.previewQuickArtistPrefix(artist.trim());
      const rows = next.sampleRowIds.length ? await api.getRowsByIds(next.sampleRowIds.slice(0, 12)) : [];
      if (alive.current && request === generation.current) { setPreview(next); setSamples(rows); }
    } catch (cause) { if (alive.current && request === generation.current) setError(errorText(cause)); }
    finally { lock.current = false; if (alive.current) setWorking(null); }
  }
  function recordMutation(label: string, undo: () => Promise<unknown>, redo: () => Promise<unknown>) {
    const after = async (action: () => Promise<unknown>) => { await action(); invalidate(); await refreshAfterMutation(); };
    recordHistory({ label, undo: () => after(undo), redo: () => after(redo) });
  }
  async function createTarget() {
    const name = newName.trim();
    if (!name || !creator || busy || lock.current) return;
    lock.current = true; setWorking("create"); setError(null);
    try {
      await runTask(creator === "tag" ? "新建 Tag" : "新建分组", async () => {
        if (creator === "tag") {
          const created = await api.createTag(name);
          if (created) recordMutation(`新建 Tag「${name}」`, () => api.deleteTag(name), async () => {
            await api.createTag(name);
            if (alive.current) setSelectedTags(current => [...new Set([...current, name])]);
          });
          await refreshAfterMutation();
          if (alive.current) { setSelectedTags(current => [...new Set([...current, name])]); setTagSearch(""); }
          notify(created ? `已新建并选中 Tag「${name}」。` : `Tag「${name}」已存在，已为你选中。`);
        } else {
          const group = await api.createGroup(name);
          recordMutation(`新建分组「${group.name}」`, () => api.deleteGroup(group.id), async () => {
            await api.restoreGroup(group);
            if (alive.current) setGroupId(group.id);
          });
          await refreshAfterMutation();
          if (alive.current) { setGroupId(group.id); setGroupSearch(""); }
          notify(`已新建并选中分组「${group.name}」。`);
        }
        invalidate();
        if (alive.current) { setCreator(null); setNewName(""); }
      });
    } catch (cause) { if (alive.current) setError(`新建失败：${errorText(cause)}`); }
    finally { lock.current = false; if (alive.current) setWorking(null); }
  }
  async function runApply() {
    if (!preview || !preview.rowsNeedingChanges || busy || lock.current) return;
    lock.current = true; setWorking("apply"); setError(null);
    try {
      await runTask("快速整理", async () => {
        let text: string;
        let next: Preview;
        if (isTag(preview)) {
          const result = await api.applyQuickTag(condition(), selectedTags);
          const changes = result.changes.map(change => ({ ...change }));
          if (changes.length) recordMutation(`快速打 Tag（${formatCount(result.changedRows)} 张）`, () => api.revertQuickTagChanges(changes), () => api.reapplyQuickTagChanges(changes));
          text = result.associationsChanged ? `已修改 ${formatCount(result.changedRows)} 张图片，共新增 ${formatCount(result.associationsChanged)} 个 Tag 关联。` : "所有命中图片已经拥有所选 Tag，没有产生修改。";
          next = { ...preview, rowsNeedingChanges: 0, alreadyTaggedRows: preview.matchedRows, associationsToAdd: 0 };
        } else if (isGroup(preview)) {
          const result = await api.applyQuickGroup(condition(), preview.targetGroupId, preview.onlyUngrouped);
          const changes = result.changes.map(change => ({ ...change }));
          if (changes.length) recordMutation(`分组到「${preview.targetGroupName}」（${formatCount(result.changedRows)} 张）`, () => api.revertQuickGroupChanges(changes), () => api.reapplyQuickGroupChanges(changes));
          text = `已将 ${formatCount(result.changedRows)} 张图片加入「${preview.targetGroupName}」。${result.onlyUngrouped ? `跳过 ${formatCount(result.skippedGroupedRows)} 张已有分组图片。` : ""}`;
          next = { ...preview, rowsNeedingChanges: 0, alreadyInGroupRows: preview.onlyUngrouped ? 0 : preview.matchedRows, skippedGroupedRows: preview.onlyUngrouped ? preview.matchedRows : 0 };
        } else {
          const result = await api.applyQuickArtistPrefix(preview.artistName);
          const changes = result.changes.map(change => ({ ...change }));
          if (changes.length) recordMutation(`修正画师前缀「${preview.artistName}」（${formatCount(result.changedRows)} 张）`, () => api.revertQuickArtistPrefixChanges(changes), () => api.reapplyQuickArtistPrefixChanges(changes));
          text = result.changedRows ? `已修正 ${formatCount(result.changedRows)} 张图片中的 ${formatCount(result.promptFieldsChanged)} 个提示词字段。` : "没有需要修正的对应画师 Tag。";
          next = { ...preview, matchedRows: 0, rowsNeedingChanges: 0, promptFieldsNeedingChanges: 0, sampleRowIds: [] };
        }
        if (alive.current) { setPreview(next); setResultText(text); setConfirmOpen(false); if (isArtist(next)) setSamples([]); }
        notify(text);
        await refreshAfterMutation();
      });
    } catch (cause) {
      if (alive.current) { setConfirmOpen(false); setError(`执行失败，规则与预览已保留，可重试：${errorText(cause)}`); }
    } finally { lock.current = false; if (alive.current) setWorking(null); }
  }
  async function openRow(id: number) {
    if (openingRow !== null) return;
    setOpeningRow(id);
    try { await openRowInMainWindow(id); }
    catch (cause) { notify(`无法在主窗口打开图片：${errorText(cause)}`, "error"); }
    finally { if (alive.current) setOpeningRow(null); }
  }

  const targetIsTag = operation === "tag";
  const targetLabel = targetIsTag ? "Tag" : "分组";
  const search = targetIsTag ? tagSearch : groupSearch;
  const visibleTargets = targetIsTag ? tags.filter(tag => tag.name.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase())) : groups.filter(group => group.name.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()));
  const summary = preview ? previewSummary(preview) : "";
  return <div className="r-quick-edit">
    <div className="qe-operation-bar"><div className="qe-switcher" role="group" aria-label="快速整理操作类型">
      {([["tag", "添加 Tag"], ["group", "批量分组"], ["artist", "提示词操作"]] as const).map(([key, label]) => <Button key={key} disabled={busy} aria-pressed={operation === key} className={operation === key ? "is-active" : ""} onClick={() => { if (key !== operation) { setOperation(key); setCreator(null); setNewName(""); invalidate(); } }}>{label}</Button>)}
    </div><div className="qe-history">
      <Button variant="ghost" disabled={!history.undoCount || busy} title={history.undoLabel ? `撤回：${history.undoLabel}` : "没有可撤回的操作"} onClick={() => void undoLastAction()}><Undo2 size={14} />撤回</Button>
      <Button variant="ghost" disabled={!history.redoCount || busy} title={history.redoLabel ? `重做：${history.redoLabel}` : "没有可重做的操作"} onClick={() => void redoLastAction()}><Redo2 size={14} />重做</Button>
    </div></div>
    <div className="qe-layout"><div className="qe-rule-column">
      <section className="qe-card"><Step number={1} title={operation === "artist" ? "输入需要修正的画师名" : "输入提示词组合"} description={operation === "artist" ? "一次处理一个画师名，不需要填写 artist: 前缀。" : "组合中的每一项都必须存在，顺序和位置不限。"} />
        {operation === "artist" ? <Input value={artist} maxLength={240} disabled={busy} placeholder="例如：parsley_f" aria-label="需要添加 artist 前缀的画师名" onChange={event => { setArtist(event.target.value); invalidate(); }} />
          : <><Textarea value={prompt} rows={4} disabled={busy} placeholder="例如：genshin, hutao（用逗号或换行分隔）" aria-label="必须同时存在的提示词组合" onChange={event => { setPrompt(event.target.value); invalidate(); }} />{requiredTokens.length > 0 && <div className="qe-token-list" aria-label="已识别的提示词条件">{requiredTokens.map(token => <span key={token} title={token}>{token}</span>)}</div>}</>}
        <div className="qe-rules"><span>扫描范围：整个资料库</span><span>{operation === "artist" ? "处理正向、角色与负向提示词；严格匹配完整 Tag 并保留权重格式" : "忽略大小写与 NovelAI 权重；girl / 1girl / 1 girl 视为同一项"}</span>{operation === "artist" && <span>已带 artist: 前缀或仅名称相似的 Tag 不会修改</span>}</div>
      </section>
      {operation !== "artist" && <section className="qe-card"><Step number={2} title={`选择${targetIsTag ? "要添加的 Tag" : "目标分组"}`} description={targetIsTag ? "可以多选；图片原有 Tag 不会被移除。" : "命中图片会统一移入这个分组；原分组关系将被替换。"} />
        <div className="qe-target-toolbar"><Input type="search" value={search} placeholder={`搜索现有${targetLabel}`} aria-label={`搜索现有${targetLabel}`} onChange={event => targetIsTag ? setTagSearch(event.target.value) : setGroupSearch(event.target.value)} /><Button variant="ghost" disabled={busy} aria-expanded={creator === operation} onClick={() => { setCreator(creator === operation ? null : operation); setNewName(""); }}>{creator === operation ? "收起" : "＋ 新建"}</Button></div>
        {creator === operation && <form className="qe-create-panel" onSubmit={event => { event.preventDefault(); void createTarget(); }}><Input autoFocus value={newName} maxLength={120} disabled={busy} placeholder={`输入新${targetLabel}名称`} aria-label={`新建${targetLabel}名称`} onChange={event => setNewName(event.target.value)} onKeyDown={event => { if (event.key === "Escape" && !busy) { event.preventDefault(); setCreator(null); setNewName(""); } }} /><div><Button variant="ghost" disabled={busy} onClick={() => { setCreator(null); setNewName(""); }}>取消</Button><Button type="submit" disabled={!newName.trim() || busy}>{working === "create" ? "创建中…" : "创建并选中"}</Button></div></form>}
        <div className="qe-target-list" aria-label={`现有${targetLabel}列表`} aria-busy={loading}>{loading ? <p>正在读取{targetLabel}…</p> : !visibleTargets.length ? <p>{(targetIsTag ? tags : groups).length ? `没有匹配的${targetLabel}。` : `还没有${targetLabel}，可以在上方直接新建。`}</p> : visibleTargets.map(target => {
          const id = "id" in target ? target.id : target.name;
          const selected = "id" in target ? groupId === target.id : selectedTags.includes(target.name);
          return <button type="button" key={id} disabled={busy} aria-pressed={selected} className={selected ? "is-selected" : ""} onClick={() => { if ("id" in target) setGroupId(target.id); else setSelectedTags(current => current.includes(target.name) ? current.filter(name => name !== target.name) : [...current, target.name]); invalidate(); }}><span className="qe-check">{selected && <Check size={12} />}</span><strong title={target.name}>{target.name}</strong><small>{formatCount("rowCount" in target ? target.rowCount : target.memberCount)}{!targetIsTag && " 张"}</small></button>;
        })}</div>
        {targetIsTag && selectedTags.length > 0 && <p className="qe-selected">已选择 {formatCount(selectedTags.length)} 个 Tag</p>}
        {!targetIsTag && <>{groupId !== null && <p className="qe-selected">目标：{groups.find(group => group.id === groupId)?.name}</p>}<label className="qe-scope"><Checkbox checked={onlyUngrouped} disabled={busy} onCheckedChange={checked => { setOnlyUngrouped(checked === true); invalidate(); }} /><span><strong>仅处理未分组的图片</strong><small>已有任意分组的命中图片会跳过，不会从原分组移出。</small></span></label></>}
      </section>}
    </div>
    <section className="qe-card qe-preview" aria-busy={Boolean(working)}><div className="qe-preview-heading"><div><h3>执行预览</h3><p>先扫描并确认影响范围，再执行修改。</p></div><Button disabled={!canPreview} onClick={() => void runPreview()}>{working === "preview" ? "扫描中…" : "预览匹配结果"}</Button></div>
      {loadError && <div className="qe-error" role="alert">{loadError}<Button variant="ghost" disabled={busy || loading} onClick={() => void refreshLists()}>重新读取</Button></div>}
      {error && <p className="qe-error" role="alert">{error}</p>}
      {preview ? <><div className={`qe-metrics ${isArtist(preview) ? "is-artist" : ""}`}><Metric value={preview.scannedRows} label="扫描图片" />{!isArtist(preview) && <Metric value={preview.matchedRows} label="命中组合" />}<Metric value={preview.rowsNeedingChanges} label={isArtist(preview) ? "需要修正" : "需要修改"} highlight />{isArtist(preview) ? <Metric value={preview.promptFieldsNeedingChanges} label="涉及提示词字段" /> : isTag(preview) ? <Metric value={preview.alreadyTaggedRows} label="已有全部 Tag" /> : <Metric value={preview.onlyUngrouped ? preview.skippedGroupedRows : preview.alreadyInGroupRows} label={preview.onlyUngrouped ? "跳过已分组" : "已在目标分组"} />}</div>
        {samples.length ? <><div className="qe-sample-heading"><strong>命中示例</strong><span>最多展示 12 张，点击可在主窗口定位</span></div><div className="qe-samples">{samples.map(row => <button type="button" key={row.id} title={rowName(row)} disabled={openingRow !== null || busy} onClick={() => void openRow(row.id)}><Thumbnail rowId={row.id} alt={rowName(row)} /><span>{rowName(row)}</span></button>)}</div></> : <div className="qe-empty"><strong>{isArtist(preview) ? "没有找到需要修正的画师 Tag" : isGroup(preview) && preview.onlyUngrouped && preview.skippedGroupedRows > 0 ? "命中图片均已有分组" : "没有图片命中这个提示词组合"}</strong><span>{isArtist(preview) ? "已带 artist: 前缀的 Tag 会自动跳过。" : isGroup(preview) && preview.onlyUngrouped && preview.skippedGroupedRows > 0 ? "已按“仅处理未分组的图片”全部跳过。" : "除已列出的泛用别名外，空格和下划线会被严格区分。"}</span></div>}
        <div className="qe-apply"><span>{summary}</span><Button variant="primary" disabled={!preview.rowsNeedingChanges || busy} onClick={() => setConfirmOpen(true)}>{working === "apply" ? "正在应用…" : operation === "tag" ? "执行打标" : operation === "group" ? "执行分组" : "修正前缀"}</Button></div>
        {resultText && <p className="qe-result" role="status">{resultText}</p>}
      </> : <div className="qe-placeholder"><Search size={30} /><strong>{working === "preview" ? "正在扫描资料库" : "等待预览"}</strong><p>{operation === "artist" ? "输入一个画师名后，扫描整个资料库的三类提示词。" : `输入提示词组合并选择目标${targetLabel}后，扫描整个资料库。`}</p></div>}
    </section></div>
    <Modal open={confirmOpen} onClose={() => setConfirmOpen(false)} busy={working === "apply"} title="确认快速整理" description="此操作会修改资料库，完成后可以撤回。" footer={<><Button disabled={working === "apply"} onClick={() => setConfirmOpen(false)}>取消</Button><Button variant="primary" disabled={busy} onClick={() => void runApply()}>{working === "apply" ? "正在应用…" : "确认执行"}</Button></>}><p>{summary}。</p>{preview && isGroup(preview) && !preview.onlyUngrouped && <p>命中图片原有的分组关系会被替换。</p>}</Modal>
  </div>;
}

function Step({ number, title, description }: { number: number; title: string; description: string }) {
  return <div className="qe-step"><span>{number}</span><div><h3>{title}</h3><p>{description}</p></div></div>;
}
function Metric({ value, label, highlight }: { value: number; label: ReactNode; highlight?: boolean }) {
  return <div className={highlight && value > 0 ? "is-highlight" : ""}><strong>{formatCount(value)}</strong><span>{label}</span></div>;
}
function previewSummary(preview: Preview): string {
  if (isArtist(preview)) return preview.rowsNeedingChanges ? `将修正 ${formatCount(preview.rowsNeedingChanges)} 张图片中的 ${formatCount(preview.promptFieldsNeedingChanges)} 个提示词字段，为「${preview.artistName}」补全 artist: 前缀` : "整个资料库中没有需要修正的对应画师 Tag";
  if (isTag(preview)) return preview.associationsToAdd ? `将为 ${formatCount(preview.rowsNeedingChanges)} 张图片新增 ${formatCount(preview.associationsToAdd)} 个 Tag 关联` : preview.matchedRows ? "命中图片已经拥有所选 Tag" : "当前规则没有可执行的修改";
  if (preview.onlyUngrouped) return preview.rowsNeedingChanges ? `将把 ${formatCount(preview.rowsNeedingChanges)} 张未分组图片加入「${preview.targetGroupName}」；跳过 ${formatCount(preview.skippedGroupedRows)} 张已有分组图片` : preview.skippedGroupedRows ? "命中图片均已有分组，将全部跳过" : "当前规则没有可执行的修改";
  return preview.rowsNeedingChanges ? `将把 ${formatCount(preview.rowsNeedingChanges)} 张图片移入「${preview.targetGroupName}」` : preview.matchedRows ? `命中图片已经位于「${preview.targetGroupName}」` : "当前规则没有可执行的修改";
}
