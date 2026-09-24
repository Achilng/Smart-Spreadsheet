import { invoke } from "@tauri-apps/api/core";
import type { RowSelection } from "./types";
export interface StyleChange { rowId: number; inputHash: string; oldArtists: string | null; oldSource: string | null; newArtists: string | null; newSource: string | null }
export interface StylePreview { libraryId: string; changes: StyleChange[]; matchedRows: number; unchanged: number; emptyResults: number; failed: number; unmatched: number; invalidItems: number; issues: string[] }
export interface StyleExportSummary { rows: number; skipped: number; uniquePrompts: number }
export function exportStyleRequest(selection: RowSelection | null, includeProcessed: boolean, path: string): Promise<StyleExportSummary> { return invoke("export_style_request", { selection, includeProcessed, path }); }
export function previewStyleResult(path: string): Promise<StylePreview> { return invoke("preview_style_result", { path }); }
export function applyStyleChanges(libraryId: string, changes: StyleChange[], reverse = false, strict = false): Promise<{ changes: StyleChange[]; conflicts: number }> { return invoke("apply_style_changes", { libraryId, changes, reverse, strict }); }
