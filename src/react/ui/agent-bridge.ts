import { listen } from "@tauri-apps/api/event";
import { reportAgentMainView } from "../../lib/api/agent";
import { snapshotQueryFilters } from "../../lib/utils/library-query";
import { errorText, formatCount } from "../../lib/utils/format";
import { clearHistory } from "../state/history";
import { defaultFilters, setQuery, useLibrary, useRows } from "../state/library";
import { refreshLibrary } from "../state/library-changes";
import { selectedCount, selectionDto, setExplicitSelection, useSelection } from "../state/selection";
import { showOnlyImages } from "../state/view-selection";
import { syncGroups, useGroups } from "../state/groups";
import { syncDuplicates, useDuplicates } from "../state/duplicates";
import { useTasks } from "../state/tasks";
import { useWorkspace } from "../state/workspace";
import { notify } from "../state/notices";
import { hasFieldDrafts } from "./FieldEditor";

const FILTER_KEYS = ["search", "tags", "tagMode", "artistFilter", "dedupe", "untaggedOnly", "hideGrouped"] as const;
const DEDUPE = new Set(["none", "artists", "positivePrompt", "vibes"]);

function sanitizePatch(patch: Record<string, unknown>): Record<string, unknown> {
  const next: Record<string, unknown> = {};
  for (const key of FILTER_KEYS) {
    const value = patch[key];
    if (value === undefined) continue;
    if ((key === "search" || key === "artistFilter") && typeof value === "string") next[key] = value;
    else if (key === "tags" && Array.isArray(value)) next[key] = value.filter(item => typeof item === "string");
    else if (key === "tagMode" && (value === "and" || value === "or")) next[key] = value;
    else if (key === "dedupe" && typeof value === "string" && DEDUPE.has(value)) next[key] = value;
    else if ((key === "untaggedOnly" || key === "hideGrouped") && typeof value === "boolean") next[key] = value;
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
      const { viewMode, detailOpen } = useWorkspace.getState();
      const groups = useGroups.getState(), duplicates = useDuplicates.getState();
      const detailVisible = detailOpen && (viewMode === "gallery" || viewMode === "table"
        || (viewMode === "group" && groups.expanded.length > 0)
        || (viewMode === "duplicates" && (duplicates.layout === "list" || duplicates.expanded.length > 0)));
      const count = selectedCount(useSelection.getState());
      const filters = snapshotQueryFilters(query);
      const filtered = { kind: "filtered", ...filters, excludedRowIds: [] };
      void reportAgentMainView(count ? selectionDto() : null, filtered, {
        selectedCount: count,
        filters: { ...filters, filters: filters.filters.map(filter => filter.type === "selectedImages"
          ? { type: filter.type, count: filter.rowIds.length } : filter) },
        visibleCount: total,
        viewMode,
        detailRowId: detailVisible ? activeRow?.id ?? null : null,
      }).catch(() => {});
    }, 150);
  };
  stops.push(useSelection.subscribe((next, previous) => { if (next.version !== previous.version) report(); }));
  stops.push(useRows.subscribe((next, previous) => { if (next.query !== previous.query || next.total !== previous.total || next.activeRow?.id !== previous.activeRow?.id) report(); }));
  stops.push(useWorkspace.subscribe((next, previous) => { if (next.viewMode !== previous.viewMode || next.detailOpen !== previous.detailOpen) report(); }));
  stops.push(useGroups.subscribe((next, previous) => { if (next.expanded !== previous.expanded) report(); }));
  stops.push(useDuplicates.subscribe((next, previous) => { if (next.expanded !== previous.expanded || next.layout !== previous.layout) report(); }));
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
