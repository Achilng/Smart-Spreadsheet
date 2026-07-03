import { invoke } from "@tauri-apps/api/core";

import type { RowPage } from "./rows";
import type { RowSelection } from "./types";

export interface GroupSummary {
  id: number;
  name: string;
  memberCount: number;
  createdAt: string;
}

export function createGroup(name: string): Promise<GroupSummary> {
  return invoke<GroupSummary>("create_group", { name });
}

export function renameGroup(groupId: number, newName: string): Promise<GroupSummary> {
  return invoke<GroupSummary>("rename_group", { groupId, newName });
}

export function deleteGroup(groupId: number): Promise<boolean> {
  return invoke<boolean>("delete_group", { groupId });
}

export function deleteEmptyGroups(): Promise<number> {
  return invoke<number>("delete_empty_groups");
}

export function listGroups(): Promise<GroupSummary[]> {
  return invoke<GroupSummary[]>("list_groups");
}

export function assignRowsToGroup(
  selection: RowSelection,
  groupId: number,
): Promise<number> {
  return invoke<number>("assign_rows_to_group", { selection, groupId });
}

export function ungroupRows(selection: RowSelection): Promise<number> {
  return invoke<number>("ungroup_rows", { selection });
}

export type SimilarityMode = "artists" | "positivePrompt";

export interface SuggestedGroup {
  name: string;
  rowIds: number[];
}

export function suggestGroups(
  mode: SimilarityMode,
  threshold: number,
): Promise<SuggestedGroup[]> {
  return invoke<SuggestedGroup[]>("suggest_groups", { mode, threshold });
}

export function getGroupMembers(
  groupId: number,
  offset: number,
  limit: number,
): Promise<RowPage> {
  return invoke<RowPage>("get_group_members", { groupId, offset, limit });
}
