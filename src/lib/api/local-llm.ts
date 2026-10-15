import { invoke } from "@tauri-apps/api/core";
import type { RowSelection } from "./types";

export interface LlmChannel { id: string; name: string; baseUrl: string; model: string; concurrency: number; enabled: boolean; hasKey: boolean; key?: string }
export interface LlmSettings { autoImport: boolean; autoResume: boolean; batchSize: number; reasoning: string; channels: LlmChannel[] }
export interface LlmJob { id: string; createdAt: string; state: string; promptVersion: string; batchSize: number; reasoning: string; channelIds: string[]; message: string; total: number; done: number; failed: number; applied: number; empty: number; reuse: boolean }
export interface LlmProgress { libraryId: string; jobId: string; total: number; done: number; failed: number; inFlight: number; queued: number; message: string; running: boolean }
export interface LlmLive { id: number; channel: string; model: string; count: number; text: string }
export interface LlmSnapshot { libraryId: string; settings: LlmSettings; jobs: LlmJob[]; progress: LlmProgress; live: LlmLive[]; liveTotal: number }
export interface LlmItem { hash: string; prompt: string; status: string; artists: string; model: string; channel: string; error: string; attempts: number; rowId: number | null }
export interface LlmEstimate { rows: number; skipped: number; uniquePrompts: number; affectedRows: number }
export const llmSnapshot = (livePage = 0) => invoke<LlmSnapshot>("llm_snapshot", { livePage });
export const saveLlmSettings = (libraryId: string, settings: LlmSettings) => invoke<LlmSettings>("llm_save_settings", { libraryId, settings });
export const estimateLlm = (libraryId: string, selection: RowSelection | null, includeProcessed: boolean, allMatching: boolean) => invoke<LlmEstimate>("llm_estimate", { libraryId, selection, includeProcessed, allMatching });
export const createLlm = (libraryId: string, selection: RowSelection | null, includeProcessed: boolean, allMatching: boolean, channelIds: string[]) => invoke<string | null>("llm_create", { libraryId, selection, includeProcessed, allMatching, channelIds });
export const actLlm = (libraryId: string, jobId: string, action: string, channelIds: string[]) => invoke<void>("llm_action", { libraryId, jobId, action, channelIds });
export const llmItems = (libraryId: string, jobId: string, page: number) => invoke<LlmItem[]>("llm_items", { libraryId, jobId, page });
export const llmModels = (libraryId: string, channel: LlmChannel) => invoke<string[]>("llm_models", { libraryId, channel });
export const exportLlm = (libraryId: string, jobId: string, path: string) => invoke<void>("llm_export", { libraryId, jobId, path });
