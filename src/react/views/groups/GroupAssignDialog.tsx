import { useEffect, useRef, useState } from "react";
import type { RowSelection } from "../../../lib/api";
import { selectedRowIds } from "../../../lib/api";
import { errorText } from "../../../lib/utils/format";
import { assignToGroup, createNewGroup, loadGroups, useGroups } from "../../state/groups";
import { beginHistoryGroup, commitHistoryGroup } from "../../state/history";
import { materializeSelection } from "../../state/selection";
import { runTask, useTasks } from "../../state/tasks";
import { Button, Input, Modal } from "../../ui/controls";
import "./groups.css";

export interface GroupAssignDialogProps { open: boolean; onOpenChange: (open: boolean) => void; selection?: RowSelection }
export function GroupAssignDialog({ open, onOpenChange, selection }: GroupAssignDialogProps) {
  return open ? <GroupAssignSession selection={selection} close={() => onOpenChange(false)} /> : null;
}
function GroupAssignSession({ selection, close }: { selection?: RowSelection; close: () => void }) {
  const groups = useGroups();
  const taskBusy = useTasks(state => state.busy);
  const [target, setTarget] = useState<RowSelection | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const initialSelection = useRef<Promise<RowSelection> | null>(null);
  useEffect(() => {
    let disposed = false;
    void loadGroups();
    const initialize = async () => {
      try {
        initialSelection.current ??= selection ? selectedRowIds(selection).then(rowIds => ({ kind: "explicit" as const, rowIds })) : materializeSelection();
        const frozen = await initialSelection.current;
        if (!disposed) setTarget(frozen);
      } catch (cause) { if (!disposed) setError(errorText(cause)); }
    };
    void initialize(); return () => { disposed = true; };
  }, []);
  async function assign(id: number | null, create = false) {
    if (!target || busy || taskBusy) return;
    setBusy(true); setError(null); setResult(null);
    let grouped = false;
    try {
      await runTask("分配分组", async () => {
        let group = id === null ? null : groups.list.find(candidate => candidate.id === id) ?? null;
        if (create) { grouped = beginHistoryGroup(`新建并分配分组「${name.trim()}」`); group = await createNewGroup(name); }
        const count = await assignToGroup(target, group);
        setResult(group ? count ? `已将 ${count} 行分配到「${group.name}」` : `所选图片已在「${group.name}」中。` : `已取消 ${count} 行的分组。`);
        if (create) setName("");
      });
    } catch (cause) { setError(errorText(cause)); }
    finally { if (grouped) commitHistoryGroup(); setBusy(false); }
  }
  const disabled = busy || taskBusy || !target || (target.kind === "explicit" && !target.rowIds.length);
  return <Modal open onClose={close} title={`分组操作${target?.kind === "explicit" ? `（${target.rowIds.length} 行）` : ""}`} busy={busy} width={440}>
    <div className="r-group-create"><Input aria-label="新分组名称" placeholder="新分组名称…" value={name} disabled={disabled} onChange={event => setName(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing && name.trim()) void assign(null, true); }} /><Button variant="primary" disabled={disabled || !name.trim()} onClick={() => void assign(null, true)}>新建并分配</Button></div>
    {!target && !error && <p role="status">正在读取选区…</p>}
    {groups.loading && !groups.list.length && <p role="status">正在读取分组…</p>}
    {groups.error && <p role="alert" className="r-group-error">{groups.error}<Button onClick={() => void loadGroups()}>重试</Button></p>}
    {!!groups.list.length && <div className="r-group-choices" aria-label="分配到已有分组"><h4>分配到已有分组</h4>{groups.list.map(group => <Button key={group.id} variant="ghost" disabled={disabled} onClick={() => void assign(group.id)}><span title={group.name}>{group.name}</span><small>{group.memberCount.toLocaleString()} 行</small></Button>)}</div>}
    <Button disabled={disabled} onClick={() => void assign(null)}>取消分组</Button>
    {result && <p role="status" className="r-group-result">{result}</p>}{error && <p role="alert" className="r-group-error">{error}</p>}
  </Modal>;
}
