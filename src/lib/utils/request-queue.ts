/** Keep at most one request in flight; obsolete queued work never reaches the backend. */
export function createRequestQueue() {
  let tail: Promise<unknown> = Promise.resolve();
  return function enqueue<T>(isCurrent: () => boolean, request: () => Promise<T>): Promise<T | undefined> {
    const result = tail.then(() => isCurrent() ? request() : undefined);
    // A failed request must not prevent later searches from running.
    tail = result.catch(() => undefined);
    return result;
  };
}
