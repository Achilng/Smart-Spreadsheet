import assert from "node:assert/strict";
import test from "node:test";

import { diffPromptField } from "../src/lib/utils/prompt-diff.ts";

test("compares tags case-insensitively and folds internal whitespace", () => {
  const diff = diffPromptField("Red   Hair, smile", "red hair, blue eyes");

  assert.deepEqual(diff.shared.map(token => token.display), ["Red   Hair"]);
  assert.deepEqual(diff.onlyLeft.map(token => token.display), ["smile"]);
  assert.deepEqual(diff.onlyRight.map(token => token.display), ["blue eyes"]);
});

test("keeps duplicate tags count-aware", () => {
  const diff = diffPromptField("cat, cat, dog", "CAT, bird, cat, cat");

  assert.deepEqual(diff.shared.map(token => token.display), ["cat", "cat"]);
  assert.deepEqual(diff.onlyLeft.map(token => token.display), ["dog"]);
  assert.deepEqual(diff.onlyRight.map(token => token.display), ["CAT", "bird"]);
});

test("marks plain, wrapped, numeric-weighted, and curated quality tags", () => {
  const diff = diffPromptField(
    "{best quality}, 1.5::very aesthetic::, -0.8::feet::, real feet",
    "",
  );

  assert.deepEqual(
    diff.onlyLeft.map(token => [token.display, token.isQuality]),
    [
      ["{best quality}", true],
      ["1.5::very aesthetic::", true],
      ["-0.8::feet::", true],
      ["real feet", false],
    ],
  );
});

test("handles empty prompts and comma/newline separators", () => {
  assert.deepEqual(diffPromptField(null, ""), {
    onlyLeft: [],
    shared: [],
    onlyRight: [],
  });

  const diff = diffPromptField("one\ntwo, three", "two");
  assert.deepEqual(diff.shared.map(token => token.display), ["two"]);
  assert.deepEqual(diff.onlyLeft.map(token => token.display), ["one", "three"]);
});
