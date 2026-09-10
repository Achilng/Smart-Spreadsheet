import assert from "node:assert/strict";
import test from "node:test";
import {
  clearScrollPositions, prepareFilterScrollPositions, restoreScrollPosition,
  savedScrollPosition, saveScrollPosition,
  filterReturnRange, rememberVisibleRange,
} from "../src/lib/stores/view-state.ts";

test("return range keeps the original viewport across searches and invalidates with its position", () => {
  clearScrollPositions();
  saveScrollPosition("gallery", 32000);
  rememberVisibleRange("gallery", 390, 421);
  prepareFilterScrollPositions(true)();
  rememberVisibleRange("gallery", 0, 31);
  prepareFilterScrollPositions(true)();
  assert.deepEqual(filterReturnRange("gallery"), { first: 390, last: 421 });
  prepareFilterScrollPositions(false)();
  prepareFilterScrollPositions(true)();
  assert.deepEqual(filterReturnRange("gallery"), { first: 390, last: 421 });
  clearScrollPositions();
  assert.equal(filterReturnRange("gallery"), undefined);
});

test("clearing the last filter restores each view's original position across repeated conditions", () => {
  clearScrollPositions();
  saveScrollPosition("gallery", 15000);
  saveScrollPosition("table", 6400);
  prepareFilterScrollPositions(true)();
  assert.equal(savedScrollPosition("gallery"), 0);
  saveScrollPosition("gallery", 200);
  prepareFilterScrollPositions(true)();
  prepareFilterScrollPositions(false)();
  assert.equal(savedScrollPosition("gallery"), 15000);
  assert.equal(savedScrollPosition("table"), 6400);
  saveScrollPosition("gallery", 18000);
  prepareFilterScrollPositions(true)();
  prepareFilterScrollPositions(false)();
  assert.equal(savedScrollPosition("gallery"), 18000);
});

test("failed or superseded requests do not consume the pre-filter position", () => {
  clearScrollPositions();
  saveScrollPosition("gallery", 23000);
  prepareFilterScrollPositions(true); // 请求未完成即清除搜索
  prepareFilterScrollPositions(false)();
  assert.equal(savedScrollPosition("gallery"), 23000);
  prepareFilterScrollPositions(true)();
  prepareFilterScrollPositions(false); // 清除失败或被新筛选取代
  prepareFilterScrollPositions(true)();
  prepareFilterScrollPositions(false)();
  assert.equal(savedScrollPosition("gallery"), 23000);
});

test("sort or library changes invalidate the old return position", () => {
  clearScrollPositions();
  saveScrollPosition("gallery", 23000);
  prepareFilterScrollPositions(true)();
  clearScrollPositions(true);
  saveScrollPosition("gallery", 150);
  prepareFilterScrollPositions(true)();
  prepareFilterScrollPositions(false)();
  assert.equal(savedScrollPosition("gallery"), 0);
});

test("restoring zero really returns the viewport to the top", () => {
  clearScrollPositions();
  const el = { scrollTop: 700, isConnected: true } as HTMLElement;
  let applied = -1;
  restoreScrollPosition(el, "gallery", 60, top => { applied = top; });
  assert.equal(el.scrollTop, 0);
  assert.equal(applied, 0);
});

test("a pending animation frame cannot restore an obsolete result set", () => {
  clearScrollPositions();
  saveScrollPosition("gallery", 15000);
  const frames: FrameRequestCallback[] = [];
  const original = globalThis.requestAnimationFrame;
  globalThis.requestAnimationFrame = callback => { frames.push(callback); return frames.length; };
  try {
    let top = 0;
    const el = {
      isConnected: true,
      get scrollTop() { return top; },
      set scrollTop(value: number) { top = Math.min(value, 100); },
    } as HTMLElement;
    restoreScrollPosition(el, "gallery");
    assert.equal(top, 100);
    prepareFilterScrollPositions(true)();
    top = 0;
    frames.shift()?.(0);
    assert.equal(top, 0);
  } finally {
    globalThis.requestAnimationFrame = original;
  }
});
