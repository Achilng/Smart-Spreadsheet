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
  consumers: Set<symbol>;
  persistent: boolean;
}

type ImageFetcher = (rowId: number) => Promise<ArrayBuffer>;

/**
 * 取消类失败（离开可视区域、缓存重置、请求过期）。
 * 与真实加载失败区分开：UI 对取消不应显示“不可用”，重新进入视口会自动重试。
 */
export class ImageLoadCancelled extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ImageLoadCancelled";
  }
}

export function isImageLoadCancelled(error: unknown): boolean {
  return error instanceof ImageLoadCancelled;
}

export class ImageLoader {
  readonly #cache = new Map<number, CachedImage>();
  readonly #pending = new Map<number, QueuedRequest>();
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

  /** Discard only the decoded URL that failed, without flushing other images. */
  invalidate(rowId: number, expectedUrl: string): void {
    const cached = this.#cache.get(rowId);
    if (!cached || cached.url !== expectedUrl) return;
    URL.revokeObjectURL(cached.url);
    this.#cache.delete(rowId);
  }

  load(rowId: number, priority = false, signal?: AbortSignal): Promise<string> {
    if (signal?.aborted) return Promise.reject(new ImageLoadCancelled("图片已离开可视区域"));
    if (this.#disposed) {
      return Promise.reject(new Error("图片加载器已关闭"));
    }
    const cached = this.cached(rowId);
    if (cached) {
      return Promise.resolve(cached);
    }
    const pending = this.#pending.get(rowId);
    if (pending) {
      return this.#subscribe(pending, signal);
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
      consumers: new Set(),
      persistent: false,
    };
    this.#pending.set(rowId, request);
    if (priority) {
      this.#queue.unshift(request);
    } else {
      this.#queue.push(request);
    }
    const subscribed = this.#subscribe(request, signal);
    this.#drain();
    return subscribed;
  }

  // Each component owns its demand. Unmounting one copy must not cancel another
  // mounted copy, and queued requests with no remaining consumers never decode.
  #subscribe(request: QueuedRequest, signal?: AbortSignal): Promise<string> {
    if (!signal) {
      request.persistent = true;
      return request.promise;
    }
    const token = Symbol();
    request.consumers.add(token);
    return new Promise((resolve, reject) => {
      const cleanup = () => {
        signal.removeEventListener("abort", abort);
        request.consumers.delete(token);
      };
      const abort = () => {
        cleanup();
        const error = new ImageLoadCancelled("图片已离开可视区域");
        reject(error);
        if (!request.persistent && request.consumers.size === 0) {
          const index = this.#queue.indexOf(request);
          if (index >= 0) {
            this.#queue.splice(index, 1);
            if (this.#pending.get(request.rowId) === request) this.#pending.delete(request.rowId);
            request.reject(error);
          }
        }
      };
      signal.addEventListener("abort", abort, { once: true });
      request.promise.then(url => { cleanup(); resolve(url); }, error => { cleanup(); reject(error); });
    });
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
      if (rowIds.has(request.rowId) || request.consumers.size > 0) {
        retained.push(request);
      } else {
        this.#pending.delete(request.rowId);
        request.reject(new ImageLoadCancelled("缩略图已离开可视区域"));
      }
    }
    this.#queue.push(...retained);
  }

  dispose(): void {
    this.#disposed = true;
    this.#generation += 1;
    for (const request of this.#queue.splice(0)) {
      request.reject(new ImageLoadCancelled("图片加载器已关闭"));
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
      request.reject(new ImageLoadCancelled("图片缓存已重置"));
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
        request.reject(new ImageLoadCancelled("图片请求已过期"));
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
      if (this.#pending.get(request.rowId) === request) {
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
