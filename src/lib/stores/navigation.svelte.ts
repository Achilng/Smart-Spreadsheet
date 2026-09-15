import { getRowsByIds } from "../api";
import { createNavigationHistory } from "../utils/navigation-history";
import { app, errorText, setNotice } from "./app-state.svelte";
import {
  duplicateBrowse, ensureClusterMembers, loadMoreClusterMembers, syncDuplicateCaches,
} from "./duplicate-browse-store.svelte";
import {
  groupBrowse, ensureGroupMembers, ensureUngrouped, loadMoreGroupMembers,
  loadMoreUngrouped, syncGroupBrowseCaches,
} from "./group-browse-store.svelte";
import { anyModalOpen } from "./modal-layer.svelte";
import { browsingScrollSnapshot, persistSort, registerBrowseResetListener, resetRows, rowStore } from "./row-store.svelte";
import { clearSelection } from "./selection-store.svelte";
import { applyScrollSnapshot } from "./view-state";
import { viewLabel } from "../views/shell/view-modes";

/** Read only navigable properties here: row clicks and scrolling never add an entry. */
export function navigationRoute() {
  return {
    view: app.viewMode,
    query: {
      tags: [...rowStore.tags], tagMode: rowStore.tagMode, dedupe: rowStore.dedupe,
      singleArtistOnly: rowStore.singleArtistOnly, artistFilter: rowStore.artistFilter,
      hasVibe: rowStore.hasVibe, untaggedOnly: rowStore.untaggedOnly,
      filters: JSON.parse(JSON.stringify(rowStore.filters)) as typeof rowStore.filters,
      groupView: rowStore.groupView, hideGrouped: rowStore.hideGrouped,
      search: rowStore.search, sort: rowStore.sort,
    },
    groupSort: groupBrowse.sortByCount,
    duplicateSort: duplicateBrowse.sortByCount,
    duplicateMode: duplicateBrowse.dedupeMode,
    jump: rowStore.revealToken,
  };
}

function capturePosition() {
  return {
    scroll: browsingScrollSnapshot(),
    activeId: rowStore.activeRow?.id ?? null,
    detailOpen: app.detailOpen,
    cardSize: app.galleryCardSize,
    rowHeight: app.tableRowHeight,
    groups: [...groupBrowse.expandedIds],
    renderLimits: { ...groupBrowse.renderLimits },
    groupCounts: Object.fromEntries(Object.entries(groupBrowse.memberCache).map(([id, data]) => [id, data.rows.length])),
    ungrouped: groupBrowse.ungroupedExpanded,
    ungroupedCount: groupBrowse.ungrouped?.rows.length ?? 0,
    clusters: [...duplicateBrowse.expandedKeys],
    clusterCounts: Object.fromEntries(Object.entries(duplicateBrowse.memberCache).map(([key, data]) => [key, data.rows.length])),
  };
}

type Route = ReturnType<typeof navigationRoute>;
type Position = ReturnType<typeof capturePosition>;
const history = createNavigationHistory<Route, Position>();
export const navigation = $state({
  backLabel: "", forwardLabel: "", restoring: false,
  /** Also cancels an uncommitted search timer when history is traversed. */
  token: 0,
});
let library: string | null | undefined;
let capturedBeforeReset = false;
let pendingSearchSession: number | undefined;
let restoreSerial = 0;
let sectionsReady = true;

function label(route: Route): string {
  const parts = [viewLabel(route.view)];
  if (route.query.search) parts.push(`搜索：${route.query.search}`);
  if (route.query.tags.length) parts.push(`Tag：${route.query.tags.join("、")}`);
  if (route.query.artistFilter) parts.push(`画师串：${route.query.artistFilter}`);
  if (route.query.hasVibe) parts.push("含 VIBE");
  if (route.query.untaggedOnly) parts.push("无 Tag");
  if (route.query.singleArtistOnly) parts.push("单画师");
  if (route.query.hideGrouped) parts.push("隐藏已分组");
  if (route.query.filters.length) parts.push(`${route.query.filters.length} 项筛选`);
  return parts.join(" · ");
}

function updateButtons(): void {
  navigation.backLabel = history.back ? label(history.back.route) : "";
  navigation.forwardLabel = history.forward ? label(history.forward.route) : "";
}

/** Runs before resets discard the old active row or scroll maps. */
export function installNavigation(): () => void {
  const uninstall = registerBrowseResetListener(searchSession => {
    if (!history.current || navigation.restoring || capturedBeforeReset) return;
    if (JSON.stringify(history.current.route) === JSON.stringify(navigationRoute())) return;
    history.save(capturePosition());
    capturedBeforeReset = true;
    pendingSearchSession = searchSession;
  });
  return () => {
    uninstall();
    library = undefined;
    restoreSerial += 1;
    navigation.restoring = false;
  };
}

/** Called in Workspace's pre-effect, before hidden views/caches change their state. */
export function observeNavigation(route: Route, directory: string | null | undefined): void {
  if (!history.current || directory !== library) {
    library = directory;
    restoreSerial += 1;
    navigation.restoring = false;
    capturedBeforeReset = false;
    pendingSearchSession = undefined;
    history.reset({ route, position: capturePosition() });
    updateButtons();
    return;
  }
  if (JSON.stringify(history.current.route) === JSON.stringify(route)) return;
  restoreSerial += 1;
  // A new user action supersedes any unfinished restoration.
  const wasRestoring = navigation.restoring;
  navigation.restoring = false;
  if (!capturedBeforeReset && !wasRestoring) history.save(capturePosition());
  const previous = history.current.route;
  const searchOnly = JSON.stringify({ ...previous, query: { ...previous.query, search: route.query.search } }) === JSON.stringify(route);
  history.record(
    { route, position: capturePosition() },
    searchOnly && pendingSearchSession !== undefined ? `search:${pendingSearchSession}` : undefined,
  );
  capturedBeforeReset = false;
  pendingSearchSession = undefined;
  updateButtons();
}

async function prepareSections(route: Route, position: Position, serial: number): Promise<void> {
  const current = () => serial === restoreSerial;
  if (route.view === "group") {
    syncGroupBrowseCaches();
    await Promise.all(position.groups.map(async id => {
      await ensureGroupMembers(id);
      while (current()) {
        const data = groupBrowse.memberCache[String(id)];
        if (!data || data.error || data.rows.length >= Math.min(position.groupCounts[id] ?? 0, data.totalCount)) break;
        const count = data.rows.length;
        await loadMoreGroupMembers(id);
        if (groupBrowse.memberCache[String(id)]?.rows.length === count) break;
      }
    }));
    if (position.ungrouped && current()) {
      await ensureUngrouped();
      while (current()) {
        const data = groupBrowse.ungrouped;
        if (!data || data.error || data.rows.length >= Math.min(position.ungroupedCount, data.totalCount)) break;
        const count = data.rows.length;
        await loadMoreUngrouped();
        if (groupBrowse.ungrouped?.rows.length === count) break;
      }
    }
  } else if (route.view === "duplicates") {
    await Promise.all(position.clusters.map(async key => {
      await ensureClusterMembers(key);
      while (current()) {
        const data = duplicateBrowse.memberCache[key];
        if (!data || data.error || data.rows.length >= Math.min(position.clusterCounts[key] ?? 0, data.totalCount)) break;
        const count = data.rows.length;
        await loadMoreClusterMembers(key);
        if (duplicateBrowse.memberCache[key]?.rows.length === count) break;
      }
    }));
  }
}

export function finishNavigationRestore(): void {
  if (navigation.restoring && sectionsReady && !rowStore.initialLoading && !rowStore.refreshing) {
    navigation.restoring = false;
  }
}

export function navigateHistory(direction: -1 | 1): void {
  if (anyModalOpen() || navigation.restoring) return;
  // Flush synchronous route changes (e.g. the search field's blur) before moving.
  observeNavigation(navigationRoute(), app.snapshot?.dataDirectory);
  if (!(direction === -1 ? history.back : history.forward)) return;
  history.save(capturePosition());
  const entry = history.go(direction)!;
  const { route, position } = entry;
  const serial = ++restoreSerial;
  navigation.token += 1;
  navigation.restoring = true;
  sectionsReady = false;
  capturedBeforeReset = false;
  pendingSearchSession = undefined;
  clearSelection();
  app.viewMode = route.view;
  Object.assign(rowStore, { ...route.query, tags: [...route.query.tags], filters: JSON.parse(JSON.stringify(route.query.filters)) });
  persistSort(route.query.sort);
  rowStore.revealIndex = null;
  rowStore.revealToken = route.jump;
  app.detailOpen = position.detailOpen;
  app.galleryCardSize = position.cardSize;
  app.tableRowHeight = position.rowHeight;
  groupBrowse.sortByCount = route.groupSort;
  groupBrowse.expandedIds = [...position.groups];
  groupBrowse.renderLimits = { ...position.renderLimits };
  groupBrowse.ungroupedExpanded = position.ungrouped;
  duplicateBrowse.sortByCount = route.duplicateSort;
  duplicateBrowse.dedupeMode = route.duplicateMode;
  if (route.view === "duplicates") syncDuplicateCaches();
  duplicateBrowse.expandedKeys = [...position.clusters];
  resetRows({ keepStale: true, resetScroll: true, navigation: position.scroll });
  updateButtons();

  void Promise.all([
    prepareSections(route, position, serial),
    position.activeId === null ? Promise.resolve([]) : getRowsByIds([position.activeId]),
  ]).then(([, rows]) => {
    if (serial !== restoreSerial) return;
    // Read fresh metadata; do not overwrite a row the user just clicked.
    if (rowStore.activeRow === null) rowStore.activeRow = rows[0] ?? null;
    if (route.view === "group" || route.view === "duplicates") {
      applyScrollSnapshot(position.scroll);
      rowStore.resetToken += 1;
    }
  }).catch(error => {
    if (serial === restoreSerial) setNotice({ tone: "error", text: `恢复浏览状态失败：${errorText(error)}` });
  }).finally(() => {
    if (serial !== restoreSerial) return;
    sectionsReady = true;
    finishNavigationRestore();
  });
}
