import { useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Popover } from "radix-ui";
import { ChevronDown, Plus, X } from "lucide-react";
import { create } from "zustand";
import { editSelectionTags, type RowRecord } from "../../lib/api";
import { useLibrary } from "../state/library";
import { runTask, useTasks } from "../state/tasks";
import { recordTagChange, refreshTagChanges } from "../state/history";
import { assignToGroup, loadGroups, useGroups } from "../state/groups";
import { errorText } from "../../lib/utils/format";
import { notify } from "../state/notices";
import { Button, Input } from "../ui/controls";
import { FieldEditor } from "../ui/FieldEditor";

const foldKey = "smart-spreadsheet.detail-folds";
const readFolds = (): Record<string, boolean> => {
  try { const value = JSON.parse(localStorage.getItem(foldKey) || "{}"); return value && typeof value === "object" ? value : {}; } catch { return {}; }
};
const useFolds = create<Record<string, boolean>>(readFolds);
function toggleFold(name: string) {
  const next = { ...useFolds.getState(), [name]: !useFolds.getState()[name] };
  useFolds.setState(next);
  try { localStorage.setItem(foldKey, JSON.stringify(next)); } catch { /* Current session still remembers the choice. */ }
}
export function DetailFold({ name, label, summary, children }: { name: string; label: string; summary?: string | null; children: ReactNode }) {
  const open = useFolds(state => Boolean(state[name]));
  const id = useId();
  return <div className="rd-fold"><button type="button" className="rd-fold-toggle" aria-expanded={open} aria-controls={id} onClick={() => toggleFold(name)}><ChevronDown size={14} /><strong>{label}</strong><span>{summary}</span></button><div id={id} hidden={!open}>{children}</div></div>;
}

export function DetailOrganization({ row }: { row: RowRecord }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const groups = useGroups(state => state.list);
  const loading = useGroups(state => state.loading);
  const loadError = useGroups(state => state.error);
  const busy = useTasks(state => state.busy);
  const noteOpen = useFolds(state => Boolean(state.note));
  const id = useId();
  const assign = async (group: typeof groups[number] | null) => {
    if (busy) return;
    setError(null);
    try { await runTask("设置分组", () => assignToGroup({ kind: "explicit", rowIds: [row.id] }, group)); setOpen(false); }
    catch (failure) { setError(errorText(failure)); }
  };
  return <><div className="rd-organize">
    <Popover.Root open={open} onOpenChange={next => { setOpen(next); if (next) { setQuery(""); void loadGroups(); } }}>
      <Popover.Trigger asChild><Button size="sm" variant="ghost" className="rd-group-trigger" title={row.groupName || "未分组"}>分组：{row.groupName || "未分组"}<ChevronDown size={12} /></Button></Popover.Trigger>
      <Popover.Portal><Popover.Content className="rd-picker" align="start" sideOffset={6} collisionPadding={12} aria-label="选择分组">
        <Input aria-label="搜索分组" placeholder="搜索分组…" value={query} onChange={event => setQuery(event.target.value)} />
        <div className="rd-picker-list"><button type="button" disabled={busy} onClick={() => void assign(null)}>未分组{!row.groupId && " · 当前"}</button>{groups.filter(group => group.name.toLocaleLowerCase().includes(query.toLocaleLowerCase())).map(group => <button type="button" key={group.id} disabled={busy} onClick={() => void assign(group)}>{group.name}{row.groupId === group.id && " · 当前"}</button>)}</div>
        {loading && <p className="r-muted">正在加载…</p>}{loadError && <p role="alert">{loadError}<Button size="sm" onClick={() => void loadGroups()}>重试</Button></p>}{error && <p role="alert">{error}</p>}
      </Popover.Content></Popover.Portal>
    </Popover.Root>
    <Button variant="ghost" size="sm" title={row.note || "添加备注"} aria-expanded={noteOpen} aria-controls={id} onClick={() => toggleFold("note")}>{row.note ? "备注" : "添加备注"}<ChevronDown size={12} /></Button>
  </div><div id={id} hidden={!noteOpen} className="rd-note"><FieldEditor row={row} label="备注" field="note" /></div></>;
}

export function DetailTags({ row }: { row: RowRecord }) {
  const tags = useLibrary(state => state.tags);
  const busy = useTasks(state => state.busy);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [visible, setVisible] = useState(0);
  const measure = useRef<HTMLDivElement>(null);
  const rail = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const saving = useRef(false);
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useLayoutEffect(() => {
    const resize = () => {
      if (!measure.current || !rail.current) return;
      const widths = Array.from(measure.current.children).map(child => child.getBoundingClientRect().width);
      const available = rail.current.clientWidth;
      const total = widths.reduce((sum, width) => sum + width + 6, 0);
      let used = 0, count = 0;
      for (const width of widths) { if (used + width + 6 > available - (total > available ? 48 : 0)) break; used += width + 6; count++; }
      setVisible(count);
    };
    const observer = new ResizeObserver(resize);
    if (rail.current) observer.observe(rail.current);
    resize(); return () => observer.disconnect();
  }, [row.tags]);
  const save = async (next: string[]) => {
    if (busy || saving.current) return;
    saving.current = true; setError(null);
    try {
      await runTask("保存标签", async () => {
        const result = await editSelectionTags({ kind: "explicit", rowIds: [row.id] }, next.filter(tag => !row.tags.includes(tag)), row.tags.filter(tag => !next.includes(tag)));
        try { recordTagChange(result.changes, "编辑标签"); } catch (failure) { notify(`标签已保存，但撤销记录失败：${errorText(failure)}`, "error"); }
        await refreshTagChanges(result.changes);
      });
      if (alive.current) { setQuery(""); requestAnimationFrame(() => input.current?.focus()); }
    } catch (failure) { if (alive.current) setError(errorText(failure)); }
    finally { saving.current = false; }
  };
  const add = (name: string) => { const value = name.trim(); if (value && !row.tags.includes(value)) void save([...row.tags, value]); };
  const suggestions = tags.filter(tag => !row.tags.includes(tag.name) && tag.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  return <div className="rd-tagbar">
    <Popover.Root open={open} onOpenChange={setOpen}>
      <div className="rd-tag-rail" ref={rail}>{row.tags.slice(0, visible).map(tag => <button type="button" className="rd-tag-token" key={tag} title={tag} aria-label={`编辑标签：${tag}`} onClick={() => setOpen(true)}>{tag}</button>)}{visible < row.tags.length && <button className="rd-tag-token" type="button" aria-label={`查看全部 ${row.tags.length} 个标签`} onClick={() => setOpen(true)}>+{row.tags.length - visible}</button>}</div>
      <Popover.Trigger asChild><Button size="sm" variant="ghost" className="rd-tag-add"><Plus size={13} />{row.tags.length ? "标签" : "添加标签"}</Button></Popover.Trigger>
      <div ref={measure} className="rd-tag-measure" aria-hidden="true">{row.tags.map(tag => <span className="rd-tag-token" key={tag}>{tag}</span>)}</div>
      <Popover.Portal><Popover.Content className="rd-picker" align="end" sideOffset={6} collisionPadding={12} aria-label="编辑图片标签" onOpenAutoFocus={event => { event.preventDefault(); input.current?.focus(); }}>
        <input ref={input} className="r-input" aria-label="搜索或新建标签" placeholder="搜索或新建标签…" value={query} onChange={event => setQuery(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); add(query); } }} />
        {row.tags.length > 0 && <><p className="rd-picker-label">已添加</p><div className="rd-picker-tags">{row.tags.map(tag => <button type="button" key={tag} disabled={busy} onClick={() => void save(row.tags.filter(value => value !== tag))} aria-label={`移除标签 ${tag}`}>{tag}<X size={12} /></button>)}</div></>}
        <p className="rd-picker-label">{query ? "搜索结果" : "可用标签"}</p>
        <div className="rd-picker-list">{suggestions.map(tag => <button key={tag.name} type="button" disabled={busy} onClick={() => add(tag.name)}><span>{tag.name}</span><Plus size={13} /></button>)}{query.trim() && !tags.some(tag => tag.name.toLocaleLowerCase() === query.trim().toLocaleLowerCase()) && <button type="button" disabled={busy} onClick={() => add(query)}>新建“{query.trim()}”</button>}{!suggestions.length && !query && <p className="r-muted">输入名称即可新建标签</p>}</div>
        {busy && <p className="r-muted" role="status">正在保存…</p>}{error && <p className="r-field-error" role="alert">保存失败：{error}</p>}
        <p className="rd-picker-help">即时保存 · 可使用撤销恢复</p>
      </Popover.Content></Popover.Portal>
    </Popover.Root>
  </div>;
}
