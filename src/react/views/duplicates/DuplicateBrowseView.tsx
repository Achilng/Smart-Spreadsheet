import { useEffect, useState } from "react";
import { ArrowLeft, LayoutGrid, List, SlidersHorizontal } from "lucide-react";
import type { DedupeCluster } from "../../../lib/api";
import { errorText } from "../../../lib/utils/format";
import { clusterLabel, loadClusterMembers, loadClusters, renameCluster, setDuplicateMode, syncDuplicates, toggleCluster, useDuplicates } from "../../state/duplicates";
import { useLibrary, useRows } from "../../state/library";
import { clearSelection } from "../../state/selection";
import { runTask, useTasks } from "../../state/tasks";
import { Button, Input, Menu, Modal, Select, type MenuItem } from "../../ui/controls";
import { SectionHeader, SectionList, SectionMembersGrid } from "../groups/SectionParts";
import { DuplicateAlbumShelf } from "./DuplicateAlbumShelf";
import "../groups/groups.css";

export function DuplicateBrowseView({ filtersOpen, onFilters }: { filtersOpen: boolean; onFilters: () => void }) {
  const state = useDuplicates(), rows = useRows(), directory = useLibrary(value => value.snapshot?.dataDirectory), taskBusy = useTasks(value => value.busy);
  const [renaming, setRenaming] = useState<DedupeCluster | null>(null), [name, setName] = useState(""), [busy, setBusy] = useState(false), [error, setError] = useState<string | null>(null);
  useEffect(() => { syncDuplicates(); }, [state.mode, rows.resetToken, rows.query, directory]);
  const shelf = state.layout === "shelf";
  const opened = shelf ? state.expanded[0] : undefined;
  const currentCluster = state.clusters.find(cluster => cluster.key === opened);
  const albumOpen = opened !== undefined;
  const sorted = state.sortByCount ? [...state.clusters].sort((a, b) => b.memberCount - a.memberCount) : [...state.clusters].sort((a, b) => clusterLabel(a).localeCompare(clusterLabel(b)));
  const visibleClusters = shelf ? sorted.filter(cluster => cluster.key === opened) : sorted;
  const order = visibleClusters.flatMap(cluster => state.expanded.includes(cluster.key) ? (state.members[cluster.key]?.rows ?? []).slice(0, state.renderLimits[cluster.key] ?? 40).map(row => row.id) : []);
  const reveal = (key: string) => useDuplicates.setState(value => ({ renderLimits: { ...value.renderLimits, [key]: (value.renderLimits[key] ?? 40) + 40 } }));
  function clearActive() { clearSelection(); useRows.setState({ activeRow: null }); }
  function openAlbum(key: string) { clearActive(); useDuplicates.setState({ expanded: [key] }); void loadClusterMembers(key); }
  function setLayout(layout: "shelf" | "list") { clearActive(); useDuplicates.setState({ layout, expanded: [] }); }
  function clusterActions(cluster: DedupeCluster): MenuItem[] {
    return [{ label: "重命名", disabled: taskBusy, action: () => { setRenaming(cluster); setName(cluster.alias ?? clusterLabel(cluster)); setError(null); } }];
  }
  const members = (key: string) => <SectionMembersGrid data={state.members[key]} scope="duplicates" order={order} limit={state.renderLimits[key] ?? 40} onReveal={() => reveal(key)} onLoad={more => void loadClusterMembers(key, more)} />;
  async function rename() {
    if (!renaming || busy || taskBusy) return;
    setBusy(true); setError(null);
    try { await runTask("设置重复项别名", () => renameCluster(renaming, name)); setRenaming(null); }
    catch (cause) { setError(errorText(cause)); } finally { setBusy(false); }
  }
  return <div className={`r-section-view${albumOpen ? " r-group-album-detail" : ""}`}>
    {albumOpen ? <header className="r-album-header r-group-detail-header">
      <div className="r-group-detail-title"><Button variant="ghost" size="icon" aria-label="返回所有重复项" title="返回所有重复项" onClick={() => setLayout("shelf")}><ArrowLeft size={20} /></Button><h1 title={currentCluster ? clusterLabel(currentCluster) : opened}>{currentCluster ? clusterLabel(currentCluster) : opened}</h1>{currentCluster && <span className="r-album-heading-count">{currentCluster.memberCount.toLocaleString()} 张</span>}</div>
      <div className="rm-actions"><Button aria-expanded={filtersOpen} onClick={onFilters}><SlidersHorizontal size={15} />图片筛选</Button>{currentCluster && <Menu label="重复项操作" items={clusterActions(currentCluster)} />}</div>
    </header> : <>
      <header className="r-album-header"><h1 className="r-album-heading">重复项 <span className="r-album-heading-count" aria-label={`${state.clusters.length.toLocaleString()} 组重复项`}>{state.clusters.length.toLocaleString()}</span></h1><Button aria-expanded={filtersOpen} onClick={onFilters}><SlidersHorizontal size={15} />图片筛选</Button></header>
      <div className="r-group-album-toolbar"><Select label="重复项依据" value={state.mode} onChange={mode => { clearActive(); setDuplicateMode(mode); }} options={[["artists", "按画师串"], ["positivePrompt", "按正向提示词"], ["vibes", "按 VIBE 引用"]]} /><Select label="重复项顺序" value={state.sortByCount ? "count" : "name"} onChange={value => useDuplicates.setState({ sortByCount: value === "count" })} options={[["count", "按图片数排序"], ["name", "按名称排序"]]} /><span style={{ flex: 1 }} /><Button aria-pressed={shelf} onClick={() => setLayout("shelf")}><LayoutGrid size={15} />书架</Button><Button aria-pressed={!shelf} onClick={() => setLayout("list")}><List size={15} />列表</Button></div>
    </>}
    {state.loading && <p className="r-group-status" role="status">正在加载重复项…</p>}
    {state.error && <div className="r-group-status" role="alert"><p className="r-group-error">加载失败：{state.error}</p><Button onClick={() => void loadClusters()}>重试</Button></div>}
    {!state.loading && !state.error && !state.clusters.length && <p className="r-group-status">{state.mode === "vibes" ? "未找到共用同一组 VIBE 引用的图片。" : "未找到重复项（所有条目均唯一）。"}</p>}
    <SectionList scope="duplicates" loading={state.loading || state.expanded.some(key => !state.members[key] || state.members[key].loading)} version={state.version}>
      {shelf ? opened !== undefined ? members(opened) : <DuplicateAlbumShelf clusters={sorted} mode={state.mode} version={state.version} onOpen={openAlbum} /> : sorted.map(cluster => {
        const expanded = state.expanded.includes(cluster.key);
        return <section className="r-browse-section" key={cluster.key}><SectionHeader label={clusterLabel(cluster)} suffix={cluster.alias && state.mode !== "vibes" ? cluster.key : undefined} count={cluster.memberCount} expanded={expanded} onToggle={() => toggleCluster(cluster.key)} items={clusterActions(cluster)} />{expanded && members(cluster.key)}</section>;
      })}
    </SectionList>
    <Modal open={Boolean(renaming)} onClose={() => setRenaming(null)} title="重命名重复项" description="自定义名称方便识别；留空可恢复原来的名称。" busy={busy} footer={<><Button disabled={busy} onClick={() => setRenaming(null)}>取消</Button><Button variant="primary" disabled={busy || taskBusy} onClick={() => void rename()}>保存</Button></>}><Input aria-label="重复项名称" value={name} disabled={busy} onChange={event => setName(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) void rename(); }} />{error && <p role="alert" className="r-group-error">{error}</p>}</Modal>
  </div>;
}
