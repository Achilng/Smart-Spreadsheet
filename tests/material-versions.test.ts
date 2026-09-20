import { test } from "node:test";
import assert from "node:assert/strict";
import { newMaterialVersion, materialVersionDrafts, moveMaterialVersion, duplicateMaterialVersion } from "../src/lib/utils/material-versions.ts";

test("reordering changes the default without mutating the editor selection or text", () => {
  const versions = [newMaterialVersion("无服设"), newMaterialVersion("原服设"), newMaterialVersion("JK 制服")];
  versions[2].text = " exact prompt\n";
  const reordered = moveMaterialVersion(versions, 2, 0);
  assert.equal(reordered[0], versions[2]);
  assert.equal(reordered[0].text, " exact prompt\n");
  assert.equal(versions[0].name, "无服设");
  assert.deepEqual(reordered.slice(1), versions.slice(0, 2));
  assert.equal(moveMaterialVersion(versions, -1, 0), versions);
  assert.equal(moveMaterialVersion(versions, 0, 3), versions);
});

test("duplicate versions get new identities and retain the saved or pending image source", () => {
  const source = { ...newMaterialVersion("原服设"), id: 7, imageSourceId: 7, text: "dress\n" };
  const copied = duplicateMaterialVersion(source);
  assert.equal(copied.id, null);
  assert.notEqual(copied.key, source.key);
  assert.equal(copied.imageSourceId, 7);
  assert.equal(copied.text, source.text);
  copied.text = "edited";
  assert.equal(source.text, "dress\n");
  source.imagePath = "D:\\图片\\new.png";
  assert.equal(duplicateMaterialVersion(source).imagePath, source.imagePath);
});

test("editing retains version order and shared-cover fallback without mutating saved material", () => {
  const material = { id: 1, title: "花绘", text: "a", tags: ["OC"], updatedAt: "",
    versions: [{ id: 2, name: "无服设", text: "a", hasImage: false }, { id: 3, name: "原服设", text: "b", hasImage: true }] };
  const drafts = materialVersionDrafts(material);
  assert.deepEqual(drafts.map(v => v.id), [2, 3]);
  assert.deepEqual(drafts.map(v => v.imageSourceId), [null, 3]);
  drafts[0].text = "changed";
  assert.equal(material.versions[0].text, "a");
  assert.equal(materialVersionDrafts(null)[0].name, "默认版本");
});
