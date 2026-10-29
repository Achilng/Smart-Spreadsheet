import type { RowRecord } from "../../lib/api";
import { errorText } from "../../lib/utils/format";

export interface SectionMembers {
  rows: RowRecord[]; totalCount: number; loading: boolean; error: string | null; exhausted?: boolean; stale?: boolean;
}

/** Album pagination with in-place refreshes, matching the gallery's retained pages. */
export function createSectionPager(adapter: {
  read: (key: string) => SectionMembers | undefined;
  write: (key: string, value: SectionMembers) => void;
  clear: (preserve: boolean) => void;
  fetch: (key: string, offset: number, limit: number) => Promise<{ rows: RowRecord[]; totalCount: number; hasMore?: boolean }>;
}) {
  let generation = 0, signature = "";
  const loaded = new Map<string, number>();
  const pending = new Map<string, number>();
  return {
    sync(next: string, force = false, clear = false): boolean {
      if (!force && signature === next) return false;
      signature = next; generation++; loaded.clear(); pending.clear();
      // Filters refresh existing cards in place; a different library must never
      // display the previous library's images while its requests are pending.
      adapter.clear(!clear);
      return true;
    },
    async load(key: string, more = false): Promise<void> {
      const current = adapter.read(key);
      const fresh = loaded.get(key) === generation;
      if (pending.get(key) === generation || (!more && fresh && current && !current.error)
        || (more && fresh && current && (current.exhausted || current.rows.length >= current.totalCount))) return;
      const request = generation;
      pending.set(key, request);
      const previous = current?.rows ?? [];
      const append = more && fresh;
      adapter.write(key, { rows: previous, totalCount: current?.totalCount ?? 0, loading: true, error: null, stale: !fresh });
      try {
        const page = await adapter.fetch(key, append ? previous.length : 0, 200);
        if (request !== generation) return;
        const rows = append ? [...previous, ...page.rows] : page.rows;
        loaded.set(key, request);
        adapter.write(key, { rows, totalCount: page.totalCount, loading: false, error: null,
          exhausted: page.hasMore === false || page.rows.length === 0 || rows.length >= page.totalCount });
      } catch (error) {
        if (request === generation) adapter.write(key, {
          rows: previous, totalCount: current?.totalCount ?? 0, loading: false, error: errorText(error), stale: !fresh,
        });
      } finally { if (pending.get(key) === request) pending.delete(key); }
    },
  };
}
