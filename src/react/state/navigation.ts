import { create } from "zustand";
import { getRowsByIds } from "../../lib/api";
import { createNavigationHistory } from "../../lib/utils/navigation-history";
import { viewLabel } from "../../lib/utils/view-modes";
import { errorText } from "../../lib/utils/format";
import { browsingScrollSnapshot, registerQueryNavigation, reloadRows, useLibrary, useRows } from "./library";
import { useWorkspace } from "./workspace";
import { clearSelection } from "./selection";
import { notify } from "./notices";
import { loadGroupMembers, syncGroups, useGroups } from "./groups";
import { loadClusterMembers, syncDuplicates, useDuplicates } from "./duplicates";

const route = () => ({ view: useWorkspace.getState().viewMode, query: structuredClone(useRows.getState().query), groupSort: useGroups.getState().sortByCount, duplicateMode: useDuplicates.getState().mode, duplicateSort: useDuplicates.getState().sortByCount });
const sections = (state: ReturnType<typeof useGroups.getState> | ReturnType<typeof useDuplicates.getState>) => ({ expanded: [...state.expanded], limits: { ...state.renderLimits }, counts: Object.fromEntries(Object.entries(state.members).map(([key, value]) => [key, value.rows.length])) });
const position = () => ({ scroll: browsingScrollSnapshot(), activeId: useRows.getState().activeRow?.id ?? null,
  detailOpen: useWorkspace.getState().detailOpen, cardSize: useWorkspace.getState().galleryCardSize, rowHeight: useWorkspace.getState().tableRowHeight, groups: sections(useGroups.getState()), duplicates: sections(useDuplicates.getState()) });
type Route = ReturnType<typeof route>;
const history = createNavigationHistory<Route, ReturnType<typeof position>>();
export const useNavigation = create<{ back: string; forward: string; restoring: boolean; token: number }>(() => ({ back: "", forward: "", restoring: false, token: 0 }));
let serial = 0;
let applyingRoute = false;
const label = (value: Route) => [viewLabel(value.view), value.query.search && `搜索：${value.query.search}`, value.query.tags.length && `Tag：${value.query.tags.join("、")}`].filter(Boolean).join(" · ");
const sync = () => useNavigation.setState({ back: history.back ? label(history.back.route) : "", forward: history.forward ? label(history.forward.route) : "" });

export function installNavigation(): () => void {
  history.reset({ route: route(), position: position() }); sync();
  const record = (searchSession?: number) => {
    if (applyingRoute) return;
    const next = route();
    const previous = history.current?.route;
    if (JSON.stringify(previous) === JSON.stringify(next)) return;
    serial++;
    if (!useNavigation.getState().restoring) history.save(position());
    useNavigation.setState({ restoring: false });
    const searchOnly = previous && JSON.stringify({ ...previous, query: { ...previous.query, search: next.query.search } }) === JSON.stringify(next);
    history.record({ route: next, position: position() }, searchOnly && searchSession !== undefined ? `search:${searchSession}` : undefined); sync();
  };
  const unsubscribeQuery = registerQueryNavigation(record);
  const unsubscribeView = useWorkspace.subscribe((next, previous) => {
    if (next.viewMode !== previous.viewMode) { record(); clearSelection(); }
  });
  const unsubscribeLibrary = useLibrary.subscribe((next, previous) => {
    if (next.snapshot?.dataDirectory !== previous.snapshot?.dataDirectory) {
      serial++; useNavigation.setState({ restoring: false });
      history.reset({ route: route(), position: position() }); sync();
    }
  });
  const unsubscribeGroups = useGroups.subscribe((next, previous) => { if (next.sortByCount !== previous.sortByCount) record(); });
  const unsubscribeDuplicates = useDuplicates.subscribe((next, previous) => { if (next.mode !== previous.mode || next.sortByCount !== previous.sortByCount) { record(); clearSelection(); } });
  return () => { unsubscribeQuery(); unsubscribeView(); unsubscribeLibrary(); unsubscribeGroups(); unsubscribeDuplicates(); serial++; useNavigation.setState({ restoring: false }); };
}

export async function navigateHistory(direction: -1 | 1): Promise<void> {
  if (useNavigation.getState().restoring || document.querySelector('[role="dialog"]')) return;
  if (!(direction === -1 ? history.back : history.forward)) return;
  history.save(position());
  const entry = history.go(direction)!;
  const current = ++serial;
  useNavigation.setState(state => ({ restoring: true, token: state.token + 1 }));
  clearSelection();
  applyingRoute = true;
  useWorkspace.setState({ viewMode: entry.route.view, detailOpen: entry.position.detailOpen, galleryCardSize: entry.position.cardSize, tableRowHeight: entry.position.rowHeight });
  useRows.setState({ query: structuredClone(entry.route.query), activeRow: null });
  useGroups.setState({ sortByCount: entry.route.groupSort });
  useDuplicates.setState({ mode: entry.route.duplicateMode, sortByCount: entry.route.duplicateSort });
  applyingRoute = false;
  try { localStorage.setItem("smart-spreadsheet.image-sort", entry.route.query.sort); } catch { /* Session navigation still works. */ }
  sync();
  try {
    const [, rows] = await Promise.all([reloadRows({ navigation: entry.position.scroll, keepActive: true }), entry.position.activeId === null ? Promise.resolve([]) : getRowsByIds([entry.position.activeId])]);
    if (current !== serial) return;
    await syncGroups(); syncDuplicates();
    if (current !== serial) return;
    useGroups.setState({ expanded: [...entry.position.groups.expanded], renderLimits: { ...entry.position.groups.limits } });
    useDuplicates.setState({ expanded: [...entry.position.duplicates.expanded], renderLimits: { ...entry.position.duplicates.limits } });
    const restoreMembers = async (count: number, get: () => { rows: unknown[]; totalCount: number; loading: boolean; error: string | null } | undefined, load: (more?: boolean) => Promise<void>, subscribe: (listener: () => void) => () => void) => {
      if (current !== serial) return;
      await load();
      if (get()?.loading) await new Promise<void>(resolve => { const stop = subscribe(() => { if (!get()?.loading || current !== serial) { stop(); resolve(); } }); });
      while (current === serial) {
        const before = get();
        if (!before || before.loading || before.error || before.rows.length >= Math.min(count, before.totalCount)) return;
        await load(true);
        if (get()?.rows.length === before.rows.length) return;
      }
    };
    await Promise.all([
      ...entry.position.groups.expanded.map(key => restoreMembers(entry.position.groups.counts[key] ?? 0, () => useGroups.getState().members[key], more => loadGroupMembers(key, more), useGroups.subscribe)),
      ...entry.position.duplicates.expanded.map(key => restoreMembers(entry.position.duplicates.counts[key] ?? 0, () => useDuplicates.getState().members[key], more => loadClusterMembers(key, more), useDuplicates.subscribe)),
    ]);
    if (current === serial && !useRows.getState().activeRow) useRows.setState({ activeRow: rows[0] ?? null });
  } catch (error) { if (current === serial) notify(`恢复浏览状态失败：${errorText(error)}`, "error"); }
  finally { if (current === serial) useNavigation.setState({ restoring: false }); }
}
