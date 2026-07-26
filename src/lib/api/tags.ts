import { invoke } from "@tauri-apps/api/core";

import type { RowSelection } from "./types";

export interface TagSummary {
  name: string;
  rowCount: number;
}

export interface TagSelectionSummary {
  name: string;
  selectedRows: number;
}

export interface TagMutationResult {
  affectedRows: number;
  normalizedTags: string[];
  associationsChanged: number;
}

export function listTags(): Promise<TagSummary[]> {
  return invoke<TagSummary[]>("list_tags");
}

export function createTag(name: string): Promise<boolean> {
  return invoke<boolean>("create_tag", { name });
}

export function deleteTag(name: string): Promise<boolean> {
  return invoke<boolean>("delete_tag", { name });
}

export function renameTag(oldName: string, newName: string): Promise<boolean> {
  return invoke<boolean>("rename_tag", { oldName, newName });
}

/** 最近使用的 Tag（打标对话框置顶用），持久化在 settings 表 */
export async function getRecentTags(): Promise<string[]> {
  const raw = await invoke<string>("get_recent_tags");
  if (!raw) {
    return [];
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter(item => typeof item === "string") : [];
  } catch {
    return [];
  }
}

export function setRecentTags(tags: string[]): Promise<void> {
  return invoke<void>("set_recent_tags", { json: JSON.stringify(tags) });
}

export function listSelectionTags(selection: RowSelection): Promise<TagSelectionSummary[]> {
  return invoke<TagSelectionSummary[]>("list_selection_tags", { selection });
}

export function addTagsToSelection(
  selection: RowSelection,
  tags: string[],
): Promise<TagMutationResult> {
  return invoke<TagMutationResult>("add_tags_to_selection", { selection, tags });
}

export function removeTagsFromSelection(
  selection: RowSelection,
  tags: string[],
): Promise<TagMutationResult> {
  return invoke<TagMutationResult>("remove_tags_from_selection", { selection, tags });
}

export function setTagsForRow(rowId: number, tags: string[]): Promise<TagMutationResult> {
  return invoke<TagMutationResult>("set_tags_for_row", { rowId, tags });
}
