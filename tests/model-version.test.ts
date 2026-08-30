import assert from "node:assert/strict";
import test from "node:test";

import {
  modelComparisonTier,
  modelVersionBadge,
} from "../src/lib/utils/model-version.ts";

test("known build hashes share their confirmed Full or Curated tier", () => {
  assert.equal(
    modelComparisonTier("NovelAI Diffusion V5 0ADF9AB7"),
    "badge:v5 F",
  );
  assert.equal(
    modelComparisonTier("NovelAI Diffusion V5 DB276663"),
    "badge:v5 C",
  );
  assert.equal(
    modelComparisonTier("NovelAI Diffusion V4.5 4BDE2A90"),
    "badge:v4.5 F",
  );
  assert.equal(
    modelComparisonTier("NovelAI Diffusion V4 79F47848"),
    "badge:v4 C",
  );
});

test("different unknown build hashes fall back to distinct raw model strings", () => {
  const first = "NovelAI Diffusion V4.5 C02D4F98";
  const second = "NovelAI Diffusion V4.5 5BB76870";

  assert.equal(modelVersionBadge(first)?.label, "v4.5");
  assert.equal(modelVersionBadge(second)?.label, "v4.5");
  assert.equal(modelComparisonTier(first), `raw:${first}`);
  assert.equal(modelComparisonTier(second), `raw:${second}`);
  assert.notEqual(modelComparisonTier(first), modelComparisonTier(second));
});

test("plain known versions and missing models retain stable tiers", () => {
  assert.equal(modelComparisonTier("NovelAI Diffusion V5"), "badge:v5");
  assert.equal(modelComparisonTier("Stable Diffusion XL"), "raw:Stable Diffusion XL");
  assert.equal(modelComparisonTier("  "), "unknown");
  assert.equal(modelComparisonTier(null), "unknown");
});
