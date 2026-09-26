import { useEffect, useState } from "react";
import { ArrowLeft, LayoutGrid, List, SlidersHorizontal } from "lucide-react";
import type { GroupSummary } from "../../../lib/api";
import { errorText } from "../../../lib/utils/format";
import { loadGroupMembers, loadGroups, renameExistingGroup, syncGroups, toggleGroup, useGroups } from "../../state/groups";
import { useLibrary, useRows } from "../../state/library";
import { runTask, useTasks } from "../../state/tasks";
import { Button, Input, Modal, Select } from "../../ui/controls";
import { DeleteGroupDialog, GroupManageDialog } from "./GroupManageDialog";
import { SectionHeader, SectionList, SectionMembersGrid } from "./SectionParts";
import { GroupAlbumShelf } from "./GroupAlbumShelf";
import { clearSelection } from "../../state/selection";
import "./groups.css";

export function GroupBrowseView({ filtersOpen, onFilters }: { filtersOpen: boolean; onFilters: () => void }) {
  const groups = useGroups(), rows = useRows(), directory = useLibrary(state => state.snapshot?.dataDirectory);
  const taskBusy = useTasks(state => state.busy);
  const [managing, setManaging] = useState(false), [deleting, setDeleting] = useState<GroupSummary | null>(null), [renaming, setRenaming] = useState<GroupSummary | null>(null);
  const [name, setName] = useState(""), [error, setError] = useState<string | null>(null), [busy, setBusy] = useState(false);
  const [shelf, setShelf] = useState(true), [search, setSearch] = useState("");
  useEffect(() => { syncGroups(); }, [rows.resetToken, rows.query, directory]);
  const sorted = (groups.sortByCount ? [...groups.list].sort((a, b) => b.memberCount - a.memberCount) : groups.list).filter(group => group.name.toLocaleLowerCase().includes(search.toLocaleLowerCase().trim()));
  const opened = groups.expanded[0];
  function openAlbum(key: string) { clearSelection(); useRows.setState({ activeRow: null }); useGroups.setState({ expanded: [key] }); void loadGroupMembers(key); }
  // Range selection must include only the groups rendered in this mode.
  const keys = shelf ? (opened ? [opened] : []) : [...sorted.map(group => String(group.id)), "ungrouped"];
  const order = keys.flatMap(key => groups.expanded.includes(key) ? (groups.members[key]?.rows ?? []).slice(0, groups.renderLimits[key] ?? 40).map(row => row.id) : []);
  const reveal = (key: string) => useGroups.setState(state => ({ renderLimits: { ...state.renderLimits, [key]: (state.renderLimits[key] ?? 40) + 40 } }));
  async function rename() {
    if (!renaming || !name.trim() || busy || taskBusy) return;
    setBusy(true); setError(null);
    try { await runTask("重命名分组", () => renameExistingGroup(renaming, name)); setRenaming(null); }
    catch (cause) { setError(errorText(cause)); } finally { setBusy(false); }
  }
  return <div className="r-section-view">
    <header className="r-album-header"><h1 className="r-group-heading">分组 <span className="r-group-heading-count" aria-label={`${groups.list.length.toLocaleString()} 个分组`}>{groups.list.length.toLocaleString()}</span></h1><div className="rm-actions"><Button aria-expanded={filtersOpen} onClick={onFilters}><SlidersHorizontal size={15} />图片筛选</Button><Button variant="primary" onClick={() => setManaging(true)}>新建 / 管理分组</Button></div></header>
    <div className="r-group-album-toolbar"><Input aria-label="搜索分组名称" placeholder="搜索分组名称…" value={search} onChange={event => setSearch(event.target.value)} /><Select label="分组顺序" value={groups.sortByCount ? "count" : "default"} onChange={value => useGroups.setState({ sortByCount: value === "count" })} options={[["default", "默认顺序"], ["count", "按图片数排序"]]} /><Button aria-pressed={shelf} onClick={() => setShelf(true)}><LayoutGrid size={15} />书架</Button><Button aria-pressed={!shelf} onClick={() => setShelf(false)}><List size={15} />列表</Button></div>
    {shelf && opened && <div className="r-group-album-back"><Button onClick={() => { clearSelection(); useRows.setState({ activeRow: null }); useGroups.setState({ expanded: [] }); }}><ArrowLeft size={15} />所有分组</Button><span>{opened === "ungrouped" ? "未分组" : groups.list.find(group => String(group.id) === opened)?.name}</span></div>}
    {groups.error && <div className="r-group-status" role="alert"><p className="r-group-error">加载失败：{groups.error}</p><Button onClick={() => void loadGroups()}>重试</Button></div>}
    {groups.loading && !groups.list.length && <p className="r-group-status" role="status">正在加载分组…</p>}
    <SectionList scope="groups" loading={groups.loading || groups.expanded.some(key => !groups.members[key] || groups.members[key].loading)} version={groups.version}>
      {!groups.list.length && !groups.loading && !groups.error && <p className="r-group-status">暂无分组。可选择图片后创建分组。</p>}
      {shelf && !opened ? <GroupAlbumShelf groups={sorted} version={groups.version} onOpen={openAlbum} /> : <>
      {(shelf ? groups.list.filter(group => String(group.id) === opened) : sorted).map(group => { const key = String(group.id), expanded = groups.expanded.includes(key); return <section key={key} className="r-browse-section"><SectionHeader label={group.name} count={group.memberCount} expanded={expanded} onToggle={() => toggleGroup(key)} items={[{ label: "重命名", disabled: taskBusy, action: () => { setRenaming(group); setName(group.name); setError(null); } }, { label: "删除分组", danger: true, disabled: taskBusy, action: () => setDeleting(group) }]} />{expanded && <SectionMembersGrid data={groups.members[key]} scope="groups" order={order} limit={groups.renderLimits[key] ?? 40} onReveal={() => reveal(key)} onLoad={more => void loadGroupMembers(key, more)} />}</section>; })}
      {(!shelf || opened === "ungrouped") && <section className="r-browse-section"><SectionHeader label="未分组" count={groups.members.ungrouped?.totalCount} expanded={groups.expanded.includes("ungrouped")} onToggle={() => toggleGroup("ungrouped")} />{groups.expanded.includes("ungrouped") && <SectionMembersGrid data={groups.members.ungrouped} scope="groups" order={order} limit={groups.renderLimits.ungrouped ?? 40} onReveal={() => reveal("ungrouped")} onLoad={more => void loadGroupMembers("ungrouped", more)} />}</section>}</>}
    </SectionList>
    <GroupManageDialog open={managing} onOpenChange={setManaging} /><DeleteGroupDialog group={deleting} onClose={() => setDeleting(null)} />
    <Modal open={Boolean(renaming)} onClose={() => setRenaming(null)} title="重命名分组" busy={busy} footer={<><Button disabled={busy} onClick={() => setRenaming(null)}>取消</Button><Button variant="primary" disabled={busy || taskBusy || !name.trim()} onClick={() => void rename()}>保存</Button></>}><Input aria-label="分组名称" value={name} disabled={busy} onChange={event => setName(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) void rename(); }} />{error && <p role="alert" className="r-group-error">{error}</p>}</Modal>
  </div>;
}
