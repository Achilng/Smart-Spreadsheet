import { invoke } from "@tauri-apps/api/core";

import type { RowSelection } from "./types";

// ── Prompt Editing ──────────────────────────────────────────────

export interface PromptEditResult {
  affectedRows: number;
}

export interface SinglePromptEditResult {
  affectedRows: number;
  newArtists: string | null;
}

export function updatePositivePrompt(
  rowId: number,
  newPrompt: string,
): Promise<SinglePromptEditResult> {
  return invoke<SinglePromptEditResult>("update_positive_prompt", { rowId, newPrompt });
}

export function updateNegativePrompt(
  rowId: number,
  newPrompt: string,
): Promise<number> {
  return invoke<number>("update_negative_prompt", { rowId, newPrompt });
}

export function findReplacePrompt(
  selection: RowSelection,
  find: string,
  replace: string,
): Promise<PromptEditResult> {
  return invoke<PromptEditResult>("find_replace_prompt", { selection, find, replace });
}

export function prefixArtistTag(
  selection: RowSelection,
  artistName: string,
): Promise<PromptEditResult> {
  return invoke<PromptEditResult>("prepend_artist", { selection, artistName });
}
