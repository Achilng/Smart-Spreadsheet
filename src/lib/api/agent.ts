import { invoke } from "@tauri-apps/api/core";

export interface AgentConversation { id: string; title: string; createdAt: string; updatedAt: string }
export interface AgentPreferences { channelId: string; model: string; effort: string }
export interface AgentChannel { id: string; name: string; model: string; hasKey: boolean }
export interface AgentState { conversations: AgentConversation[]; running: string[]; preferences: AgentPreferences; channels: AgentChannel[] }
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
export interface AgentPreviewReport {
  previewId: string; title: string; target: Record<string, unknown>; targetNotes: string[];
  actions: Record<string, unknown>[]; scannedRows: number; changedRows: number; unchangedRows: number;
  fieldCounts: Record<string, number>; newGroups: string[]; newTags: string[];
  matchAnalysis: AgentMatchAnalysis[]; samples: AgentSample[]; warnings: string[];
}
export interface AgentOperation {
  id: number; conversationId: string | null; createdAt: string; kind: "rows" | "rule" | string; title: string;
  report: AgentPreviewReport & { ruleId?: number; rule?: { name: string; description?: string } };
  changedRows: number; state: "applied" | "undone"; undoMessage: string | null; undoneAt: string | null;
}
export interface AgentUndoOutcome { operationId: number; restoredRows: number; skippedRows: number; message: string }
export interface AgentEvent { conversationId: string; kind: "run-start" | "text-delta" | "reasoning-delta" | "tool-start" | "tool-end" | "messages-changed" | "run-end"; delta?: string; name?: string }

export const agentState = () => invoke<AgentState>("agent_state");
export const saveAgentPreferences = (preferences: AgentPreferences) => invoke<void>("agent_save_preferences", { preferences });
export const agentModels = (channelId: string) => invoke<string[]>("agent_models", { channelId });
export const agentMessages = (conversationId: string) => invoke<AgentMessage[]>("agent_messages", { conversationId });
export const agentSend = (conversationId: string | null, text: string) => invoke<string>("agent_send", { conversationId, text });
export const agentCancel = (conversationId: string) => invoke<void>("agent_cancel", { conversationId });
export const renameAgentConversation = (id: string, title: string) => invoke<void>("agent_rename_conversation", { id, title });
export const deleteAgentConversation = (id: string) => invoke<void>("agent_delete_conversation", { id });
export const agentOperations = (limit?: number) => invoke<AgentOperation[]>("agent_operations", { limit });
export const undoAgentOperation = (operationId: number) => invoke<AgentUndoOutcome>("agent_undo", { operationId });
export const agentChangePage = (source: "preview" | "operation", id: string, offset: number, limit: number) =>
  invoke<{ total: number; changes: AgentRowChange[] }>("agent_change_page", { source, id, offset, limit });
export const openAgentWindow = () => invoke<void>("open_agent_window");
export const reportAgentMainView = (selection: unknown, filtered: unknown, info: unknown) =>
  invoke<void>("agent_report_main_view", { selection, filtered, info });
