import { useEffect, useState } from "react";
import type { DedupeCluster } from "../../../lib/api";
import { errorText } from "../../../lib/utils/format";
import { clusterLabel, loadClusterMembers, loadClusters, renameCluster, setDuplicateMode, syncDuplicates, toggleCluster, useDuplicates } from "../../state/duplicates";
import { useLibrary, useRows } from "../../state/library";
import { runTask, useTasks } from "../../state/tasks";
import { Button, Input, Modal, Select } from "../../ui/controls";
import { SectionHeader, SectionList, SectionMembersGrid } from "../groups/SectionParts";
import "../groups/groups.css";

export function DuplicateBrowseView() {
  const state = useDuplicates(), rows = useRows(), directory = useLibrary(value => value.snapshot?.dataDirectory), taskBusy = useTasks(value => value.busy);
  const [renaming, setRenaming] = useState<DedupeCluster | null>(null), [name, setName] = useState(""), [busy, setBusy] = useState(false), [error, setError] = useState<string | null>(null);
  useEffect(() => { syncDuplicates(); }, [state.mode, rows.resetToken, rows.query, directory]);
  const sorted = state.sortByCount ? [...state.clusters].sort((a, b) => b.memberCount - a.memberCount) : [...state.clusters].sort((a, b) => clusterLabel(a).localeCompare(clusterLabel(b)));
  const order = sorted.flatMap(cluster => state.expanded.includes(cluster.key) ? (state.members[cluster.key]?.rows ?? []).slice(0, state.renderLimits[cluster.key] ?? 40).map(row => row.id) : []);
  async function rename() {
    if (!renaming || busy || taskBusy) return;
    setBusy(true); setError(null);
    try { await runTask("设置重复项别名", () => renameCluster(renaming, name)); setRenaming(null); }
    catch (cause) { setError(errorText(cause)); } finally { setBusy(false); }
  }
  return <div className="r-section-view"><div className="r-section-toolbar"><span>{state.clusters.length.toLocaleString()} 组重复项</span><Select label="重复项依据" value={state.mode} onChange={setDuplicateMode} options={[["artists", "按画师串"], ["positivePrompt", "按正向提示词"], ["vibes", "按 VIBE 引用"]]} /><Select label="重复项顺序" value={state.sortByCount ? "count" : "name"} onChange={value => useDuplicates.setState({ sortByCount: value === "count" })} options={[["count", "按图片数排序"], ["name", "按名称排序"]]} /></div>
    {state.loading && <p className="r-group-status" role="status">正在加载重复项…</p>}
    {state.error && <div className="r-group-status" role="alert"><p className="r-group-error">加载失败：{state.error}</p><Button onClick={() => void loadClusters()}>重试</Button></div>}
    {!state.loading && !state.error && !state.clusters.length && <p className="r-group-status">{state.mode === "vibes" ? "未找到共用同一组 VIBE 引用的图片。" : "未找到重复项（所有条目均唯一）。"}</p>}
    <SectionList scope="duplicates" loading={state.loading || state.expanded.some(key => !state.members[key] || state.members[key].loading)} version={state.version}>{sorted.map(cluster => { const expanded = state.expanded.includes(cluster.key); return <section className="r-browse-section" key={cluster.key}><SectionHeader label={clusterLabel(cluster)} suffix={cluster.alias && state.mode !== "vibes" ? cluster.key : undefined} count={cluster.memberCount} expanded={expanded} onToggle={() => toggleCluster(cluster.key)} items={[{ label: "重命名", disabled: taskBusy, action: () => { setRenaming(cluster); setName(cluster.alias ?? clusterLabel(cluster)); setError(null); } }]} />{expanded && <SectionMembersGrid data={state.members[cluster.key]} scope="duplicates" order={order} limit={state.renderLimits[cluster.key] ?? 40} onReveal={() => useDuplicates.setState(value => ({ renderLimits: { ...value.renderLimits, [cluster.key]: (value.renderLimits[cluster.key] ?? 40) + 40 } }))} onLoad={more => void loadClusterMembers(cluster.key, more)} />}</section>; })}</SectionList>
    <Modal open={Boolean(renaming)} onClose={() => setRenaming(null)} title="重命名重复项" description="自定义名称方便识别；留空可恢复原来的名称。" busy={busy} footer={<><Button disabled={busy} onClick={() => setRenaming(null)}>取消</Button><Button variant="primary" disabled={busy || taskBusy} onClick={() => void rename()}>保存</Button></>}><Input aria-label="重复项名称" value={name} disabled={busy} onChange={event => setName(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) void rename(); }} />{error && <p role="alert" className="r-group-error">{error}</p>}</Modal>
  </div>;
}
