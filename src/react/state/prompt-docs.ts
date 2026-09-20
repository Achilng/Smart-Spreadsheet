import { create } from "zustand";
import type { JSONContent } from "@tiptap/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  createPromptDoc, deletePromptDoc, listPromptDocs, loadPromptDoc, savePromptDoc,
  type PromptDocAsset, type PromptDocDetail, type PromptDocSummary,
} from "../../lib/api/prompt-docs";
import { registerCloseGuard } from "../../lib/stores/close-guard";
import { lastPromptDoc, rememberPromptDoc } from "../../lib/stores/view-state";
import { errorText } from "../../lib/utils/format";
import { notify } from "./notices";
import { useLibrary } from "./library";
import { runTask } from "./tasks";

type SaveState = "idle" | "dirty" | "saving" | "saved" | "error";
interface PromptDocsState {
  directory: string | null;
  docs: PromptDocSummary[];
  active: PromptDocDetail | null;
  title: string;
  content: JSONContent;
  plainText: string;
  search: string;
  loading: boolean;
  initialized: boolean;
  error: string | null;
  saveState: SaveState;
  revision: number;
  savedRevision: number;
  documentVersion: number;
}
export const emptyPromptDoc = (): JSONContent => ({ type: "doc", content: [{ type: "paragraph" }] });
export const usePromptDocs = create<PromptDocsState>(() => ({
  directory: null, docs: [], active: null, title: "", content: emptyPromptDoc(), plainText: "", search: "",
  loading: false, initialized: false, error: null, saveState: "idle", revision: 0, savedRevision: 0, documentVersion: 0,
}));
let timer: ReturnType<typeof setTimeout> | undefined;
let savePromise: Promise<boolean> | null = null;
let initializePromise: Promise<void> | null = null;
const displayToPersistent = new Map<string, string>();
const persistentToDisplay = new Map<string, string>();

export function hasPromptDocDrafts(): boolean {
  const state = usePromptDocs.getState();
  return Boolean(state.active && state.revision !== state.savedRevision);
}

const unregisterCloseGuard = registerCloseGuard(async () => {
  if (!hasPromptDocDrafts()) return null;
  await flushPromptDocSave();
  return hasPromptDocDrafts() ? `提示词文档「${usePromptDocs.getState().title || "未命名文档"}」有未保存的修改` : null;
});
const beforeUnload = (event: BeforeUnloadEvent) => {
  if (hasPromptDocDrafts()) { event.preventDefault(); event.returnValue = ""; }
};
if (typeof window !== "undefined") window.addEventListener("beforeunload", beforeUnload);
if (import.meta.hot) import.meta.hot.dispose(() => {
  unregisterCloseGuard();
  window.removeEventListener("beforeunload", beforeUnload);
  clearTimeout(timer);
});

export function registerPromptDocAsset(asset: PromptDocAsset): string {
  const display = convertFileSrc(asset.path);
  displayToPersistent.set(display, asset.src);
  persistentToDisplay.set(asset.src, display);
  return display;
}

function mapImages(value: unknown, mapper: (src: string) => string): unknown {
  if (Array.isArray(value)) return value.map(child => mapImages(child, mapper));
  if (!value || typeof value !== "object") return value;
  const source = value as Record<string, unknown>;
  const next = Object.fromEntries(Object.entries(source).map(([key, child]) => [key, mapImages(child, mapper)]));
  if (source.type === "image" && next.attrs && typeof next.attrs === "object") {
    const attrs = { ...next.attrs as Record<string, unknown> };
    if (typeof attrs.src === "string") attrs.src = mapper(attrs.src);
    next.attrs = attrs;
  }
  return next;
}

function applyDoc(doc: PromptDocDetail | null): void {
  clearTimeout(timer);
  displayToPersistent.clear();
  persistentToDisplay.clear();
  for (const asset of doc?.assets ?? []) registerPromptDocAsset(asset);
  rememberPromptDoc(doc?.id ?? null);
  usePromptDocs.setState(state => ({
    active: doc, title: doc?.title ?? "", plainText: doc?.plainText ?? "",
    content: doc ? mapImages(doc.content, src => persistentToDisplay.get(src) ?? src) as JSONContent : emptyPromptDoc(),
    saveState: doc ? "saved" : "idle", revision: 0, savedRevision: 0, documentVersion: state.documentVersion + 1,
  }));
}

function upsertSummary(doc: PromptDocSummary): void {
  const summary = { id: doc.id, title: doc.title, createdAt: doc.createdAt, updatedAt: doc.updatedAt, plainText: doc.plainText };
  usePromptDocs.setState(state => ({ docs: [summary, ...state.docs.filter(item => item.id !== doc.id)].sort((a, b) =>
    b.updatedAt.localeCompare(a.updatedAt) || a.title.localeCompare(b.title) || a.id.localeCompare(b.id)) }));
}

export function initializePromptDocs(): Promise<void> {
  if (initializePromise) return initializePromise;
  const directory = useLibrary.getState().snapshot?.dataDirectory ?? null;
  const state = usePromptDocs.getState();
  if (state.directory === directory && state.initialized) return Promise.resolve();
  initializePromise = (async () => {
    // Library switching must flush before switching the backend connection. Retain
    // a failed draft instead of accidentally writing it into a different library.
    if (state.directory !== directory && hasPromptDocDrafts()) {
      usePromptDocs.setState({ error: "上一个资料库的文档尚未保存，请先切回该资料库保存。" });
      return;
    }
    const remembered = lastPromptDoc();
    if (state.directory !== directory) {
      applyDoc(null);
      usePromptDocs.setState({ docs: [], directory, initialized: false });
    }
    usePromptDocs.setState({ loading: true, error: null });
    try {
      const docs = await listPromptDocs();
      const target = docs.find(doc => doc.id === remembered) ?? docs[0];
      const detail = target ? await loadPromptDoc(target.id) : null;
      usePromptDocs.setState({ docs, directory, initialized: true });
      applyDoc(detail);
    } catch (error) { usePromptDocs.setState({ error: errorText(error), initialized: false }); }
    finally { usePromptDocs.setState({ loading: false }); }
  })().finally(() => { initializePromise = null; });
  return initializePromise;
}

export function editPromptDoc(patch: Partial<Pick<PromptDocsState, "title" | "content" | "plainText">>): void {
  if (!usePromptDocs.getState().active) return;
  usePromptDocs.setState(state => ({ ...patch, revision: state.revision + 1, saveState: "dirty" }));
  clearTimeout(timer);
  timer = setTimeout(() => { void flushPromptDocSave(); }, 800);
}

/** Serial saves retain edits made during IPC and only mark the saved revision clean. */
export function flushPromptDocSave(): Promise<boolean> {
  clearTimeout(timer);
  if (savePromise) return savePromise;
  if (!hasPromptDocDrafts()) return Promise.resolve(true);
  savePromise = (async () => {
    while (hasPromptDocDrafts()) {
      const state = usePromptDocs.getState();
      if (state.directory !== (useLibrary.getState().snapshot?.dataDirectory ?? null)) return false;
      const id = state.active!.id;
      const revision = state.revision;
      const content = mapImages(state.content, src => displayToPersistent.get(src) ?? src);
      usePromptDocs.setState({ saveState: "saving" });
      try {
        const saved = await savePromptDoc(id, state.title, content, state.plainText);
        if (usePromptDocs.getState().active?.id !== id) return false;
        upsertSummary(saved);
        usePromptDocs.setState(current => ({
          active: saved, savedRevision: revision,
          ...(current.revision === revision ? { title: saved.title, saveState: "saved" as const } : { saveState: "dirty" as const }),
        }));
      } catch (error) {
        usePromptDocs.setState({ saveState: "error" });
        notify(`文档保存失败：${errorText(error)}`, "error");
        return false;
      }
    }
    return true;
  })().finally(() => { savePromise = null; });
  return savePromise;
}

/** Use only after the user explicitly chooses to discard a failed draft. */
export async function discardPromptDocChanges(): Promise<void> {
  clearTimeout(timer);
  if (savePromise) await savePromise;
  applyDoc(usePromptDocs.getState().active);
}

export async function switchPromptDoc(id: string | null, confirmDiscard: () => Promise<boolean>): Promise<void> {
  if (usePromptDocs.getState().loading || (id && usePromptDocs.getState().active?.id === id)) return;
  usePromptDocs.setState({ loading: true });
  try {
    if (!(await flushPromptDocSave()) && !(await confirmDiscard())) return;
    const doc = id ? await loadPromptDoc(id) : await runTask("新建提示词文档", () => createPromptDoc("未命名文档"));
    upsertSummary(doc);
    applyDoc(doc);
  } catch (error) { notify(errorText(error), "error"); }
  finally { usePromptDocs.setState({ loading: false }); }
}

export async function removeActivePromptDoc(): Promise<void> {
  const state = usePromptDocs.getState();
  if (!state.active || state.loading) return;
  usePromptDocs.setState({ loading: true });
  clearTimeout(timer);
  try {
    // An already-issued save must finish before deleting its destination.
    if (savePromise) await savePromise;
    await runTask("删除提示词文档", () => deletePromptDoc(state.active!.id));
    const docs = usePromptDocs.getState().docs.filter(doc => doc.id !== state.active!.id);
    usePromptDocs.setState({ docs });
    applyDoc(null);
    if (docs[0]) applyDoc(await loadPromptDoc(docs[0].id));
    notify("提示词文档已删除。");
  } catch (error) { notify(errorText(error), "error"); }
  finally { usePromptDocs.setState({ loading: false }); }
}
