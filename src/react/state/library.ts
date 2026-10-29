import { create } from "zustand";
import { getAppSnapshot, getRowsByIds, listTags, queryRows, type AppSnapshot, type RowQuery, type RowRecord, type TagSummary } from "../../lib/api";
import { createRequestQueue } from "../../lib/utils/request-queue";
import { trimPageCache } from "../../lib/utils/page-cache";
import { errorText } from "../../lib/utils/format";
import { applyScrollSnapshot, captureScrollSnapshot, clearScrollPositions, prepareFilterScrollSnapshot, type ScrollSnapshot } from "../../lib/stores/view-state";
import { useWorkspace } from "./workspace";
import { notify } from "./notices";

interface LibraryState {
  snapshot: AppSnapshot | null;
  tags: TagSummary[];
  loaded: boolean;
  error: string | null;
  tagError: string | null;
}
export const useLibrary = create<LibraryState>(() => ({ snapshot: null, tags: [], loaded: false, error: null, tagError: null }));
export const PAGE_SIZE = 200;
export const MAX_CACHED_ROW_PAGES = 12;
type Filters = Omit<RowQuery, "offset" | "limit">;
export const defaultFilters: Filters = {
  tags: [], tagMode: "and", dedupe: "none", singleArtistOnly: false, artistFilter: "", hasVibe: false,
  untaggedOnly: false, filters: [], groupView: false, hideGrouped: false, search: "", sort: "timeAsc",
};

interface RowsState {
  query: Filters;
  pages: ReadonlyMap<number, RowRecord[]>;
  total: number;
  loading: boolean;
  refreshing: boolean;
  error: string | null;
  activeRow: RowRecord | null;
  resetToken: number;
}

function readSort(): Filters["sort"] {
  try {
    const saved = localStorage.getItem("smart-spreadsheet.image-sort");
    if (saved === "timeAsc" || saved === "timeDesc" || saved === "recentlyUpdated") return saved;
  } catch { /* Preferences are optional; browsing must remain available. */ }
  return "timeAsc";
}

export const useRows = create<RowsState>(() => ({
  query: { ...defaultFilters, sort: readSort() }, pages: new Map(), total: 0, loading: true, refreshing: false,
  error: null, activeRow: null, resetToken: 0,
}));
let generation = 0;
let initialized: Promise<void> | null = null;
let pending = new Set<number>();
let replacing = false;
let incoming = new Map<number, RowRecord[]>();
let requiredPages = new Set([0]);
let visiblePages = new Set([0]);
// Range selection only needs IDs, not every visited row's prompts and metadata.
let visitedRowIds = new Map<number, readonly number[]>();
let pendingScroll: ScrollSnapshot | null = null;
let keepActive = false;
let beforeQueryChange: (searchSession?: number) => void = () => {};
const enqueue = createRequestQueue();

export function registerQueryNavigation(listener: typeof beforeQueryChange): () => void {
  beforeQueryChange = listener;
  return () => { if (beforeQueryChange === listener) beforeQueryChange = () => {}; };
}

export function browsingScrollSnapshot(): ScrollSnapshot {
  return pendingScroll ? structuredClone(pendingScroll) : captureScrollSnapshot();
}

export function hasActiveFilters(query = useRows.getState().query): boolean {
  return Boolean(query.tags.length || query.dedupe !== "none" || query.singleArtistOnly || query.artistFilter || query.hasVibe || query.untaggedOnly || query.filters.length || query.hideGrouped || query.search);
}

export function initializeLibrary(): Promise<void> {
  // A window owns one boot operation. StrictMode remounts subscribe to it without
  // starting a second IPC request or replacing the query currently being used.
  if (initialized) return initialized;
  initialized = (async () => {
    try {
      const snapshot = await getAppSnapshot();
      useLibrary.setState({ snapshot, loaded: true, error: null });
      if (snapshot.startupError) initialized = null;
      if (snapshot.dataDirectory && !snapshot.startupError) {
        await Promise.all([reloadRows(), refreshTags()]);
      }
    } catch (error) {
      useLibrary.setState({ loaded: true, error: errorText(error) });
      initialized = null;
    }
  })();
  return initialized;
}

/** Invalidate pending page requests before reconnecting to a different library. */
export function resetLibraryRows(): void {
  generation++; pending = new Set(); incoming = new Map(); replacing = false; pendingScroll = null;
  visitedRowIds = new Map(); visiblePages = new Set([0]);
  clearScrollPositions();
  useRows.setState(state => ({ query: { ...defaultFilters, sort: state.query.sort }, pages: new Map(), total: 0, loading: false, refreshing: false, error: null, activeRow: null, resetToken: state.resetToken + 1 }));
}

let tagsGeneration = 0;
export async function refreshTags(): Promise<void> {
  const request = ++tagsGeneration;
  const directory = useLibrary.getState().snapshot?.dataDirectory;
  const current = () => request === tagsGeneration && directory === useLibrary.getState().snapshot?.dataDirectory;
  try { const tags = await listTags(); if (current()) useLibrary.setState({ tags, tagError: null }); }
  catch (error) { if (current()) useLibrary.setState({ tagError: errorText(error) }); }
}

export function setQuery(patch: Partial<Filters>, searchSession?: number): void {
  const before = useRows.getState().query;
  const next = { ...before, ...patch };
  if (patch.tags?.length) next.untaggedOnly = false;
  if (patch.untaggedOnly) next.tags = [];
  if (JSON.stringify(before) === JSON.stringify(next)) return;
  const tagModeOnly = before.tagMode !== next.tagMode && JSON.stringify({ ...before, tagMode: next.tagMode }) === JSON.stringify(next);
  const dedupeOnly = before.dedupe !== next.dedupe && JSON.stringify({ ...before, dedupe: next.dedupe }) === JSON.stringify(next);
  useRows.setState({ query: next });
  if (patch.sort) {
    try { localStorage.setItem("smart-spreadsheet.image-sort", patch.sort); } catch (error) { notify(`无法记住图片顺序：${errorText(error)}`, "error"); }
  }
  // Refresh display reductions in place. Resetting to the first page remounts
  // visible cards even when deduplication returns exactly the same pictures.
  void reloadRows({ resetScroll: !tagModeOnly, filterChange: !tagModeOnly && before.sort === next.sort, preserveFilterPosition: dedupeOnly, searchSession });
}

export function clearFilters(): void {
  const query = useRows.getState().query;
  setQuery({ ...defaultFilters, sort: query.sort, tagMode: query.tagMode, groupView: query.groupView });
}

interface ReloadOptions {
  resetScroll?: boolean;
  filterChange?: boolean;
  preserveFilterPosition?: boolean;
  navigation?: ScrollSnapshot;
  keepActive?: boolean;
  searchSession?: number;
}

export async function reloadRows(options: ReloadOptions = {}): Promise<void> {
  if (!options.navigation) beforeQueryChange(options.searchSession);
  generation += 1;
  pending = new Set();
  replacing = true;
  incoming = new Map();
  keepActive = options.keepActive ?? false;
  if (options.navigation) pendingScroll = structuredClone(options.navigation);
  else if (options.filterChange) pendingScroll = prepareFilterScrollSnapshot(hasActiveFilters(), options.preserveFilterPosition);
  else if (options.resetScroll) { clearScrollPositions(hasActiveFilters()); pendingScroll = captureScrollSnapshot(); }
  else pendingScroll ??= captureScrollSnapshot();
  const range = pendingScroll.ranges.find(([key]) => key === useWorkspace.getState().viewMode)?.[1];
  const first = range ? Math.max(0, Math.floor(range.first / PAGE_SIZE)) : 0;
  const last = range ? Math.max(first, Math.floor(range.last / PAGE_SIZE)) : 0;
  requiredPages = new Set(Array.from({ length: last - first + 1 }, (_, index) => first + index));
  const state = useRows.getState();
  useRows.setState({ loading: state.pages.size === 0, refreshing: state.pages.size > 0, error: null });
  await Promise.all([...requiredPages].map(ensurePage));
}

export function ensureVisibleRowPages(pages: readonly number[]): void {
  visiblePages = new Set(pages);
  for (const page of pages) void ensurePage(page);
}

export async function ensurePage(page: number): Promise<void> {
  const state = useRows.getState();
  if (pending.has(page) || state.error || (replacing ? incoming.has(page) : state.pages.has(page))) return;
  pending.add(page);
  const requestGeneration = generation;
  const query = { ...state.query, tags: [...state.query.tags], filters: structuredClone(state.query.filters) };
  try {
    const result = await enqueue(() => requestGeneration === generation, () => queryRows({ ...query, offset: page * PAGE_SIZE, limit: PAGE_SIZE }));
    if (!result || requestGeneration !== generation || useRows.getState().error) return;
    const current = useRows.getState();
    let pages: Map<number, RowRecord[]>;
    if (replacing) {
      incoming.set(page, result.rows);
      const lastPage = Math.max(0, Math.ceil(result.totalCount / PAGE_SIZE) - 1);
      requiredPages = new Set([...requiredPages].map(value => Math.min(value, lastPage)));
      const missing = [...requiredPages].filter(value => !incoming.has(value));
      if (missing.length) { await Promise.all(missing.map(ensurePage)); return; }
      pages = new Map(incoming);
      visiblePages = new Set(requiredPages);
      visitedRowIds = new Map([...pages].map(([key, rows]) => [key, rows.map(row => row.id)]));
      incoming = new Map();
      if (pendingScroll) applyScrollSnapshot(pendingScroll);
      pendingScroll = null;
    } else {
      pages = new Map(current.pages); pages.set(page, result.rows);
      visitedRowIds.set(page, result.rows.map(row => row.id));
    }
    trimPageCache(pages, visiblePages, MAX_CACHED_ROW_PAGES);
    const activeRow = replacing && !keepActive ? null : current.activeRow;
    useRows.setState({ pages, total: result.totalCount, loading: false, refreshing: false, error: null,
      resetToken: current.resetToken + (replacing ? 1 : 0), activeRow });
    replacing = false;
  } catch (error) {
    if (requestGeneration === generation) useRows.setState({ loading: false, refreshing: false, error: errorText(error) });
  } finally {
    if (requestGeneration === generation) pending.delete(page);
  }
}

export function rowIdAt(index: number): number | undefined {
  return visitedRowIds.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE];
}

export function patchRowFields(rowId: number, fields: Partial<RowRecord>, options: { resetScroll?: boolean; reload?: boolean } = {}): void {
  const state = useRows.getState();
  const pages = new Map(state.pages);
  for (const [key, rows] of pages) {
    if (rows.some(row => row.id === rowId)) pages.set(key, rows.map(row => row.id === rowId ? { ...row, ...fields } : row));
  }
  useRows.setState({ pages, activeRow: state.activeRow?.id === rowId ? { ...state.activeRow, ...fields } : state.activeRow });
  // Prompt, tag and note edits can change filter membership and duplicate groups.
  if (options.reload !== false) void reloadRows({ resetScroll: options.resetScroll ?? state.query.sort === "recentlyUpdated", keepActive: true });
}

/** Refresh only cached changed rows when the caller has proved membership/order stable.
 * Any in-flight pagination/navigation falls back to the normal generation-safe reload.
 */
export async function refreshCachedRows(rowIds: readonly number[], isCurrent: () => boolean = () => true): Promise<boolean> {
  const state = useRows.getState();
  if (pending.size || replacing || state.loading || state.refreshing || state.error) return false;
  const requestGeneration = generation;
  const changed = new Set(rowIds);
  const ids = new Set<number>();
  for (const rows of state.pages.values()) for (const row of rows) if (changed.has(row.id)) ids.add(row.id);
  if (state.activeRow) ids.add(state.activeRow.id);
  // A bulk edit must not turn twelve cached pages into a larger reread than the
  // existing visible-page reload. Use the bounded normal path in that case.
  if (ids.size > PAGE_SIZE + 1) return false;
  const records = ids.size ? await getRowsByIds([...ids]) : [];
  const current = useRows.getState();
  if (!isCurrent() || generation !== requestGeneration || current.query !== state.query || current.activeRow?.id !== state.activeRow?.id) return false;
  const byId = new Map(records.map(row => [row.id, row]));
  if (records.length !== ids.size) return false;
  const pages = new Map(current.pages);
  for (const [key, rows] of pages) {
    if (rows.some(row => byId.has(row.id))) pages.set(key, rows.map(row => byId.get(row.id) ?? row));
  }
  // Dependent group/duplicate caches still observe every mutation, including hidden views.
  useRows.setState({ pages, activeRow: current.activeRow ? byId.get(current.activeRow.id) ?? current.activeRow : null, resetToken: current.resetToken + 1 });
  return true;
}
