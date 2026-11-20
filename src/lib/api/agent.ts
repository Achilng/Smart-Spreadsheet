import { invoke } from "@tauri-apps/api/core";

export interface AgentConversation { id: string; title: string; createdAt: string; updatedAt: string }
export interface AgentChannelModels { model: string; recent: string[] }
export interface AgentPreferences {
  channelId: string; model: string; effort: string; channelModels: Record<string, AgentChannelModels>;
  /** Context window in tokens; 0 or missing means the 200k default. */
  contextWindow?: number;
}
export const DEFAULT_CONTEXT_WINDOW = 200_000;
/** How full the context was after the latest model reply or summary (stored on that message). `compactions`: summaries made so far. */
export interface AgentContextUsage { used: number; window: number; source: "api" | "estimate"; compactions?: number }
export interface AgentChannel { id: string; name: string; model: string; hasKey: boolean; catalogRevision: string }
export interface AgentState { libraryId: string; conversations: AgentConversation[]; running: string[]; preferences: AgentPreferences; channels: AgentChannel[] }

export function selectedAgentModel(preferences: AgentPreferences, channelId: string): string {
  return (!preferences.channelId || preferences.channelId === channelId
    ? preferences.model
    : preferences.channelModels[channelId]?.model ?? "").trim();
}
export interface AgentMessage { id: number; createdAt: string; kind: "user" | "assistant" | "tool_call" | "tool_result" | "error" | string; content: Record<string, unknown> }

export interface AgentTextChange { field: string; before: string | null; after: string | null }
export interface AgentRowChange {
  id: number;
  text?: AgentTextChange[];
  group?: { before: number | null; after: number | null; beforeName: string | null; afterName: string | null };
  favorite?: [boolean, boolean];
  tagsAdded?: string[];
  tagsRemoved?: string[];
}
export interface AgentSample {
  rowId: number;
  text: { field: string; before: string; after: string; afterEmpty: boolean }[];
  group?: { before: string | null; after: string | null };
  favorite?: [boolean, boolean];
  tagsAdded?: string[];
  tagsRemoved?: string[];
}
export interface AgentMatchAnalysis {
  field: string; rowsMatched: number; occurrences: number; rowsWithMultipleMatches: number;
  positions: { atStart: number; inMiddle: number; atEnd: number; spanningLines: number };
  distinctMatchCount: number;
  distinctMatches: { text: string; count: number; sampleRowId: number; context: string }[];
  sameMatchInOtherFields?: Record<string, number>;
  actionIndex?: number;
}
export interface AgentTextEdit {
  field: string;
  /** Edited span by characters (notes). */
  removed: string; added: string;
  /** Prompt fields: tags that go and come. */
  removedTags?: string[]; addedTags?: string[];
  formatOnly?: boolean;
  /** Excerpt around the edit in the first row; empty when it spans too much. */
  before: string; after: string; afterEmpty: boolean;
}
/** Rows that change in exactly the same way. */
export interface AgentChangeGroup {
  count: number; rowIds: number[];
  text?: AgentTextEdit[];
  group?: [string | null, string | null];
  favorite?: [boolean, boolean];
  tagsAdded?: string[];
  tagsRemoved?: string[];
}
export interface AgentPreviewReport {
  previewId: string; title: string; target: Record<string, unknown>; targetNotes: string[];
  actions: Record<string, unknown>[]; scannedRows: number; changedRows: number; unchangedRows: number;
  fieldCounts: Record<string, number>; newGroups: string[]; newTags: string[];
  matchAnalysis: AgentMatchAnalysis[]; samples: AgentSample[]; warnings: string[];
  /** Missing on reports made before grouping existed. */
  changeGroups?: AgentChangeGroup[]; otherRows?: number;
  /** Plain-language lines for group / TAG / artist-string operations and rule trial runs. */
  summary?: string[];
  /** Set when the preview is a library operation (rename / merge / delete groups or TAGs, aliases). */
  library?: { type: string; [key: string]: unknown } | null;
}
export type AgentRuleSnapshot = { id: number; name: string; description?: string; enabled?: boolean };
export interface AgentOperation {
  id: number; conversationId: string | null; createdAt: string;
  kind: "rows" | "rule" | "library" | "rule_update" | "rule_delete" | string; title: string;
  report: AgentPreviewReport & { ruleId?: number; rule?: AgentRuleSnapshot; before?: AgentRuleSnapshot; after?: AgentRuleSnapshot; changed?: string[] };
  changedRows: number; state: "applied" | "undone"; undoMessage: string | null; undoneAt: string | null;
}
export interface AgentUndoOutcome { operationId: number; restoredRows: number; skippedRows: number; message: string }
export interface AgentEvent {
  conversationId: string; kind: "run-start" | "text-delta" | "reasoning-delta" | "tool-start" | "tool-end" | "messages-changed" | "run-end" | "retry";
  delta?: string; name?: string;
  /** Retry events: why the request is being sent again (or the older conversation summarized first), and which attempt this is. */
  reason?: "network" | "too-long" | "compact"; attempt?: number; message?: string;
}

export const agentState = () => invoke<AgentState>("agent_state");
export const saveAgentPreferences = (libraryId: string, preferences: AgentPreferences) =>
  invoke<void>("agent_save_preferences", { libraryId, preferences });
export const agentModels = (libraryId: string, channel: AgentChannel) =>
  invoke<string[]>("agent_models", { libraryId, channelId: channel.id, revision: channel.catalogRevision });
export const agentMessages = (conversationId: string) => invoke<AgentMessage[]>("agent_messages", { conversationId });
export interface AgentAttachment { name: string; mime: string; data: string }
export const agentSend = (conversationId: string | null, text: string, attachments: AgentAttachment[] = []) => invoke<string>("agent_send", { conversationId, text, attachments });
export const agentCancel = (conversationId: string) => invoke<void>("agent_cancel", { conversationId });
export const renameAgentConversation = (id: string, title: string) => invoke<void>("agent_rename_conversation", { id, title });
export const deleteAgentConversation = (id: string) => invoke<void>("agent_delete_conversation", { id });
export const agentOperations = (limit?: number) => invoke<AgentOperation[]>("agent_operations", { limit });
export const undoAgentOperation = (operationId: number) => invoke<AgentUndoOutcome>("agent_undo", { operationId });
export const applyAgentPreview = (conversationId: string, previewId: string) => invoke<AgentOperation>("agent_apply_preview", { conversationId, previewId });
export const showAgentChangedRows = (source: "preview" | "operation", id: string, group?: number) =>
  invoke<number>("agent_show_changed_rows", { source, id, group: group ?? null });
export const agentChangePage = (source: "preview" | "operation", id: string, offset: number, limit: number) =>
  invoke<{ total: number; changes: AgentRowChange[] }>("agent_change_page", { source, id, offset, limit });
export const openAgentWindow = () => invoke<void>("open_agent_window");
export const reportAgentMainView = (selection: unknown, filtered: unknown, info: unknown) =>
  invoke<void>("agent_report_main_view", { selection, filtered, info });
export const reportAgentCompareView = (info: unknown) => invoke<void>("agent_report_compare_view", { info });
