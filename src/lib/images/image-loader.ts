interface CachedImage {
  url: string;
  lastUsed: number;
}

interface QueuedRequest {
  rowId: number;
  generation: number;
  promise: Promise<string>;
  resolve: (url: string) => void;
  reject: (error: unknown) => void;
}

type ImageFetcher = (rowId: number) => Promise<ArrayBuffer>;

export class ImageLoader {
  readonly #cache = new Map<number, CachedImage>();
  readonly #pending = new Map<number, Promise<string>>();
  readonly #queue: QueuedRequest[] = [];
  readonly #fetch: ImageFetcher;
  readonly #maxConcurrentRequests: number;
  readonly #maxCachedUrls: number;
  readonly #mimeType: string | undefined;
  #activeRequests = 0;
  #clock = 0;
  #generation = 0;
  #disposed = false;

  constructor(
    fetch: ImageFetcher,
    maxConcurrentRequests: number,
    maxCachedUrls: number,
    mimeType?: string,
  ) {
    this.#fetch = fetch;
    this.#maxConcurrentRequests = maxConcurrentRequests;
    this.#maxCachedUrls = maxCachedUrls;
    this.#mimeType = mimeType;
  }

  cached(rowId: number): string | null {
    const cached = this.#cache.get(rowId);
    if (!cached) {
      return null;
    }
    cached.lastUsed = ++this.#clock;
    return cached.url;
  }

  load(rowId: number, priority = false): Promise<string> {
    if (this.#disposed) {
      return Promise.reject(new Error("图片加载器已关闭"));
    }
    const cached = this.cached(rowId);
    if (cached) {
      return Promise.resolve(cached);
    }
    const pending = this.#pending.get(rowId);
    if (pending) {
      return pending;
    }

    let resolveRequest!: (url: string) => void;
    let rejectRequest!: (error: unknown) => void;
    const promise = new Promise<string>((resolve, reject) => {
      resolveRequest = resolve;
      rejectRequest = reject;
    });
    const request: QueuedRequest = {
      rowId,
      generation: this.#generation,
      promise,
      resolve: resolveRequest,
      reject: rejectRequest,
    };
    this.#pending.set(rowId, promise);
    if (priority) {
      this.#queue.unshift(request);
    } else {
      this.#queue.push(request);
    }
    this.#drain();
    return promise;
  }

  retain(rowIds: ReadonlySet<number>): void {
    for (const rowId of rowIds) {
      const cached = this.#cache.get(rowId);
      if (cached) {
        cached.lastUsed = ++this.#clock;
      }
    }
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
    this.#generation += 1;
    for (const request of this.#queue.splice(0)) {
      request.reject(new Error("图片加载器已关闭"));
    }
    for (const image of this.#cache.values()) {
      URL.revokeObjectURL(image.url);
    }
    this.#cache.clear();
    this.#pending.clear();
  }

  /** 清空缓存与排队请求；工作簿替换后行 ID 可能重复，必须丢弃旧图。 */
  clear(): void {
    this.#generation += 1;
    for (const request of this.#queue.splice(0)) {
      request.reject(new Error("图片缓存已重置"));
    }
    for (const image of this.#cache.values()) {
      URL.revokeObjectURL(image.url);
    }
    this.#cache.clear();
    this.#pending.clear();
  }

  #drain(): void {
    while (
      !this.#disposed &&
      this.#activeRequests < this.#maxConcurrentRequests &&
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
      const response = await this.#fetch(request.rowId);
      const buffer = binaryBuffer(response);
      if (this.#disposed || request.generation !== this.#generation) {
        request.reject(new Error("图片请求已过期"));
        return;
      }
      const blob = this.#mimeType
        ? new Blob([buffer], { type: this.#mimeType })
        : new Blob([buffer]);
      const url = URL.createObjectURL(blob);
      this.#cache.set(request.rowId, { url, lastUsed: ++this.#clock });
      this.#evict();
      request.resolve(url);
    } catch (error) {
      request.reject(error);
    } finally {
      if (this.#pending.get(request.rowId) === request.promise) {
        this.#pending.delete(request.rowId);
      }
      this.#activeRequests -= 1;
      this.#drain();
    }
  }

  #evict(): void {
    while (this.#cache.size > this.#maxCachedUrls) {
      let oldestRowId: number | null = null;
      let oldestUse = Number.POSITIVE_INFINITY;
      for (const [rowId, image] of this.#cache) {
        if (image.lastUsed < oldestUse) {
          oldestUse = image.lastUsed;
          oldestRowId = rowId;
        }
      }
      if (oldestRowId === null) {
        return;
      }
      const image = this.#cache.get(oldestRowId);
      if (image) {
        URL.revokeObjectURL(image.url);
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
