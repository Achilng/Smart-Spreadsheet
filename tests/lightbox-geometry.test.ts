import { test } from "node:test";
import assert from "node:assert/strict";
import { clampImagePan, imageFitScale, LIGHTBOX_PADDING, zoomImageAt } from "../src/lib/images/lightbox-geometry.ts";

test("portrait, panorama and tall originals fit entirely within a resized canvas", () => {
  for (const image of [{ width: 832, height: 1216 }, { width: 1216, height: 832 }, { width: 400, height: 12000 }, { width: 12000, height: 400 }]) {
    for (const viewport of [{ width: 1200, height: 560 }, { width: 860, height: 240 }, { width: 320, height: 160 }]) {
      const scale = imageFitScale(image, viewport);
      assert.ok(image.width * scale <= viewport.width - 2 * LIGHTBOX_PADDING + 1e-8);
      assert.ok(image.height * scale <= viewport.height - 2 * LIGHTBOX_PADDING + 1e-8);
      assert.ok(scale > 0 && scale <= 1);
    }
  }
  assert.equal(imageFitScale({ width: 100, height: 100 }, { width: 900, height: 600 }), 1);
  assert.equal(imageFitScale({ width: 832, height: 1216 }, { width: 0, height: 0 }), 0);
});
test("zoom preserves the image point under the pointer", () => {
  const pan = { x: 25, y: -30 }, point = { x: 120, y: 90 };
  const next = zoomImageAt(pan, point, .5, 1.25);
  assert.equal((point.x - next.x) / 1.25, (point.x - pan.x) / .5);
  assert.equal((point.y - next.y) / 1.25, (point.y - pan.y) / .5);
});
test("panning can reach both image edges and re-centers when fitted", () => {
  const image = { width: 832, height: 1216 }, viewport = { width: 900, height: 600 };
  const low = clampImagePan({ x: -9999, y: -9999 }, image, viewport, 1);
  const high = clampImagePan({ x: 9999, y: 9999 }, image, viewport, 1);
  assert.equal(low.x, 0); assert.equal(high.x, 0);
  assert.equal(low.y, -332); assert.equal(high.y, 332);
  const fitted = clampImagePan(high, image, viewport, imageFitScale(image, viewport));
  assert.ok(Math.abs(fitted.x) < 1e-8 && Math.abs(fitted.y) < 1e-8);
});
