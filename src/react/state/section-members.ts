import type { RowRecord } from "../../lib/api";
import { errorText } from "../../lib/utils/format";

export interface SectionMembers {
  rows: RowRecord[]; totalCount: number; loading: boolean; error: string | null; exhausted?: boolean;
}

/** Append-only album pagination. Each owner supplies its own query and store. */
export function createSectionPager(adapter: {
  read: (key: string) => SectionMembers | undefined;
  write: (key: string, value: SectionMembers) => void;
  clear: () => void;
  fetch: (key: string, offset: number, limit: number) => Promise<{ rows: RowRecord[]; totalCount: number; hasMore?: boolean }>;
}) {
  let generation = 0, signature = "";
  return {
    sync(next: string, force = false): boolean {
      if (!force && signature === next) return false;
      signature = next; generation++; adapter.clear(); return true;
    },
    async load(key: string, more = false): Promise<void> {
      const current = adapter.read(key);
      if (current?.loading || (!more && current && !current.error)
        || (more && current && (current.exhausted || current.rows.length >= current.totalCount))) return;
      const request = generation;
      const previous = current?.rows ?? [];
      adapter.write(key, { rows: previous, totalCount: current?.totalCount ?? 0, loading: true, error: null });
      try {
        const page = await adapter.fetch(key, more ? previous.length : 0, 200);
        if (request !== generation) return;
        const rows = more ? [...previous, ...page.rows] : page.rows;
        adapter.write(key, { rows, totalCount: page.totalCount, loading: false, error: null,
          exhausted: page.hasMore === false || page.rows.length === 0 || rows.length >= page.totalCount });
      } catch (error) {
        if (request === generation) adapter.write(key, {
          rows: previous, totalCount: current?.totalCount ?? 0, loading: false, error: errorText(error),
        });
      }
    },
  };
}
