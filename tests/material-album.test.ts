import { test } from "node:test";
import assert from "node:assert/strict";
import { materialAlbum } from "../src/lib/utils/material-album.ts";

const material = { id: 7, title: "素材", text: "legacy", tags: [], updatedAt: "", versions: [
  { id: 7, name: "默认", text: "a", hasImage: false },
  { id: 8, name: "共用封面", text: "b", hasImage: false },
  { id: 9, name: "配图", text: "c", hasImage: true },
] };
test("shared covers count once; choosing a version moves its real picture to the front", () => {
  const album = materialAlbum(material, 9);
  assert.equal(album.images.length, 2);
  assert.deepEqual(album.previews.map(image => [image.kind, image.id]), [["version", 9], ["cover", 7]]);
  assert.equal(album.version.text, "c");
  assert.equal(material.versions[0].id, 7);
  assert.equal(materialAlbum(material, 8).previews[0].kind, "cover");
});
test("removed versions and legacy materials fall back without losing text", () => {
  assert.equal(materialAlbum(material, 99).version.id, 7);
  const old = materialAlbum({ ...material, versions: [] });
  assert.equal(old.version.text, "legacy"); assert.equal(old.images.length, 1);
});
test("many images keep the selected photo visible and bound album loading to three", () => {
  const many = { ...material, versions: Array.from({ length: 20 }, (_, id) => ({ id, name: String(id), text: String(id), hasImage: true })) };
  const album = materialAlbum(many, 19);
  assert.equal(album.images.length, 21);
  assert.equal(album.previews.length, 3); assert.equal(album.previews[0].id, 19);
  assert.equal(new Set(album.previews.map(image => `${image.kind}-${image.id}`)).size, 3);
  assert.equal(materialAlbum(many, 7).previews[0].kind, "version");
});
