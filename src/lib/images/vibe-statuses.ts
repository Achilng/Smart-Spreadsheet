import { getRowVibeStatus } from "../api";

const MAX_CONCURRENT_REQUESTS = 4;
const MAX_CACHED_STATUSES = 1000;

interface CachedStatus {
  count: number | null;
  lastUsed: number;
}

interface PendingStatus {
  generation: number;
  promise: Promise<number | null>;
}

interface QueuedRequest {
  rowId: number;
  generation: number;
  resolve: (count: number | null) => void;
  reject: (error: unknown) => void;
}

class VibeStatusLoader {
  readonly #cache = new Map<number, CachedStatus>();
  readonly #pending = new Map<number, PendingStatus>();
  readonly #queue: QueuedRequest[] = [];
  #activeRequests = 0;
  #clock = 0;
  #generation = 0;

  load(rowId: number): Promise<number | null> {
    const cached = this.#cache.get(rowId);
    if (cached) {
      cached.lastUsed = ++this.#clock;
      return Promise.resolve(cached.count);
    }
    const pending = this.#pending.get(rowId);
    if (pending) {
      return pending.promise;
    }

    const generation = this.#generation;
    const promise = new Promise<number | null>((resolve, reject) => {
      this.#queue.push({ rowId, generation, resolve, reject });
      this.#drain();
    });
    this.#pending.set(rowId, { generation, promise });
    return promise;
  }

  retain(rowIds: ReadonlySet<number>): void {
    const retained: QueuedRequest[] = [];
    for (const request of this.#queue.splice(0)) {
      if (rowIds.has(request.rowId)) {
        retained.push(request);
      } else {
        const pending = this.#pending.get(request.rowId);
        if (pending?.generation === request.generation) {
          this.#pending.delete(request.rowId);
        }
        request.reject(new Error("VIBE 状态已离开可视区域"));
      }
    }
    this.#queue.push(...retained);
  }

  /** 资料库内容变化后行 ID 可能复用，旧状态不得继续显示。 */
  clear(): void {
    this.#generation += 1;
    for (const request of this.#queue.splice(0)) {
      request.reject(new Error("VIBE 状态缓存已重置"));
    }
    this.#cache.clear();
    this.#pending.clear();
  }

  #drain(): void {
    while (
      this.#activeRequests < MAX_CONCURRENT_REQUESTS &&
      this.#queue.length > 0
    ) {
      const request = this.#queue.shift();
      if (!request) {
        return;
      }
      this.#activeRequests += 1;
      void this.#run(request);
    }
  }

  async #run(request: QueuedRequest): Promise<void> {
    try {
      const count = await getRowVibeStatus(request.rowId);
      if (request.generation !== this.#generation) {
        request.reject(new Error("VIBE 状态缓存已重置"));
        return;
      }
      this.#cache.set(request.rowId, { count, lastUsed: ++this.#clock });
      this.#evict();
      request.resolve(count);
    } catch (error) {
      request.reject(error);
    } finally {
      const pending = this.#pending.get(request.rowId);
      if (pending?.generation === request.generation) {
        this.#pending.delete(request.rowId);
      }
      this.#activeRequests -= 1;
      this.#drain();
    }
  }

  #evict(): void {
    while (this.#cache.size > MAX_CACHED_STATUSES) {
      let oldestRowId: number | null = null;
      let oldestUse = Number.POSITIVE_INFINITY;
      for (const [rowId, status] of this.#cache) {
        if (status.lastUsed < oldestUse) {
          oldestUse = status.lastUsed;
          oldestRowId = rowId;
        }
      }
      if (oldestRowId === null) {
        return;
      }
      this.#cache.delete(oldestRowId);
    }
  }
}

/** 全应用共享的 VIBE 引用数加载器（可见区限流 + LRU 缓存）。 */
export const vibeStatuses = new VibeStatusLoader();
