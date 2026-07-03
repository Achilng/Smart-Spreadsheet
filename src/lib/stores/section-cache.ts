import { errorText } from "./app-state.svelte";
import type { SectionMembers } from "./section-types";

export const MEMBERS_PAGE = 200;

export interface PageResult {
  rows: { id: number }[];
  totalCount: number;
}

/**
 * 创建一个签名失效 + 分页加载 + 防竞态的成员缓存。
 * 用于 group-browse-store 和 duplicate-browse-store 的成员加载。
 */
export function createSectionCache(
  cache: () => Record<string, SectionMembers>,
  setCache: (key: string, value: SectionMembers) => void,
  getSignature: () => string,
) {
  async function ensure(
    key: string,
    fetchPage: (offset: number, limit: number) => Promise<PageResult>,
  ): Promise<void> {
    if (cache()[key]) return;
    const sig = getSignature();
    setCache(key, { rows: [], totalCount: 0, loading: true, error: null });
    try {
      const page = await fetchPage(0, MEMBERS_PAGE);
      if (sig !== getSignature()) return;
      setCache(key, { rows: page.rows as any[], totalCount: page.totalCount, loading: false, error: null });
    } catch (e) {
      if (sig !== getSignature()) return;
      setCache(key, { rows: [], totalCount: 0, loading: false, error: errorText(e) });
    }
  }

  async function loadMore(
    key: string,
    fetchPage: (offset: number, limit: number) => Promise<PageResult>,
  ): Promise<void> {
    const data = cache()[key];
    if (!data || data.loading) return;
    const sig = getSignature();
    data.loading = true;
    try {
      const page = await fetchPage(data.rows.length, MEMBERS_PAGE);
      if (sig !== getSignature()) return;
      data.rows = [...data.rows, ...page.rows as any[]];
      data.totalCount = page.totalCount;
      data.loading = false;
      data.error = null;
    } catch (e) {
      if (sig !== getSignature()) return;
      data.loading = false;
      data.error = errorText(e);
    }
  }

  return { ensure, loadMore };
}
