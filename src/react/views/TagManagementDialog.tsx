import { useState } from "react";
import { createTag, deleteTag, renameTag } from "../../lib/api";
import { materialIdsForTag, restoreMaterialTag } from "../../lib/api/materials";
import { errorText } from "../../lib/utils/format";
import { captureSelectionStates, recordHistory, restoreRowStates } from "../state/history";
import { refreshLibrary } from "../state/library-changes";
import { setQuery, useRows } from "../state/library";
import { runTask, useTasks } from "../state/tasks";
import { notify } from "../state/notices";
import { Button, Input, Modal } from "../ui/controls";

export function TagManagementDialog({ name, mode, onClose }: { name: string; mode: "rename" | "delete"; onClose: () => void }) {
  const [value, setValue] = useState(name), [error, setError] = useState<string | null>(null), [busy, setBusy] = useState(false);
  const taskBusy = useTasks(state => state.busy);
  const renameFilter = (from: string, to: string) => { const tags = useRows.getState().query.tags; if (tags.includes(from)) setQuery({ tags: [...new Set(tags.map(tag => tag === from ? to : tag))] }); };
  const refresh = () => refreshLibrary({ preserveSelection: true });
  async function apply() {
    if (busy || taskBusy || (mode === "rename" && !value.trim())) return;
    if (mode === "rename" && value.trim() === name) { onClose(); return; }
    setBusy(true); setError(null);
    try {
      await runTask(mode === "rename" ? "重命名 Tag" : "删除 Tag", async () => {
        if (mode === "rename") {
          const to = value.trim();
          if (!await renameTag(name, to)) throw new Error("此 Tag 已不存在，请刷新列表。");
          recordHistory({ label: `重命名 Tag「${name}」`, undo: async () => { await renameTag(to, name); renameFilter(to, name); await refresh(); }, redo: async () => { await renameTag(name, to); renameFilter(name, to); await refresh(); } });
          renameFilter(name, to);
        } else {
          const before = await captureSelectionStates({ kind: "filtered", tags: [name], tagMode: "and", dedupe: "none", singleArtistOnly: false, artistFilter: "", hasVibe: false, untaggedOnly: false, filters: [], search: "", excludedRowIds: [] });
          const materialIds = await materialIdsForTag(name);
          if (!await deleteTag(name)) throw new Error("此 Tag 已不存在，请刷新列表。");
          recordHistory({ label: `删除 Tag「${name}」`, undo: async () => { await createTag(name); await restoreMaterialTag(name, materialIds); await restoreRowStates(before); await refresh(); }, redo: async () => { await deleteTag(name); await refresh(); } });
          const tags = useRows.getState().query.tags;
          if (tags.includes(name)) setQuery({ tags: tags.filter(tag => tag !== name) });
        }
        onClose();
        try { await refresh(); } catch (failure) { notify(`Tag 已更新，但刷新失败：${errorText(failure)}`, "error"); }
      });
    } catch (failure) { setError(errorText(failure)); }
    finally { setBusy(false); }
  }
  return <Modal open title={mode === "rename" ? "重命名 Tag" : `删除 Tag「${name}」`} onClose={onClose} busy={busy} description={mode === "delete" ? "将从所有图片和素材上移除此 Tag，图片和素材本身会保留。可通过撤销恢复。" : undefined}
    footer={<><Button disabled={busy} onClick={onClose}>取消</Button><Button variant={mode === "delete" ? "danger" : "primary"} disabled={busy || taskBusy || (mode === "rename" && !value.trim())} onClick={() => void apply()}>{busy ? "处理中…" : mode === "rename" ? "保存" : "确认删除"}</Button></>}>
    {mode === "rename" && <Input aria-label="Tag 新名称" value={value} disabled={busy} onChange={event => setValue(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) void apply(); }} />}
    {error && <p role="alert" className="r-field-error">{error}</p>}
  </Modal>;
}
