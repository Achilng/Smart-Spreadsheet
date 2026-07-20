import { invoke } from "@tauri-apps/api/core";

export type QuickEditPromptField =
  | "positivePrompt"
  | "characterPrompt"
  | "negativePrompt";

export interface QuickEditCondition {
  fields: QuickEditPromptField[];
  requiredTokens: string[];
}

export interface QuickTagPreview {
  scannedRows: number;
  matchedRows: number;
  rowsNeedingChanges: number;
  alreadyTaggedRows: number;
  associationsToAdd: number;
  sampleRowIds: number[];
  normalizedTokens: string[];
  normalizedTags: string[];
}

export interface QuickTagAssociation {
  rowId: number;
  tag: string;
}

export interface QuickTagApplyResult {
  scannedRows: number;
  matchedRows: number;
  changedRows: number;
  associationsChanged: number;
  changes: QuickTagAssociation[];
}

export function previewQuickTag(
  condition: QuickEditCondition,
  tags: string[],
): Promise<QuickTagPreview> {
  return invoke<QuickTagPreview>("preview_quick_tag", { condition, tags });
}

export function applyQuickTag(
  condition: QuickEditCondition,
  tags: string[],
): Promise<QuickTagApplyResult> {
  return invoke<QuickTagApplyResult>("apply_quick_tag", { condition, tags });
}

export function revertQuickTagChanges(changes: QuickTagAssociation[]): Promise<number> {
  return invoke<number>("revert_quick_tag_changes", { changes });
}

export function reapplyQuickTagChanges(changes: QuickTagAssociation[]): Promise<number> {
  return invoke<number>("reapply_quick_tag_changes", { changes });
}
