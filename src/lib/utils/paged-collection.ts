export interface CollectionPage<T> { items: T[]; total: number }

/** On-demand page cache; a new query invalidates every older response. */
export class PagedCollection<T> {
  total = 0;
  ready = false;
  readonly pages = new Map<number, T[]>();
  readonly pending = new Set<number>();
  readonly errors = new Map<number, unknown>();
  private generation = 0;
  private fetchPage: ((offset: number) => Promise<CollectionPage<T>>) | null = null;
  readonly pageSize: number;
  private changed: () => void;

  constructor(pageSize: number, changed: () => void) { this.pageSize = pageSize; this.changed = changed; }

  reset(fetchPage: (offset: number) => Promise<CollectionPage<T>>) {
    this.generation++;
    this.fetchPage = fetchPage;
    this.total = 0;
    this.ready = false;
    this.pages.clear(); this.pending.clear(); this.errors.clear();
    this.changed();
  }

  get(index: number): T | undefined {
    return this.pages.get(Math.floor(index / this.pageSize))?.[index % this.pageSize];
  }

  async load(pageIndex: number): Promise<void> {
    if (!this.fetchPage || pageIndex < 0 || this.pages.has(pageIndex) || this.pending.has(pageIndex) || this.errors.has(pageIndex)) return;
    if (this.ready && pageIndex * this.pageSize >= this.total) return;
    const generation = this.generation;
    this.pending.add(pageIndex);
    this.changed();
    try {
      const page = await this.fetchPage(pageIndex * this.pageSize);
      if (generation !== this.generation) return;
      this.pages.set(pageIndex, page.items);
      this.total = page.total;
      this.ready = true;
    } catch (error) {
      if (generation === this.generation) this.errors.set(pageIndex, error);
    } finally {
      if (generation === this.generation) { this.pending.delete(pageIndex); this.changed(); }
    }
  }

  retry() {
    const failed = [...this.errors.keys()];
    this.errors.clear();
    for (const page of failed) void this.load(page);
  }

  dispose() { this.generation++; this.fetchPage = null; }
}
