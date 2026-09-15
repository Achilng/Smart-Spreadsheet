import assert from "node:assert/strict";
import test from "node:test";
import { createNavigationHistory } from "../src/lib/utils/navigation-history.ts";

test("back and forward retain each entry's latest browsing position", () => {
  const history = createNavigationHistory<string, number>();
  history.reset({ route: "all", position: 0 });
  history.save(32000);
  history.record({ route: "tag", position: 0 });
  history.save(1500);
  history.record({ route: "artist", position: 0 });
  assert.deepEqual(history.go(-1), { route: "tag", position: 1500 });
  assert.deepEqual(history.go(-1), { route: "all", position: 32000 });
  assert.equal(history.go(-1), undefined);
  assert.deepEqual(history.go(1), { route: "tag", position: 1500 });
});

test("one search gesture merges queries but preserves the page before typing", () => {
  const history = createNavigationHistory<string, number>();
  history.reset({ route: "all", position: 30000 });
  for (const route of ["a", "ar", "artist"]) history.record({ route, position: 0 }, "search:1");
  assert.equal(history.length, 2);
  assert.equal(history.go(-1)?.route, "all");
  assert.equal(history.go(1)?.route, "artist");
  history.record({ route: "new search", position: 0 }, "search:2");
  assert.equal(history.go(-1)?.route, "artist");
});

test("new navigation after back discards the forward branch, including a search", () => {
  const history = createNavigationHistory<string, number>();
  history.reset({ route: "all", position: 100 });
  history.record({ route: "a", position: 200 }, "search:1");
  history.record({ route: "table", position: 300 });
  history.go(-1);
  history.record({ route: "ab", position: 0 }, "search:1");
  assert.equal(history.forward, undefined);
  assert.equal(history.go(-1)?.route, "a");
});

test("erasing a continuous search back to its origin does not leave duplicate history", () => {
  const history = createNavigationHistory<string, number>();
  history.reset({ route: "all", position: 32000 });
  history.record({ route: "a", position: 0 }, "search:1");
  history.record({ route: "all", position: 32000 }, "search:1");
  assert.equal(history.length, 1);
  assert.equal(history.back, undefined);
  history.record({ route: "b", position: 0 }, "search:1");
  assert.equal(history.length, 2);
  assert.equal(history.go(-1)?.position, 32000);
});

test("detail/scroll updates and identical routes never create an entry; library reset clears both directions", () => {
  const history = createNavigationHistory<string, number>(3);
  history.reset({ route: "all", position: 0 });
  history.save(12);
  history.record({ route: "all", position: 15 });
  assert.equal(history.length, 1);
  for (const route of ["a", "b", "c", "d"]) history.record({ route, position: 0 });
  assert.equal(history.length, 3);
  history.go(-1);
  history.reset({ route: "new library", position: 0 });
  assert.equal(history.back, undefined);
  assert.equal(history.forward, undefined);
});
