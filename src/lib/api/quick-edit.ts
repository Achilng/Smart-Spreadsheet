import { invoke } from "@tauri-apps/api/core";

export type QuickEditTextField =
  | "positivePrompt"
  | "characterPrompt"
  | "negativePrompt"
  | "artists"
  | "note";

export interface QuickEditCondition {
  fields: QuickEditTextField[];
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

export interface QuickGroupPreview {
  scannedRows: number;
  matchedRows: number;
  rowsNeedingChanges: number;
  alreadyInGroupRows: number;
  sampleRowIds: number[];
  normalizedTokens: string[];
  targetGroupId: number;
  targetGroupName: string;
}

export interface QuickGroupChange {
  rowId: number;
  previousGroupId: number | null;
  targetGroupId: number;
}

export interface QuickGroupApplyResult {
  scannedRows: number;
  matchedRows: number;
  changedRows: number;
  changes: QuickGroupChange[];
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

export function previewQuickGroup(
  condition: QuickEditCondition,
  groupId: number,
): Promise<QuickGroupPreview> {
  return invoke<QuickGroupPreview>("preview_quick_group", { condition, groupId });
}

export function applyQuickGroup(
  condition: QuickEditCondition,
  groupId: number,
): Promise<QuickGroupApplyResult> {
  return invoke<QuickGroupApplyResult>("apply_quick_group", { condition, groupId });
}

export function revertQuickGroupChanges(changes: QuickGroupChange[]): Promise<number> {
  return invoke<number>("revert_quick_group_changes", { changes });
}

export function reapplyQuickGroupChanges(changes: QuickGroupChange[]): Promise<number> {
  return invoke<number>("reapply_quick_group_changes", { changes });
}
