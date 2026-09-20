import { create } from "zustand";
import { getAppSnapshot, listTags, queryRows, type AppSnapshot, type RowQuery, type RowRecord, type TagSummary } from "../../lib/api";
import { createRequestQueue } from "../../lib/utils/request-queue";
import { errorText } from "../../lib/utils/format";

interface LibraryState {
  snapshot: AppSnapshot | null;
  tags: TagSummary[];
  loaded: boolean;
  error: string | null;
  tagError: string | null;
}
export const useLibrary = create<LibraryState>(() => ({ snapshot: null, tags: [], loaded: false, error: null, tagError: null }));
export const PAGE_SIZE = 200;
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
const enqueue = createRequestQueue();

export function initializeLibrary(): Promise<void> {
  // A window owns one boot operation. StrictMode remounts subscribe to it without
  // starting a second IPC request or replacing the query currently being used.
  if (initialized) return initialized;
  initialized = (async () => {
    try {
      const snapshot = await getAppSnapshot();
      useLibrary.setState({ snapshot, loaded: true, error: null });
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

export async function refreshTags(): Promise<void> {
  try { useLibrary.setState({ tags: await listTags(), tagError: null }); }
  catch (error) { useLibrary.setState({ tagError: errorText(error) }); }
}

export function setQuery(patch: Partial<Filters>): void {
  const before = useRows.getState().query;
  const next = { ...before, ...patch };
  if (JSON.stringify(before) === JSON.stringify(next)) return;
  useRows.setState({ query: next });
  if (patch.sort) {
    try { localStorage.setItem("smart-spreadsheet.image-sort", patch.sort); } catch { /* Optional preference. */ }
  }
  void reloadRows();
}

export function clearFilters(): void {
  setQuery({ ...defaultFilters, sort: useRows.getState().query.sort });
}

export async function reloadRows(): Promise<void> {
  generation += 1;
  pending = new Set();
  replacing = true;
  const state = useRows.getState();
  useRows.setState({ loading: state.pages.size === 0, refreshing: state.pages.size > 0, error: null });
  await ensurePage(0);
}

export async function ensurePage(page: number): Promise<void> {
  const state = useRows.getState();
  if (pending.has(page) || state.error || (replacing && page !== 0) || (!replacing && state.pages.has(page))) return;
  pending.add(page);
  const requestGeneration = generation;
  const query = { ...state.query, tags: [...state.query.tags], filters: structuredClone(state.query.filters) };
  try {
    const result = await enqueue(() => requestGeneration === generation, () => queryRows({ ...query, offset: page * PAGE_SIZE, limit: PAGE_SIZE }));
    if (!result || requestGeneration !== generation) return;
    const current = useRows.getState();
    const pages = new Map(replacing ? undefined : current.pages);
    pages.set(page, result.rows);
    useRows.setState({ pages, total: result.totalCount, loading: false, refreshing: false, error: null,
      resetToken: current.resetToken + (replacing ? 1 : 0), activeRow: replacing ? null : current.activeRow });
    replacing = false;
  } catch (error) {
    if (requestGeneration === generation) useRows.setState({ loading: false, refreshing: false, error: errorText(error) });
  } finally {
    if (requestGeneration === generation) pending.delete(page);
  }
}

export function rowAt(index: number): RowRecord | undefined {
  return useRows.getState().pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE];
}
