import { invoke } from "@tauri-apps/api/core";

// ── Prompt Documents ────────────────────────────────────────────

export interface PromptDocAsset {
  src: string;
  path: string;
}

export interface PromptDocSummary {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  plainText: string;
}

export interface PromptDocDetail extends PromptDocSummary {
  content: unknown;
  assets: PromptDocAsset[];
}

export function listPromptDocs(): Promise<PromptDocSummary[]> {
  return invoke<PromptDocSummary[]>("list_prompt_docs");
}

export function createPromptDoc(title: string): Promise<PromptDocDetail> {
  return invoke<PromptDocDetail>("create_prompt_doc", { title });
}

export function loadPromptDoc(docId: string): Promise<PromptDocDetail> {
  return invoke<PromptDocDetail>("load_prompt_doc", { docId });
}

export function savePromptDoc(
  docId: string,
  title: string,
  content: unknown,
  plainText: string,
): Promise<PromptDocDetail> {
  return invoke<PromptDocDetail>("save_prompt_doc", { docId, title, content, plainText });
}

export function deletePromptDoc(docId: string): Promise<void> {
  return invoke<void>("delete_prompt_doc", { docId });
}

export function importPromptDocImageFromPath(
  docId: string,
  path: string,
): Promise<PromptDocAsset> {
  return invoke<PromptDocAsset>("import_prompt_doc_image_from_path", { docId, path });
}

export function importPromptDocImageBytes(
  docId: string,
  fileName: string,
  bytes: number[],
): Promise<PromptDocAsset> {
  return invoke<PromptDocAsset>("import_prompt_doc_image_bytes", { docId, fileName, bytes });
}
