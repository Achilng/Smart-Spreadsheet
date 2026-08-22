import {
  compareSample,
  compareSectionRows,
  type CompareSample,
  type CompareSectionKind,
} from "../api/compare";
import type { RowRecord } from "../api/rows";

/** 对比页每个分区/模型组的分页大小。 */
export const COMPARE_PAGE_SIZE = 24;

interface SectionPageState {
  rows: RowRecord[];
  totalCount: number;
  hasMore: boolean;
  loading: boolean;
  error: string | null;
}

function emptyPage(): SectionPageState {
  return { rows: [], totalCount: 0, hasMore: false, loading: false, error: null };
}

/** 模型分组分区的状态键：无分组用 kind 本身，分组用 `kind::模型名`。 */
export function comparePageKey(kind: CompareSectionKind, model: string | null): string {
  return model == null ? kind : `${kind}::${model}`;
}

class CompareStore {
  sampleRowId = $state<number | null>(null);
  sample = $state<CompareSample | null>(null);
  sampleLoading = $state(false);
  sampleError = $state<string | null>(null);
  /** 并排对比的目标行；非空时窗口切换到并排视图。 */
  compareTarget = $state<RowRecord | null>(null);

  pages = $state<Record<string, SectionPageState>>({});

  async setSample(rowId: number | null): Promise<void> {
    if (rowId === this.sampleRowId) return;
    this.sampleRowId = rowId;
    this.sample = null;
    this.sampleError = null;
    this.sampleLoading = rowId != null;
    this.compareTarget = null;
    this.pages = {};
    if (rowId == null) return;
    try {
      const sample = await compareSample(rowId);
      if (this.sampleRowId !== rowId) return;
      this.sample = sample;
    } catch (error) {
      if (this.sampleRowId === rowId) {
        this.sampleError = error instanceof Error ? error.message : String(error);
      }
    } finally {
      if (this.sampleRowId === rowId) {
        this.sampleLoading = false;
      }
    }
  }

  /** 分区摘要；样本未加载完成时为 undefined。 */
  sectionSummary(kind: CompareSectionKind) {
    return this.sample?.sections.find(section => section.kind === kind);
  }

  page(kind: CompareSectionKind, model: string | null): SectionPageState {
    return this.pages[comparePageKey(kind, model)] ?? emptyPage();
  }

  /** 首次渲染分区/分组时加载第一页；已尝试过（含失败）时是空操作。 */
  async ensure(kind: CompareSectionKind, model: string | null): Promise<void> {
    if (this.pages[comparePageKey(kind, model)]) return;
    await this.loadPage(kind, model, 0);
  }

  /** 失败后的“重试”：强制重新加载第一页。 */
  async reload(kind: CompareSectionKind, model: string | null): Promise<void> {
    await this.loadPage(kind, model, 0);
  }

  async loadMore(kind: CompareSectionKind, model: string | null): Promise<void> {
    const state = this.pages[comparePageKey(kind, model)];
    if (!state || state.loading || !state.hasMore) return;
    await this.loadPage(kind, model, state.rows.length);
  }

  private async loadPage(
    kind: CompareSectionKind,
    model: string | null,
    offset: number,
  ): Promise<void> {
    const rowId = this.sampleRowId;
    if (rowId == null) return;
    const key = comparePageKey(kind, model);
    const current = this.pages[key] ?? emptyPage();
    this.pages[key] = { ...current, loading: true, error: null };
    try {
      const page = await compareSectionRows(
        rowId,
        kind,
        model,
        offset,
        COMPARE_PAGE_SIZE,
      );
      // 样本已切换时丢弃过期结果，避免混入上一张图的分区
      if (this.sampleRowId !== rowId) return;
      const previous = this.pages[key] ?? emptyPage();
      this.pages[key] = {
        rows: offset === 0 ? page.rows : [...previous.rows, ...page.rows],
        totalCount: page.totalCount,
        hasMore: page.hasMore,
        loading: false,
        error: null,
      };
    } catch (error) {
      if (this.sampleRowId !== rowId) return;
      const previous = this.pages[key] ?? emptyPage();
      this.pages[key] = {
        ...previous,
        loading: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }
}

export const compareStore = new CompareStore();
