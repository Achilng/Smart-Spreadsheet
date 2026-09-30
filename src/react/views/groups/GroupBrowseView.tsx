import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, LayoutGrid, List, SlidersHorizontal } from "lucide-react";
import type { GroupSummary } from "../../../lib/api";
import { errorText } from "../../../lib/utils/format";
import { loadGroupMembers, loadGroups, renameExistingGroup, syncGroups, toggleGroup, useGroups } from "../../state/groups";
import { useLibrary, useRows } from "../../state/library";
import { runTask, useTasks } from "../../state/tasks";
import { Button, Input, Menu, Modal, SearchField, Segmented, Select, type MenuItem } from "../../ui/controls";
import { FilterChips } from "../CanvasHeader";
import { DeleteGroupDialog, GroupManageDialog, MergeGroupDialog } from "./GroupManageDialog";
import { SectionHeader, SectionList, SectionMembersGrid } from "./SectionParts";
import { GroupAlbumShelf } from "./GroupAlbumShelf";
import { clearSelection } from "../../state/selection";
import { useAlbumTransition } from "../../ui/use-album-transition";
import "./groups.css";

export function GroupBrowseView({ filtersOpen, onFilters }: { filtersOpen: boolean; onFilters: () => void }) {
  const motion = useAlbumTransition();
  const groups = useGroups(), rows = useRows(), directory = useLibrary(state => state.snapshot?.dataDirectory);
  const taskBusy = useTasks(state => state.busy);
  const [managing, setManaging] = useState(false), [deleting, setDeleting] = useState<GroupSummary | null>(null), [renaming, setRenaming] = useState<GroupSummary | null>(null), [merging, setMerging] = useState<GroupSummary | null>(null);
  const [name, setName] = useState(""), [error, setError] = useState<string | null>(null), [busy, setBusy] = useState(false);
  const shelf = groups.layout === "shelf", search = groups.search;
  const setShelf = (value: boolean) => useGroups.setState({ layout: value ? "shelf" : "list" });
  const setSearch = (value: string) => useGroups.setState({ search: value });
  useEffect(() => { syncGroups(); }, [rows.resetToken, rows.query, directory]);
  const sorted = useMemo(() => (groups.sortByCount ? [...groups.list].sort((a, b) => b.memberCount - a.memberCount) : groups.list).filter(group => group.name.toLocaleLowerCase().includes(search.toLocaleLowerCase().trim())), [groups.list, groups.sortByCount, search]);
  const opened = groups.expanded[0];
  const albumOpen = shelf && Boolean(opened);
  // Remount the viewport per destination so layout clamping cannot overwrite the page we just left.
  const positionKey = JSON.stringify(albumOpen ? ["groups", "album", opened] : ["groups", groups.layout, groups.sortByCount, search.trim().toLocaleLowerCase()]);
  const currentGroup = groups.list.find(group => String(group.id) === opened);
  const albumName = opened === "ungrouped" ? "未分组" : currentGroup?.name;
  const albumCount = opened === "ungrouped" ? groups.members.ungrouped?.totalCount : currentGroup?.memberCount;
  const openAlbum = useCallback((key: string) => { void motion.navigate("enter", key, () => { clearSelection(); useRows.setState({ activeRow: null }); useGroups.setState({ expanded: [key] }); void loadGroupMembers(key); }, () => loadGroupMembers(key)); }, [motion.navigate]);
  function returnToShelf() { const update = () => { clearSelection(); useRows.setState({ activeRow: null }); useGroups.setState({ expanded: [] }); setShelf(true); }; if (albumOpen) void motion.navigate("leave", opened, update); else update(); }
  const groupActions = useCallback((group: GroupSummary): MenuItem[] => {
    return [{ label: "重命名", disabled: taskBusy, action: () => { setRenaming(group); setName(group.name); setError(null); } }, { label: "删除分组", danger: true, disabled: taskBusy, action: () => setDeleting(group) }];
  }, [taskBusy]);
  const albumActions = useCallback((group: GroupSummary): MenuItem[] => {
    const [rename, remove] = groupActions(group);
    return [rename, { label: "合并到…", disabled: taskBusy || groups.list.length < 2, action: () => setMerging(group) }, { ...remove, separator: true }];
  }, [groupActions, taskBusy, groups.list.length]);
  // Range selection must include only the groups rendered in this mode.
  const order = useMemo(() => {
    const keys = shelf ? (opened ? [opened] : []) : [...sorted.map(group => String(group.id)), "ungrouped"];
    return keys.flatMap(key => groups.expanded.includes(key) ? (groups.members[key]?.rows ?? []).slice(0, groups.renderLimits[key] ?? 40).map(row => row.id) : []);
  }, [shelf, opened, sorted, groups.expanded, groups.members, groups.renderLimits]);
  const reveal = (key: string) => useGroups.setState(state => ({ renderLimits: { ...state.renderLimits, [key]: (state.renderLimits[key] ?? 40) + 40 } }));
  async function rename() {
    if (!renaming || !name.trim() || busy || taskBusy) return;
    setBusy(true); setError(null);
    try { await runTask("重命名分组", () => renameExistingGroup(renaming, name)); setRenaming(null); }
    catch (cause) { setError(errorText(cause)); } finally { setBusy(false); }
  }
  return <div ref={motion.root} className={`r-section-view${albumOpen ? " r-group-album-detail" : ""}`}>
    {albumOpen ? <header className="r-page-head r-group-detail-header">
      <div className="r-group-detail-title"><Button variant="ghost" size="icon" aria-label="返回所有分组" title="返回所有分组" onClick={returnToShelf}><ArrowLeft size={20} /></Button><h1 className="r-page-title" title={albumName}>{albumName}</h1>{albumCount !== undefined && <span className="r-page-count">{albumCount.toLocaleString()} 张</span>}</div>
      <div className="r-page-actions"><Button className="is-toggle" aria-expanded={filtersOpen} onClick={onFilters}><SlidersHorizontal size={15} />图片筛选</Button>{currentGroup && <Menu label="分组操作" items={groupActions(currentGroup)} />}</div>
    </header> : <>
    <header className="r-page-head"><h1 className="r-page-title">分组<span className="r-page-count" aria-label={`${groups.list.length.toLocaleString()} 个分组`}>{groups.list.length.toLocaleString()}</span></h1><div className="r-page-actions"><Button className="is-toggle" aria-expanded={filtersOpen} onClick={onFilters}><SlidersHorizontal size={15} />图片筛选</Button><Button variant="primary" onClick={() => setManaging(true)}>新建 / 管理分组</Button></div></header>
    <div className="r-page-toolbar"><SearchField aria-label="搜索分组名称" placeholder="搜索分组名称…" value={search} onChange={event => setSearch(event.target.value)} onClear={() => setSearch("")} clearLabel="清除分组搜索" /><Select pill label="分组顺序" value={groups.sortByCount ? "count" : "default"} onChange={value => useGroups.setState({ sortByCount: value === "count" })} options={[["default", "默认顺序"], ["count", "按图片数排序"]]} /><span className="r-toolbar-spacer" /><Segmented label="分组布局" value={shelf ? "shelf" : "list"} onChange={value => { if (value === "shelf") returnToShelf(); else setShelf(false); }} options={[{ value: "shelf", label: "书架", icon: <LayoutGrid size={14} /> }, { value: "list", label: "列表", icon: <List size={14} /> }]} /></div>
    </>}
    <FilterChips />
    {groups.error && <div className="r-group-status" role="alert"><p className="r-group-error">加载失败：{groups.error}</p><Button onClick={() => void loadGroups()}>重试</Button></div>}
    {groups.loading && !groups.list.length && <p className="r-group-status" role="status">正在加载分组…</p>}
    <SectionList key={positionKey} positionKey={positionKey} loading={groups.loading || groups.expanded.some(key => !groups.members[key] || groups.members[key].loading)} version={groups.version}>
      {!albumOpen && !groups.list.length && !groups.loading && !groups.error && <p className="r-group-status">暂无分组。可选择图片后创建分组。</p>}
      {shelf ? opened ? <SectionMembersGrid data={groups.members[opened]} scope="groups" order={order} limit={groups.renderLimits[opened] ?? 40} onReveal={() => reveal(opened)} onLoad={more => void loadGroupMembers(opened, more)} /> : <GroupAlbumShelf groups={sorted} version={groups.version} onOpen={openAlbum} menuFor={albumActions} /> : <>
      {sorted.map(group => { const key = String(group.id), expanded = groups.expanded.includes(key); return <section key={key} className="r-browse-section"><SectionHeader label={group.name} count={group.memberCount} expanded={expanded} onToggle={() => toggleGroup(key)} items={groupActions(group)} />{expanded && <SectionMembersGrid data={groups.members[key]} scope="groups" order={order} limit={groups.renderLimits[key] ?? 40} onReveal={() => reveal(key)} onLoad={more => void loadGroupMembers(key, more)} />}</section>; })}
      <section className="r-browse-section"><SectionHeader label="未分组" count={groups.members.ungrouped?.totalCount} expanded={groups.expanded.includes("ungrouped")} onToggle={() => toggleGroup("ungrouped")} />{groups.expanded.includes("ungrouped") && <SectionMembersGrid data={groups.members.ungrouped} scope="groups" order={order} limit={groups.renderLimits.ungrouped ?? 40} onReveal={() => reveal("ungrouped")} onLoad={more => void loadGroupMembers("ungrouped", more)} />}</section></>}
    </SectionList>
    <GroupManageDialog open={managing} onOpenChange={setManaging} /><DeleteGroupDialog group={deleting} onClose={() => setDeleting(null)} /><MergeGroupDialog group={merging} onClose={() => setMerging(null)} />
    <Modal open={Boolean(renaming)} onClose={() => setRenaming(null)} title="重命名分组" busy={busy} footer={<><Button disabled={busy} onClick={() => setRenaming(null)}>取消</Button><Button variant="primary" disabled={busy || taskBusy || !name.trim()} onClick={() => void rename()}>保存</Button></>}><Input aria-label="分组名称" value={name} disabled={busy} onChange={event => setName(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) void rename(); }} />{error && <p role="alert" className="r-group-error">{error}</p>}</Modal>
  </div>;
}
