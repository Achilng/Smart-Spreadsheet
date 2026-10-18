import { create } from "zustand";
import { getDedupeClusterMembers, listDedupeClusters, setDedupeAlias, type DedupeCluster, type DedupeMode } from "../../lib/api";
import { errorText } from "../../lib/utils/format";
import { useLibrary, useRows } from "./library";
import { createSectionPager, type SectionMembers } from "./section-members";
import { recordHistory } from "./history";
import { notifyToolboxLibraryChanged } from "../../lib/windows/library-events";
import type { SortMode } from "../../lib/api";

export type DuplicateMode = Exclude<DedupeMode, "none">;
export const useDuplicates = create<{ mode: DuplicateMode; memberSort: SortMode; layout: "shelf" | "list"; sortByCount: boolean; clusters: DedupeCluster[]; expanded: string[]; members: Record<string, SectionMembers>; renderLimits: Record<string, number>; loading: boolean; error: string | null; version: number }>(() => ({ mode: "artists", memberSort: "timeAsc", layout: "shelf", sortByCount: true, clusters: [], expanded: [], members: {}, renderLimits: {}, loading: false, error: null, version: 0 }));
const memberPager = createSectionPager({
  read: key => useDuplicates.getState().members[key],
  write: (key, value) => useDuplicates.setState(state => ({ members: { ...state.members, [key]: value } })),
  clear: () => useDuplicates.setState({ members: {} }),
  fetch: (key, offset, limit) => {
    const [mode, ...filters] = args();
    return getDedupeClusterMembers(mode, key, ...filters, offset, limit, useDuplicates.getState().memberSort);
  },
});
let listGeneration = 0;
let directory: string | null | undefined;
function args(): Parameters<typeof listDedupeClusters> { const q = useRows.getState().query; return [useDuplicates.getState().mode, [...q.tags], q.tagMode, q.singleArtistOnly, q.hasVibe, q.untaggedOnly, structuredClone(q.filters), q.hideGrouped]; }
export function syncDuplicates(force = false): void {
  const nextDirectory = useLibrary.getState().snapshot?.dataDirectory;
  const changedDirectory = directory !== nextDirectory;
  const next = JSON.stringify([nextDirectory, useRows.getState().resetToken, ...args(), useDuplicates.getState().memberSort]);
  if (!memberPager.sync(next, force)) return;
  directory = nextDirectory;
  useDuplicates.setState(state => ({ renderLimits: {}, version: state.version + 1, ...(changedDirectory ? { clusters: [], expanded: [] } : {}) }));
  void loadClusters();
  for (const key of useDuplicates.getState().expanded) void loadClusterMembers(key);
}
export const invalidateDuplicates = () => syncDuplicates(true);
export function setDuplicateMemberSort(memberSort: SortMode): void {
  if (useDuplicates.getState().memberSort === memberSort) return;
  useDuplicates.setState({ memberSort });
  syncDuplicates();
}
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
export const loadClusterMembers = memberPager.load;
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
