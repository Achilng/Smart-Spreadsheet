import assert from "node:assert/strict";
import test from "node:test";
import { createRequestQueue } from "../src/lib/utils/request-queue.ts";

test("slow search runs once and only the newest queued generation reaches the backend", async () => {
  const enqueue = createRequestQueue();
  const calls: string[] = [];
  let generation = 0;
  let finish!: (value: string) => void;
  const first = enqueue(() => true, () => {
    calls.push("first");
    return new Promise<string>(resolve => { finish = resolve; });
  });
  await Promise.resolve();
  const obsolete = enqueue(() => generation === 0, async () => { calls.push("obsolete"); return "old"; });
  generation += 1;
  const latest = enqueue(() => generation === 1, async () => { calls.push("latest"); return "new"; });
  const nextPage = enqueue(() => generation === 1, async () => { calls.push("page"); return "page"; });
  await Promise.resolve();
  assert.deepEqual(calls, ["first"]);
  finish("first result");
  assert.deepEqual(await Promise.all([first, obsolete, latest, nextPage]), ["first result", undefined, "new", "page"]);
  assert.deepEqual(calls, ["first", "latest", "page"]);
});

test("search errors propagate but do not block the next request", async () => {
  const enqueue = createRequestQueue();
  const failed = enqueue(() => true, async () => { throw new Error("failed search"); });
  const next = enqueue(() => true, async () => 42);
  await assert.rejects(failed, /failed search/);
  assert.equal(await next, 42);
});
