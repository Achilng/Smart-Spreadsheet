import { useEffect, useState } from "react";
import { Plus } from "lucide-react";
import { addTagsToSelection, removeTagsFromSelection, getRecentTags, setRecentTags, listSelectionTags, restoreMutableRowStates, type RowSelection } from "../../lib/api";
import { splitListText } from "../../lib/utils/list-text";
import { errorText, formatCount } from "../../lib/utils/format";
import { captureSelectionStates, recordRowStateChange } from "../state/history";
import { refreshTags, useLibrary } from "../state/library";
import { refreshLibrary } from "../state/library-changes";
import { runTask } from "../state/tasks";
import { notify } from "../state/notices";
import { Button, Checkbox, Input, Modal } from "../ui/controls";

export function TagAssignDialog({ selection, count, onClose }: { selection: RowSelection; count: number; onClose: () => void }) {
  const tags = useLibrary(state => state.tags);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [coverage, setCoverage] = useState(new Map<string, number>());
  const [recent, setRecent] = useState<string[]>([]);
  const [created, setCreated] = useState<string[]>([]);
  const [staged, setStaged] = useState(new Map<string, "add" | "remove">());
  const [discard, setDiscard] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let disposed = false;
    setLoading(true); setLoadError(null);
    void Promise.all([listSelectionTags(selection), getRecentTags(), refreshTags()]).then(([summaries, recentTags]) => {
      if (!disposed) { setCoverage(new Map(summaries.map(item => [item.name, item.selectedRows]))); setRecent(recentTags); }
    }).catch(failure => { if (!disposed) setLoadError(errorText(failure)); }).finally(() => { if (!disposed) setLoading(false); });
    return () => { disposed = true; };
  }, [selection, attempt]);
  const names = [...new Set([...created, ...tags.map(tag => tag.name)])];
  const base = (name: string) => (coverage.get(name) ?? 0) >= count ? "all" : (coverage.get(name) ?? 0) > 0 ? "partial" : "none";
  const displayed = (name: string) => staged.get(name) === "add" ? "all" : staged.get(name) === "remove" ? "none" : base(name);
  const cycle = (name: string) => {
    setResult(null);
    setStaged(previous => {
      const next = new Map(previous), op = previous.get(name);
      if (!op) next.set(name, base(name) === "all" ? "remove" : "add");
      else if (op === "add" && base(name) !== "none") next.set(name, "remove");
      else next.delete(name);
      return next;
    });
  };
  const addNames = () => {
    const nextNames = splitListText(query);
    if (!nextNames.length) return;
    setCreated(previous => [...new Set([...nextNames.filter(name => !names.includes(name)), ...previous])]);
    setStaged(previous => new Map([...previous, ...nextNames.map(name => [name, "add"] as const)])); setQuery(""); setResult(null);
  };
  const close = () => { if (!busy) { if (staged.size) setDiscard(true); else onClose(); } };
  const apply = async () => {
    if (busy || !staged.size) return;
    setBusy(true); setError(null); setResult(null);
    const adds = [...staged].filter(([, op]) => op === "add").map(([name]) => name);
    const removes = [...staged].filter(([, op]) => op === "remove").map(([name]) => name);
    try {
      await runTask("应用 Tag 修改", async () => {
        const before = await captureSelectionStates(selection);
        try {
          if (adds.length) await addTagsToSelection(selection, adds);
          if (removes.length) await removeTagsFromSelection(selection, removes);
        } catch (failure) {
          try { await restoreMutableRowStates(before); }
          catch (rollbackError) { notify(`部分 Tag 修改可能已生效，恢复失败：${errorText(rollbackError)}`, "error"); }
          await refreshLibrary({ preserveSelection: true });
          throw failure;
        }
        // A later read or preference failure must never invite applying successful writes again.
        setStaged(new Map()); setCreated([]);
        setResult(`已对 ${formatCount(count)} 张图片${[adds.length && `添加 ${adds.length} 个 Tag`, removes.length && `移除 ${removes.length} 个 Tag`].filter(Boolean).join("、")}。`);
        try { await recordRowStateChange("编辑 Tag", before); }
        catch (historyError) { notify(`Tag 已保存，但未能记录撤销历史：${errorText(historyError)}`, "error"); }
        await refreshLibrary({ preserveSelection: true });
        const summaries = await listSelectionTags(selection); setCoverage(new Map(summaries.map(item => [item.name, item.selectedRows])));
        if (adds.length) {
          const merged = [...adds, ...recent.filter(name => !adds.includes(name))].slice(0, 10);
          setRecent(merged); void setRecentTags(merged).catch(failure => notify(`Tag 已保存，但最近使用列表更新失败：${errorText(failure)}`, "error"));
        }
      });
    } catch (failure) { setError(errorText(failure)); }
    finally { setBusy(false); }
  };
  const matching = names.filter(name => name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  const recentNames = query.trim() ? [] : recent.filter(name => names.includes(name));
  const rest = matching.filter(name => !recentNames.includes(name));
  const renderTags = (items: string[]) => items.map(name => <label key={name} className="r-tag-pick" data-staged={staged.has(name)}>
    <Checkbox checked={displayed(name) === "partial" ? "indeterminate" : displayed(name) === "all"} disabled={busy || loading} onCheckedChange={() => cycle(name)} />
    <span title={name}>{name}</span><small>{staged.get(name) === "add" ? "待添加" : staged.get(name) === "remove" ? "待移除" : `${coverage.get(name) ?? 0} / ${count}`}</small>
  </label>);
  return <><Modal open onClose={close} title={`编辑 Tag（${formatCount(count)} 张）`} busy={busy} width={440}
    footer={<><span className="r-muted r-footer-summary">{staged.size ? `${staged.size} 项待应用` : "修改将在应用后保存"}</span><Button disabled={busy} onClick={close}>关闭</Button><Button variant="primary" disabled={busy || loading || !!loadError || !staged.size} onClick={() => void apply()}>{busy ? "应用中…" : "应用"}</Button></>}>
    <Input aria-label="搜索或输入新 Tag" placeholder="搜索或输入新 Tag（逗号可分隔多个）" value={query} disabled={busy} onChange={event => setQuery(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); addNames(); } }} />
    {loading ? <p className="r-muted">读取所选图片的 Tag…</p> : loadError ? <div role="alert"><p className="r-field-error">{loadError}</p><Button onClick={() => setAttempt(value => value + 1)}>重试</Button></div> : <div className="r-tag-picker">
      {recentNames.length > 0 && <><h4>最近使用</h4>{renderTags(recentNames)}<h4>全部 Tag</h4></>}{renderTags(rest)}
      {!!query.trim() && !names.includes(query.trim()) && <Button variant="ghost" onClick={addNames} disabled={busy}><Plus size={14} />新建并勾选“{query.trim()}”</Button>}
      {!names.length && !query && <p className="r-muted">输入名称以添加第一个 Tag</p>}
    </div>}
    {result && <p className="r-success" role="status">{result}</p>}{error && <p className="r-field-error" role="alert">{error}</p>}
  </Modal><Modal open={discard} onClose={() => setDiscard(false)} title="放弃未应用的 Tag 修改？" footer={<><Button onClick={() => setDiscard(false)}>继续编辑</Button><Button variant="danger" onClick={onClose}>放弃修改</Button></>}><p>未应用的修改不会保存到资料库。</p></Modal></>;
}
