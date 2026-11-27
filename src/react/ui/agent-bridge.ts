import { listen } from "@tauri-apps/api/event";
import { getRowsByIds } from "../../lib/api";
import { reportAgentMainView } from "../../lib/api/agent";
import { openCompareWindow, type CompareWindowView } from "../../lib/windows/compare";
import type { ViewMode } from "../../lib/utils/view-modes";
import { snapshotQueryFilters } from "../../lib/utils/library-query";
import { errorText, formatCount } from "../../lib/utils/format";
import { clearHistory } from "../state/history";
import { defaultFilters, setQuery, useLibrary, useRows } from "../state/library";
import { refreshLibrary } from "../state/library-changes";
import { selectedCount, selectionDto, setExplicitSelection, useSelection } from "../state/selection";
import { showOnlyImages } from "../state/view-selection";
import { clearSelection } from "../state/selection";
import { loadGroupMembers, syncGroups, toggleGroup, useGroups } from "../state/groups";
import { clusterLabel, setDuplicateMode, syncDuplicates, useDuplicates, type DuplicateMode } from "../state/duplicates";
import { revealRow } from "../state/reveal-row";
import { useTasks } from "../state/tasks";
import { useWorkspace } from "../state/workspace";
import { notify } from "../state/notices";
import { hasFieldDrafts } from "./FieldEditor";
import { materialChangedElsewhere, revealMaterial } from "../state/materials";
import { promptDocChangedElsewhere, revealPromptDoc } from "../state/prompt-docs";

const FILTER_KEYS = ["search", "tags", "tagMode", "artistFilter", "dedupe", "singleArtistOnly", "hasVibe", "untaggedOnly", "hideGrouped"] as const;
const BOOLEAN_KEYS = new Set<string>(["singleArtistOnly", "hasVibe", "untaggedOnly", "hideGrouped"]);
const DEDUPE = new Set(["none", "artists", "positivePrompt", "vibes"]);
const VIEW_MODES = new Set<ViewMode>(["gallery", "table", "group", "duplicates", "materials", "promptDocs"]);
const SORTS = new Set(["timeAsc", "timeDesc", "recentlyUpdated"]);
/** The agent's names for the image order. */
const SORT_NAMES: Record<string, string> = { timeAsc: "oldest", timeDesc: "newest", recentlyUpdated: "recentlyUpdated" };
const MAX_REPORTED_SECTIONS = 20;

interface MainControl {
  viewMode?: ViewMode | null; sort?: string | null; duplicatesMode?: DuplicateMode | null;
  openGroupId?: number | null; openDetailRowId?: number | null; locateRowId?: number | null;
  compare?: { rowId: number; targetRowId?: number | null; view?: CompareWindowView | null } | null;
}

/** Group sections the user has open, as the agent sees them. */
function openGroups() {
  const { expanded, list, members } = useGroups.getState();
  return expanded.slice(0, MAX_REPORTED_SECTIONS).map(key => {
    if (key === "ungrouped") return { id: null, name: "未分组", memberCount: members[key]?.totalCount ?? null };
    const group = list.find(item => String(item.id) === key);
    return { id: Number(key), name: group?.name ?? null, memberCount: group?.memberCount ?? null };
  });
}

/** Duplicate clusters the user has open; a sample row lets the agent widen it with expandToSame. */
function openClusters() {
  const { mode, expanded, clusters, members } = useDuplicates.getState();
  return expanded.slice(0, MAX_REPORTED_SECTIONS).map(key => {
    const cluster = clusters.find(item => item.key === key);
    const label = cluster ? clusterLabel(cluster, mode) : key;
    return {
      label: label.length > 160 ? `${label.slice(0, 160)}…` : label,
      memberCount: cluster?.memberCount ?? members[key]?.totalCount ?? null,
      sampleRowId: members[key]?.rows[0]?.id ?? null,
    };
  });
}

function detailVisible(): boolean {
  const { viewMode, detailOpen } = useWorkspace.getState();
  const groups = useGroups.getState(), duplicates = useDuplicates.getState();
  return detailOpen && (viewMode === "gallery" || viewMode === "table"
    || (viewMode === "group" && groups.expanded.length > 0)
    || (viewMode === "duplicates" && (duplicates.layout === "list" || duplicates.expanded.length > 0)));
}

function openGroup(id: number) {
  const key = String(id);
  const groups = useGroups.getState();
  const name = groups.list.find(group => group.id === id)?.name ?? "";
  if (groups.search && !name.toLocaleLowerCase().includes(groups.search.toLocaleLowerCase().trim())) useGroups.setState({ search: "" });
  if (groups.layout === "shelf") {
    if (groups.expanded[0] === key) return;
    clearSelection(); useRows.setState({ activeRow: null });
    useGroups.setState({ expanded: [key] }); void loadGroupMembers(key);
  } else if (!groups.expanded.includes(key)) toggleGroup(key);
}

/** Interface changes the agent asked for; none of them touch library data. */
async function applyControl(control: MainControl) {
  if (control.viewMode && VIEW_MODES.has(control.viewMode)) useWorkspace.getState().setView(control.viewMode);
  if (control.duplicatesMode && DEDUPE.has(control.duplicatesMode)) {
    setDuplicateMode(control.duplicatesMode);
    useWorkspace.getState().setView("duplicates");
  }
  if (control.sort && SORTS.has(control.sort)) setQuery({ sort: control.sort as Parameters<typeof setQuery>[0]["sort"] });
  if (typeof control.openGroupId === "number") {
    useWorkspace.getState().setView("group");
    openGroup(control.openGroupId);
  }
  if (typeof control.openDetailRowId === "number") {
    const [row] = await getRowsByIds([control.openDetailRowId]);
    if (!row) throw new Error(`图片 #${control.openDetailRowId} 已不存在`);
    useWorkspace.setState({ detailOpen: true });
    if (!detailVisible()) useWorkspace.getState().setView("gallery");
    useRows.setState({ activeRow: row });
  }
  if (typeof control.locateRowId === "number") {
    await revealRow(control.locateRowId);
    notify(`整理助手已清空筛选并定位到 #${control.locateRowId}`);
  }
  if (control.compare && typeof control.compare.rowId === "number") {
    await openCompareWindow(control.compare.rowId, control.compare.targetRowId ?? undefined, control.compare.view ?? undefined);
  }
}

function sanitizePatch(patch: Record<string, unknown>): Record<string, unknown> {
  const next: Record<string, unknown> = {};
  for (const key of FILTER_KEYS) {
    const value = patch[key];
    if (value === undefined) continue;
    if ((key === "search" || key === "artistFilter") && typeof value === "string") next[key] = value;
    else if (key === "tags" && Array.isArray(value)) next[key] = value.filter(item => typeof item === "string");
    else if (key === "tagMode" && (value === "and" || value === "or")) next[key] = value;
    else if (key === "dedupe" && typeof value === "string" && DEDUPE.has(value)) next[key] = value;
    else if (BOOLEAN_KEYS.has(key) && typeof value === "boolean") next[key] = value;
  }
  return next;
}

/** Keeps the agent informed of the main window's selection and applies its requests. */
export function installAgentBridge(): () => void {
  let disposed = false;
  const stops: (() => void)[] = [];
  const retain = (promise: Promise<() => void>) => void promise.then(stop => { if (disposed) stop(); else stops.push(stop); }).catch(() => {});
  let timer: ReturnType<typeof setTimeout> | undefined;
  const report = () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      const snapshot = useLibrary.getState().snapshot;
      if (!snapshot?.dataDirectory || snapshot.startupError) return;
      const { query, total, activeRow } = useRows.getState();
      const { viewMode } = useWorkspace.getState();
      const duplicates = useDuplicates.getState();
      const count = selectedCount(useSelection.getState());
      const filters = snapshotQueryFilters(query);
      const filtered = { kind: "filtered", ...filters, excludedRowIds: [] };
      void reportAgentMainView(count ? selectionDto() : null, filtered, {
        selectedCount: count,
        filters: { ...filters, filters: filters.filters.map(filter => filter.type === "selectedImages"
          ? { type: filter.type, count: filter.rowIds.length } : filter) },
        visibleCount: total,
        viewMode,
        sort: SORT_NAMES[query.sort] ?? query.sort,
        detailRowId: detailVisible() ? activeRow?.id ?? null : null,
        ...(viewMode === "group" ? { openGroups: openGroups() } : {}),
        ...(viewMode === "duplicates" ? { duplicates: { mode: duplicates.mode, layout: duplicates.layout, openClusters: openClusters() } } : {}),
      }).catch(() => {});
    }, 150);
  };
  stops.push(useSelection.subscribe((next, previous) => { if (next.version !== previous.version) report(); }));
  stops.push(useRows.subscribe((next, previous) => { if (next.query !== previous.query || next.total !== previous.total || next.activeRow?.id !== previous.activeRow?.id) report(); }));
  stops.push(useWorkspace.subscribe((next, previous) => { if (next.viewMode !== previous.viewMode || next.detailOpen !== previous.detailOpen) report(); }));
  stops.push(useGroups.subscribe((next, previous) => { if (next.expanded !== previous.expanded || next.list !== previous.list) report(); }));
  stops.push(useDuplicates.subscribe((next, previous) => {
    if (next.expanded !== previous.expanded || next.layout !== previous.layout || next.mode !== previous.mode
      || next.clusters !== previous.clusters || next.expanded.some(key => next.members[key] !== previous.members[key])) report();
  }));
  stops.push(useLibrary.subscribe((next, previous) => { if (next.snapshot !== previous.snapshot) report(); }));
  retain(listen("agent://request-main-view", report));
  retain(listen<{ rowIds: number[]; showOnly?: boolean }>("agent://main-selection", ({ payload }) => {
    if (payload.showOnly) showOnlyImages(payload.rowIds);
    else setExplicitSelection(payload.rowIds);
    notify(`整理助手已${payload.showOnly ? "筛出" : "选中"} ${formatCount(payload.rowIds.length)} 张图片`);
  }));
  retain(listen<{ clear: boolean; patch: Record<string, unknown> }>("agent://main-filters", event => {
    const patch = sanitizePatch(event.payload.patch ?? {});
    const query = useRows.getState().query;
    setQuery((event.payload.clear ? { ...defaultFilters, sort: query.sort, ...patch } : patch) as Parameters<typeof setQuery>[0]);
  }));
  retain(listen<MainControl>("agent://main-control", ({ payload }) => {
    void applyControl(payload ?? {}).catch(error => notify(`整理助手操作主窗口失败：${errorText(error)}`, "error"));
  }));
  retain(listen<{ kind: string; id: number | string }>("agent://content-changed", ({ payload }) => {
    if (payload.kind === "material") void materialChangedElsewhere(Number(payload.id));
    else if (payload.kind === "doc") void promptDocChangedElsewhere(String(payload.id));
  }));
  retain(listen<{ kind: string; id: number | string }>("agent://reveal-content", ({ payload }) => {
    if (payload.kind === "material") void revealMaterial(Number(payload.id));
    else if (payload.kind === "doc") { useWorkspace.getState().setView("promptDocs"); void revealPromptDoc(String(payload.id)); }
  }));
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  const refresh = async () => {
    refreshTimer = undefined;
    if (disposed) return;
    if (useTasks.getState().busy || hasFieldDrafts()) { refreshTimer = setTimeout(() => void refresh(), 1200); return; }
    try {
      clearHistory();
      await refreshLibrary({ preserveSelection: true });
      void syncGroups(); syncDuplicates();
    } catch (error) { if (!disposed) notify(`刷新整理助手的修改失败：${errorText(error)}`, "error"); }
  };
  retain(listen("agent://library-changed", () => { if (!refreshTimer) refreshTimer = setTimeout(() => void refresh(), 200); }));
  report();
  return () => { disposed = true; clearTimeout(timer); clearTimeout(refreshTimer); stops.forEach(stop => stop()); };
}
