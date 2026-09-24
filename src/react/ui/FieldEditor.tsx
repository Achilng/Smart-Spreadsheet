import { useEffect, useRef, useState } from "react";
import { Check, Copy, Pencil } from "lucide-react";
import { mutableRowState, updateCharacterPrompt, updateNegativePrompt, updateNote, updatePositivePrompt, type RowRecord } from "../../lib/api";
import { patchRowFields, useLibrary } from "../state/library";
import { recordRowStateChange } from "../state/history";
import { runTask, useTasks } from "../state/tasks";
import { notify } from "../state/notices";
import { errorText } from "../../lib/utils/format";
import { Button, Hint, Modal, Textarea } from "./controls";
import { notifyToolboxLibraryChanged } from "../../lib/windows/library-events";

type Field = "positivePrompt" | "characterPrompt" | "negativePrompt" | "note";
const drafts = new Map<string, string>();
export function hasFieldDrafts(): boolean { return drafts.size > 0; }
export function clearFieldDrafts(): void { drafts.clear(); }

export function FieldEditor({ row, field, label }: { row: RowRecord; field: Field; label: string }) {
  const directory = useLibrary(state => state.snapshot?.dataDirectory);
  const key = `${directory}\u0000${row.id}\u0000${field}`;
  const busy = useTasks(state => state.busy);
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState("");
  const [base, setBase] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [discard, setDiscard] = useState(false);
  const [restored, setRestored] = useState(false);
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(copyTimer.current), []);
  const begin = () => {
    const current = row[field] ?? "";
    const draft = drafts.get(key);
    setBase(current); setValue(draft ?? current); setRestored(draft !== undefined && draft !== current); setError(null); setEditing(true);
  };
  const cancel = () => { if (value !== base) setDiscard(true); else { drafts.delete(key); setEditing(false); } };
  const save = async () => {
    if (saving || busy) return;
    setSaving(true); setError(null);
    const before = mutableRowState(row);
    const saved = value;
    try {
      await runTask(`保存${label}`, async () => {
        if (field === "positivePrompt" || field === "characterPrompt") {
          const result = await (field === "positivePrompt" ? updatePositivePrompt : updateCharacterPrompt)(row.id, saved);
          patchRowFields(row.id, { [field]: saved, artists: result.newArtists, artistLlm: result.artistLlm ?? null });
        } else {
          await (field === "negativePrompt" ? updateNegativePrompt : updateNote)(row.id, saved);
          patchRowFields(row.id, { [field]: field === "note" ? saved.trim() || null : saved });
        }
        drafts.delete(key); setEditing(false); setRestored(false);
        notifyToolboxLibraryChanged("main");
        try { await recordRowStateChange(`编辑${label}`, [before]); }
        catch (historyError) { notify(`操作已完成，但未能记录撤销历史：${errorText(historyError)}`, "error"); }
      });
    } catch (failure) { setError(errorText(failure)); }
    finally { setSaving(false); }
  };
  const copy = async () => {
    try { await navigator.clipboard.writeText(row[field] ?? ""); setCopied(true); clearTimeout(copyTimer.current); copyTimer.current = setTimeout(() => setCopied(false), 1200); notify(`已复制${label}`); }
    catch (failure) { notify(`复制失败：${errorText(failure)}`, "error"); }
  };
  return <section><div className="r-section-heading"><h4>{label}</h4><div className="r-field-actions">
    {!editing && <Button variant="ghost" size="sm" onClick={begin} disabled={busy}><Pencil size={12} />编辑</Button>}
    <Hint text={`复制${label}`}><Button variant="ghost" size="icon" disabled={!row[field]} aria-label={`复制${label}`} onClick={() => void copy()}>{copied ? <Check size={13} /> : <Copy size={13} />}</Button></Hint>
  </div></div>
    {editing ? <div className="r-inline-editor">
      {restored && <p className="r-muted">已恢复此图片未保存的草稿</p>}
      <Textarea autoFocus aria-label={`编辑${label}`} value={value} disabled={saving} rows={field === "note" ? 3 : 6} onChange={event => {
        const next = event.target.value; setValue(next); if (next === base) drafts.delete(key); else drafts.set(key, next);
      }} onKeyDown={event => {
        if (event.nativeEvent.isComposing || saving) return;
        if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); cancel(); }
        if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); event.stopPropagation(); void save(); }
      }} />
      {error && <p className="r-field-error" role="alert">保存失败：{error}</p>}
      <div className="r-inline-editor-footer"><span>Enter 保存 · Shift+Enter 换行</span><Button variant="ghost" size="sm" disabled={saving} onClick={cancel}>取消</Button><Button variant="primary" size="sm" disabled={busy || saving} onClick={() => void save()}>{saving ? "保存中…" : "保存"}</Button></div>
    </div> : <p className={row[field] ? "r-prompt" : "r-muted"}>{row[field] || "暂无内容"}</p>}
    <Modal open={discard} onClose={() => setDiscard(false)} title={`放弃${label}修改？`} footer={<><Button onClick={() => setDiscard(false)}>继续编辑</Button><Button variant="danger" onClick={() => { drafts.delete(key); setEditing(false); setDiscard(false); }}>放弃修改</Button></>}>
      <p>尚未保存的内容将被丢弃。</p>
    </Modal>
  </section>;
}
