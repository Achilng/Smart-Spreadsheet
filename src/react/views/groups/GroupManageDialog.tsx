import { useEffect, useRef, useState } from "react";
import type { GroupSummary } from "../../../lib/api";
import { errorText } from "../../../lib/utils/format";
import { cleanEmptyGroups, createNewGroup, loadGroups, mergeGroups, removeGroup, renameExistingGroup, useGroups } from "../../state/groups";
import { runTask, useTasks } from "../../state/tasks";
import { Button, Input, Modal, Select } from "../../ui/controls";
import "./groups.css";

export function GroupManageDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  return open ? <GroupManageSession close={() => onOpenChange(false)} /> : null;
}
function GroupManageSession({ close }: { close: () => void }) {
  const groups = useGroups(), taskBusy = useTasks(state => state.busy);
  const [editing, setEditing] = useState<GroupSummary | null>(null), [name, setName] = useState("");
  const [createName, setCreateName] = useState("");
  const [deleting, setDeleting] = useState<GroupSummary | null>(null);
  const [merging, setMerging] = useState<GroupSummary | null>(null);
  const [busy, setBusy] = useState(false), [error, setError] = useState<string | null>(null), [result, setResult] = useState<string | null>(null);
  const editingInput = useRef<HTMLInputElement>(null);
  useEffect(() => { void loadGroups(); }, []);
  useEffect(() => {
    if (!editing) return;
    // Intercept at window capture before Radix's document Escape handler so an
    // inline rename can be canceled without also dismissing its parent dialog.
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.target !== editingInput.current) return;
      event.preventDefault(); event.stopPropagation();
      if (!busy) setEditing(null);
    };
    window.addEventListener("keydown", escape, true);
    return () => window.removeEventListener("keydown", escape, true);
  }, [editing, busy]);
  async function action(label: string, run: () => Promise<void>) {
    if (busy || taskBusy) return;
    setBusy(true); setError(null); setResult(null);
    try { await runTask(label, run); } catch (cause) { setError(errorText(cause)); } finally { setBusy(false); }
  }
  return <><Modal open onClose={close} title={`管理分组（${groups.list.length}）`} busy={busy} width={520}>
    <div className="r-group-create"><Input value={createName} onChange={event => setCreateName(event.target.value)} placeholder="新分组名称…" aria-label="新分组名称" disabled={busy || taskBusy} /><Button disabled={busy || taskBusy || !createName.trim()} onClick={() => void action("新建分组", async () => { await createNewGroup(createName); setCreateName(""); })}>新建分组</Button></div>
    <Button disabled={busy || taskBusy} onClick={() => void action("清理空分组", async () => { const count = await cleanEmptyGroups(); setResult(count ? `已清理 ${count} 个空分组` : "没有空分组需要清理"); })}>清理空分组</Button>
    {groups.loading && !groups.list.length && <p role="status">正在读取分组…</p>}
    {groups.error && <p role="alert" className="r-group-error">{groups.error}<Button onClick={() => void loadGroups()}>重试</Button></p>}
    {!groups.loading && !groups.error && !groups.list.length && <p className="r-group-status">暂无分组</p>}
    <div className="r-group-manage-list">{groups.list.map(group => <div key={group.id} className="r-group-manage-row">
      {editing?.id === group.id ? <><Input ref={editingInput} autoFocus aria-label="分组名称" value={name} disabled={busy || taskBusy} onChange={event => setName(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing && name.trim()) void action("重命名分组", async () => { await renameExistingGroup(group, name); setEditing(null); }); }} /><Button disabled={busy || taskBusy} onClick={() => setEditing(null)}>取消</Button><Button variant="primary" disabled={busy || taskBusy || !name.trim()} onClick={() => void action("重命名分组", async () => { await renameExistingGroup(group, name); setEditing(null); })}>保存</Button></>
        : <><span title={group.name}>{group.name}</span><small>{group.memberCount.toLocaleString()} 行</small><Button size="sm" disabled={busy || taskBusy} onClick={() => { setEditing(group); setName(group.name); }}>重命名</Button><Button size="sm" disabled={busy || taskBusy || groups.list.length < 2} onClick={() => setMerging(group)}>合并</Button><Button size="sm" variant="danger" disabled={busy || taskBusy} onClick={() => setDeleting(group)}>删除</Button></>}
    </div>)}</div>
    {result && <p role="status" className="r-group-result">{result}</p>}{error && <p role="alert" className="r-group-error">{error}</p>}
  </Modal><DeleteGroupDialog group={deleting} onClose={() => setDeleting(null)} onDeleted={() => setDeleting(null)} /><MergeGroupDialog group={merging} onClose={() => setMerging(null)} /></>;
}
export function MergeGroupDialog({ group, onClose }: { group: GroupSummary | null; onClose: () => void }) {
  const groups = useGroups(state => state.list), taskBusy = useTasks(state => state.busy);
  const [targetId, setTargetId] = useState(""), [busy, setBusy] = useState(false), [error, setError] = useState<string | null>(null);
  useEffect(() => { setTargetId(""); setError(null); }, [group?.id]);
  const options = groups.filter(candidate => candidate.id !== group?.id).map(candidate => [String(candidate.id), candidate.name] as const);
  async function merge() {
    const target = groups.find(candidate => String(candidate.id) === targetId);
    if (!group || !target || busy || taskBusy) return;
    setBusy(true); setError(null);
    try { await runTask("合并分组", () => mergeGroups(group, target)); onClose(); }
    catch (cause) { setError(errorText(cause)); } finally { setBusy(false); }
  }
  return <Modal open={Boolean(group)} onClose={onClose} title="合并分组" description={group ? `将「${group.name}」内的所有图片移入另一个分组，完成后移除原分组。可以撤销此操作。` : undefined} busy={busy} footer={<><Button disabled={busy} onClick={onClose}>取消</Button><Button variant="primary" disabled={busy || taskBusy || !targetId} onClick={() => void merge()}>合并分组</Button></>}><Select label="目标分组" value={targetId} onChange={setTargetId} options={options} disabled={busy || taskBusy} />{error && <p role="alert" className="r-group-error">{error}</p>}</Modal>;
}
export function DeleteGroupDialog({ group, onClose, onDeleted }: { group: GroupSummary | null; onClose: () => void; onDeleted?: () => void }) {
  const busyTask = useTasks(state => state.busy), [busy, setBusy] = useState(false), [error, setError] = useState<string | null>(null);
  useEffect(() => setError(null), [group?.id]);
  async function remove() {
    if (!group || busy || busyTask) return;
    setBusy(true); setError(null);
    try { await runTask("删除分组", () => removeGroup(group)); onDeleted?.(); onClose(); }
    catch (cause) { setError(errorText(cause)); } finally { setBusy(false); }
  }
  return <Modal open={Boolean(group)} onClose={onClose} title="删除分组" description={group ? `删除「${group.name}」后，组内 ${group.memberCount.toLocaleString()} 张图片会回到未分组，图片不会被删除。` : undefined} busy={busy} footer={<><Button disabled={busy} onClick={onClose}>取消</Button><Button variant="danger" disabled={busy || busyTask} onClick={() => void remove()}>{busy ? "正在删除…" : "删除分组"}</Button></>}>
    {error && <p role="alert" className="r-group-error">{error}</p>}
  </Modal>;
}
