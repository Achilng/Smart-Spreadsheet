import { test } from "node:test";
import assert from "node:assert/strict";
import { isMaterialImagePath, mergeMaterialMetadata, splitMaterialTags } from "../src/lib/utils/materials.ts";

test("metadata selection keeps source order and exact prompt content without labels", () => {
  const sections = [
    { id: "positive", label: "正向", text: "  quality, dress\n" },
    { id: "negative", label: "负向", text: "bad" },
    { id: "character", label: "角色", text: "girl, red hair" },
  ];
  assert.equal(mergeMaterialMetadata(sections, ["character", "positive", "positive", "missing"]), "  quality, dress\n\n\ngirl, red hair");
  assert.equal(mergeMaterialMetadata(sections, []), "");
});
test("material import accepts common images and ignores archives and folders", () => {
  assert.equal(isMaterialImagePath("D:\\素材\\礼服.PNG"), true);
  assert.equal(isMaterialImagePath("D:\\素材\\封面.webp"), true);
  assert.equal(isMaterialImagePath("D:\\素材\\package.zip"), false);
  assert.equal(isMaterialImagePath("D:\\素材"), false);
  assert.deepEqual(splitMaterialTags(" 服设,礼服，服设\n角色\r\n"), ["服设", "礼服", "角色"]);
});
