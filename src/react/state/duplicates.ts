import { create } from "zustand";
import { getDedupeClusterMembers, listDedupeClusters, setDedupeAlias, type DedupeCluster, type DedupeMode } from "../../lib/api";
import { errorText } from "../../lib/utils/format";
import { useLibrary, useRows } from "./library";
import type { SectionMembers } from "./groups";
import { recordHistory } from "./history";
import { notifyToolboxLibraryChanged } from "../../lib/windows/library-events";

export type DuplicateMode = Exclude<DedupeMode, "none">;
export const useDuplicates = create<{ mode: DuplicateMode; layout: "shelf" | "list"; sortByCount: boolean; clusters: DedupeCluster[]; expanded: string[]; members: Record<string, SectionMembers>; renderLimits: Record<string, number>; loading: boolean; error: string | null; version: number }>(() => ({ mode: "artists", layout: "shelf", sortByCount: true, clusters: [], expanded: [], members: {}, renderLimits: {}, loading: false, error: null, version: 0 }));
let signature = "", generation = 0, listGeneration = 0;
let directory: string | null | undefined;
function args(): Parameters<typeof listDedupeClusters> { const q = useRows.getState().query; return [useDuplicates.getState().mode, [...q.tags], q.tagMode, q.singleArtistOnly, q.hasVibe, q.untaggedOnly, structuredClone(q.filters), q.hideGrouped]; }
export function syncDuplicates(force = false): void {
  const nextDirectory = useLibrary.getState().snapshot?.dataDirectory;
  const changedDirectory = directory !== nextDirectory;
  const next = JSON.stringify([nextDirectory, useRows.getState().resetToken, ...args()]);
  if (!force && next === signature) return;
  directory = nextDirectory;
  signature = next; generation++;
  useDuplicates.setState(state => ({ members: {}, renderLimits: {}, version: state.version + 1, ...(changedDirectory ? { clusters: [], expanded: [] } : {}) }));
  void loadClusters();
  for (const key of useDuplicates.getState().expanded) void loadClusterMembers(key);
}
export const invalidateDuplicates = () => syncDuplicates(true);
export function setDuplicateMode(mode: DuplicateMode): void {
  if (useDuplicates.getState().mode === mode) return;
  useDuplicates.setState({ mode, expanded: [], clusters: [] }); syncDuplicates(true);
}
export async function loadClusters(): Promise<void> {
  const request = ++listGeneration;
  useDuplicates.setState(state => ({ loading: !state.clusters.length, error: null }));
  try {
    const clusters = await listDedupeClusters(...args());
    if (request === listGeneration) useDuplicates.setState(state => ({ clusters, expanded: state.expanded.filter(key => clusters.some(cluster => cluster.key === key)) }));
  } catch (error) { if (request === listGeneration) useDuplicates.setState({ error: errorText(error) }); }
  finally { if (request === listGeneration) useDuplicates.setState({ loading: false }); }
}
export async function loadClusterMembers(key: string, more = false): Promise<void> {
  const current = useDuplicates.getState().members[key];
  if (current?.loading || (!more && current && !current.error) || (more && current && current.rows.length >= current.totalCount)) return;
  const request = generation;
  useDuplicates.setState(state => ({ members: { ...state.members, [key]: { rows: current?.rows ?? [], totalCount: current?.totalCount ?? 0, loading: true, error: null } } }));
  try {
    const [mode, ...filters] = args();
    const page = await getDedupeClusterMembers(mode, key, ...filters, more ? current?.rows.length ?? 0 : 0, 200);
    if (request !== generation) return;
    useDuplicates.setState(state => ({ members: { ...state.members, [key]: { rows: more ? [...(current?.rows ?? []), ...page.rows] : page.rows, totalCount: page.totalCount, loading: false, error: null } } }));
  } catch (error) { if (request === generation) useDuplicates.setState(state => ({ members: { ...state.members, [key]: { ...state.members[key], loading: false, error: errorText(error) } } })); }
}
export function loadClusterPreview(key: string) {
  const [mode, ...filters] = args();
  return getDedupeClusterMembers(mode, key, ...filters, 0, 3);
}
export function toggleCluster(key: string): void {
  const state = useDuplicates.getState();
  const expanded = state.expanded.includes(key) ? state.expanded.filter(item => item !== key) : [...state.expanded, key];
  useDuplicates.setState({ expanded }); if (expanded.includes(key)) void loadClusterMembers(key);
}
export function clusterLabel(cluster: DedupeCluster, mode = useDuplicates.getState().mode): string { return cluster.alias || (mode === "vibes" ? `VIBE 组合 ${cluster.key.slice(0, 8)}` : cluster.key); }
export async function renameCluster(cluster: DedupeCluster, alias: string): Promise<void> {
  const mode = useDuplicates.getState().mode;
  await setDedupeAlias(mode, cluster.key, alias.trim());
  notifyToolboxLibraryChanged("main");
  recordHistory({ label: "重命名重复项", undo: async () => { await setDedupeAlias(mode, cluster.key, cluster.alias ?? ""); await loadClusters(); }, redo: async () => { await setDedupeAlias(mode, cluster.key, alias.trim()); await loadClusters(); } });
  await loadClusters();
}
