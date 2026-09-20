import assert from "node:assert/strict";
import test from "node:test";
import { PagedCollection, type CollectionPage } from "../src/lib/utils/paged-collection.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => resolve = done);
  return { promise, resolve };
}

test("scrolling loads multiple ranges without losing earlier cards or repeating requests", async () => {
  const calls: number[] = [];
  const rows = Array.from({ length: 125 }, (_, id) => ({ id }));
  const cache = new PagedCollection<{ id: number }>(48, () => {});
  cache.reset(async offset => { calls.push(offset); return { items: rows.slice(offset, offset + 48), total: rows.length }; });
  await cache.load(0);
  await Promise.all([cache.load(1), cache.load(1), cache.load(2)]);
  await cache.load(0);
  await cache.load(3);
  assert.deepEqual(calls, [0, 48, 96]);
  assert.equal(cache.total, 125);
  assert.equal(cache.get(47)?.id, 47);
  assert.equal(cache.get(48)?.id, 48);
  assert.equal(cache.get(124)?.id, 124);
  assert.equal(cache.get(125), undefined);
});

test("slow old searches cannot overwrite results or clear a new pending request", async () => {
  const old = deferred<CollectionPage<string>>();
  const next = deferred<CollectionPage<string>>();
  const cache = new PagedCollection<string>(48, () => {});
  cache.reset(() => old.promise);
  const oldLoad = cache.load(0);
  cache.reset(() => next.promise);
  const newLoad = cache.load(0);
  old.resolve({ items: ["old"], total: 500 });
  await oldLoad;
  assert.equal(cache.get(0), undefined);
  assert.equal(cache.pending.has(0), true);
  next.resolve({ items: ["new"], total: 1 });
  await newLoad;
  assert.equal(cache.get(0), "new");
  assert.equal(cache.total, 1);
});

test("failed later ranges retain loaded cards and retry only on explicit request", async () => {
  let failures = 0;
  const cache = new PagedCollection<number>(2, () => {});
  cache.reset(async offset => {
    if (offset === 2 && failures++ === 0) throw new Error("temporary failure");
    return { items: [offset, offset + 1], total: 4 };
  });
  await cache.load(0);
  await cache.load(1);
  await cache.load(1);
  assert.equal(failures, 1);
  assert.equal(cache.get(0), 0);
  assert.equal(cache.errors.size, 1);
  cache.retry();
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.equal(cache.get(2), 2);
  assert.equal(cache.errors.size, 0);
});

test("empty results stop fetching and disposal ignores pending responses", async () => {
  let calls = 0;
  const cache = new PagedCollection<number>(48, () => {});
  cache.reset(async () => { calls++; return { items: [], total: 0 }; });
  await cache.load(0);
  await cache.load(1);
  assert.equal(cache.ready, true);
  assert.equal(calls, 1);
  const pending = deferred<CollectionPage<number>>();
  cache.reset(() => pending.promise);
  const loading = cache.load(0);
  cache.dispose();
  pending.resolve({ items: [1], total: 1 });
  await loading;
  assert.equal(cache.get(0), undefined);
});
