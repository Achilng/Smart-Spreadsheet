import { getRowThumbnail } from "../api";

const MAX_CONCURRENT_REQUESTS = 8;
const MAX_CACHED_URLS = 480;

interface CachedThumbnail {
  url: string;
  lastUsed: number;
}

interface QueuedRequest {
  rowId: number;
  resolve: (url: string) => void;
  reject: (error: unknown) => void;
}

export class ThumbnailLoader {
  readonly #cache = new Map<number, CachedThumbnail>();
  readonly #pending = new Map<number, Promise<string>>();
  readonly #queue: QueuedRequest[] = [];
  #activeRequests = 0;
  #clock = 0;
  #disposed = false;

  load(rowId: number): Promise<string> {
    if (this.#disposed) {
      return Promise.reject(new Error("缩略图加载器已关闭"));
    }
    const cached = this.#cache.get(rowId);
    if (cached) {
      cached.lastUsed = ++this.#clock;
      return Promise.resolve(cached.url);
    }
    const pending = this.#pending.get(rowId);
    if (pending) {
      return pending;
    }

    const request = new Promise<string>((resolve, reject) => {
      this.#queue.push({ rowId, resolve, reject });
      this.#drain();
    });
    this.#pending.set(rowId, request);
    return request;
  }

  retain(rowIds: ReadonlySet<number>): void {
    const retained: QueuedRequest[] = [];
    for (const request of this.#queue.splice(0)) {
      if (rowIds.has(request.rowId)) {
        retained.push(request);
      } else {
        this.#pending.delete(request.rowId);
        request.reject(new Error("缩略图已离开可视区域"));
      }
    }
    this.#queue.push(...retained);
  }

  dispose(): void {
    this.#disposed = true;
    for (const request of this.#queue.splice(0)) {
      request.reject(new Error("缩略图加载器已关闭"));
    }
    for (const thumbnail of this.#cache.values()) {
      URL.revokeObjectURL(thumbnail.url);
    }
    this.#cache.clear();
    this.#pending.clear();
  }

  /** 清空缓存与排队请求；工作簿替换后行 ID 可能重复，必须丢弃旧图。 */
  clear(): void {
    for (const request of this.#queue.splice(0)) {
      this.#pending.delete(request.rowId);
      request.reject(new Error("缩略图缓存已重置"));
    }
    for (const thumbnail of this.#cache.values()) {
      URL.revokeObjectURL(thumbnail.url);
    }
    this.#cache.clear();
  }

  #drain(): void {
    while (
      !this.#disposed &&
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
      const response = await getRowThumbnail(request.rowId);
      const buffer = binaryBuffer(response);
      if (this.#disposed) {
        request.reject(new Error("缩略图加载器已关闭"));
        return;
      }
      const url = URL.createObjectURL(new Blob([buffer], { type: "image/png" }));
      this.#cache.set(request.rowId, { url, lastUsed: ++this.#clock });
      this.#evict();
      request.resolve(url);
    } catch (error) {
      request.reject(error);
    } finally {
      this.#pending.delete(request.rowId);
      this.#activeRequests -= 1;
      this.#drain();
    }
  }

  #evict(): void {
    while (this.#cache.size > MAX_CACHED_URLS) {
      let oldestRowId: number | null = null;
      let oldestUse = Number.POSITIVE_INFINITY;
      for (const [rowId, thumbnail] of this.#cache) {
        if (thumbnail.lastUsed < oldestUse) {
          oldestUse = thumbnail.lastUsed;
          oldestRowId = rowId;
        }
      }
      if (oldestRowId === null) {
        return;
      }
      const thumbnail = this.#cache.get(oldestRowId);
      if (thumbnail) {
        URL.revokeObjectURL(thumbnail.url);
      }
      this.#cache.delete(oldestRowId);
    }
  }
}

export function binaryBuffer(value: ArrayBuffer | Uint8Array | number[]): ArrayBuffer {
  if (value instanceof ArrayBuffer) {
    return value;
  }
  return Uint8Array.from(value).buffer;
}
