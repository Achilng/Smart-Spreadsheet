import { test } from "node:test";
import assert from "node:assert/strict";
import { pointerSort, type SortPreview } from "../src/lib/utils/pointer-sort.ts";

function fixture(count = 3) {
  const callbacks = new Map<number, FrameRequestCallback>();
  let frame = 0;
  const view = Object.assign(new EventTarget(), {
    requestAnimationFrame(callback: FrameRequestCallback) { callbacks.set(++frame, callback); return frame; },
    cancelAnimationFrame(id: number) { callbacks.delete(id); },
  });
  let captured: number | null = null, scrollTop = 0;
  const node = Object.assign(new EventTarget(), {
    ownerDocument: { defaultView: view },
    setPointerCapture(id: number) { captured = id; },
    hasPointerCapture(id: number) { return captured === id; },
    releasePointerCapture() { captured = null; },
    getBoundingClientRect() { return { top: 0, bottom: 108 }; },
    contains(row: unknown) { return rows.includes(row as typeof rows[number]); },
    querySelectorAll() { return rows; },
  });
  Object.defineProperty(node, "scrollTop", { get: () => scrollTop, set: value => scrollTop = Math.max(0, Math.min(count * 36 - 108, value)) });
  const rows = Array.from({ length: count }, (_, index) => ({
    dataset: { sortKey: `v${index}` },
    getBoundingClientRect() { return { top: index * 36 - scrollTop, height: 32, left: 10, width: 200 }; },
  }));
  const moves: [string, number][] = [];
  let preview: SortPreview | null = null;
  const options = { onpreview: (value: SortPreview | null) => preview = value, onmove: (key: string, index: number) => moves.push([key, index]) };
  const action = pointerSort(node as unknown as HTMLElement, options);
  function pointer(type: string, y: number, source = 0, pointerId = 1) {
    const handle = { closest: () => rows[source], matches: () => false, focus() {} };
    const target = { closest: () => handle };
    const event = new Event(type, { cancelable: true });
    Object.defineProperties(event, { target: { value: target }, button: { value: 0 }, isPrimary: { value: true }, pointerId: { value: pointerId }, clientY: { value: y } });
    node.dispatchEvent(event);
  }
  return { pointer, moves, action, options, view, get preview() { return preview; }, get captured() { return captured; },
    get scrollTop() { return scrollTop; }, get pendingFrames() { return callbacks.size; },
    tick() { const scheduled = [...callbacks.values()]; callbacks.clear(); scheduled.forEach(fn => fn(0)); },
  };
}

test("captured pointer drag moves first to last only on release, with insertion feedback", () => {
  const f = fixture();
  f.pointer("pointerdown", 16);
  assert.equal(f.captured, 1);
  f.pointer("pointermove", 100);
  assert.equal(f.preview?.beforeKey, null);
  assert.equal(f.preview?.lastKey, "v2");
  assert.deepEqual(f.moves, []);
  f.pointer("pointerup", 100);
  assert.deepEqual(f.moves, [["v0", 2]]);
  assert.equal(f.preview, null);
  assert.equal(f.captured, null);
  assert.equal(f.pendingFrames, 0);
  f.action.destroy();
});

test("dragging upward uses the target midpoint and ignores other pointers", () => {
  const f = fixture();
  f.pointer("pointerdown", 88, 2);
  f.pointer("pointermove", 1, 2, 2);
  f.pointer("pointerup", 1, 2, 2);
  assert.equal(f.preview, null);
  assert.equal(f.captured, 1);
  f.pointer("pointermove", 4, 2);
  assert.equal(f.preview?.beforeKey, "v0");
  f.pointer("pointerup", 4, 2);
  assert.deepEqual(f.moves, [["v2", 0]]);
  f.action.destroy();
});

test("clicks, pointer cancellation, Escape and disabled updates never reorder", () => {
  const f = fixture();
  f.pointer("pointerdown", 16); f.pointer("pointermove", 18); f.pointer("pointerup", 18);
  f.pointer("pointerdown", 16); f.pointer("pointermove", 100); f.pointer("pointercancel", 100);
  f.pointer("pointerdown", 16); f.pointer("pointermove", 100);
  const escape = new Event("keydown", { cancelable: true }); Object.defineProperty(escape, "key", { value: "Escape" });
  f.view.dispatchEvent(escape);
  assert.equal(escape.defaultPrevented, true);
  f.pointer("pointerdown", 16); f.pointer("pointermove", 100);
  f.action.update({ ...f.options, disabled: true });
  f.pointer("pointerdown", 16); f.pointer("pointermove", 100); f.pointer("pointerup", 100);
  assert.deepEqual(f.moves, []);
  assert.equal(f.captured, null);
  assert.equal(f.preview, null);
  assert.equal(f.pendingFrames, 0);
  f.action.destroy();
});

test("edge dragging scrolls long lists and can reach the final version", () => {
  const f = fixture(8);
  f.pointer("pointerdown", 16); f.pointer("pointermove", 107);
  for (let i = 0; i < 40; i++) f.tick();
  assert.ok(f.scrollTop > 0);
  f.pointer("pointerup", 107);
  assert.deepEqual(f.moves, [["v0", 7]]);
  f.action.destroy();
  assert.equal(f.pendingFrames, 0);
});
