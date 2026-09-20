import { create } from "zustand";
import type { RowRecord } from "../../lib/api";
import { getCompareSample, queryCompareSameArtists, queryCompareSameVibeDiffStyle, queryCompareSameStyleDiffVibe, queryCompareSameStyleAllModels, type CompareSample, type CompareModelSection } from "../../lib/api/compare";
import { createRequestQueue } from "../../lib/utils/request-queue";

export type CompareSectionKey = "artists" | "vibeDiffStyle" | "styleDiffVibe";
export type CompareTab = CompareSectionKey | "models";
export interface SectionState { items: RowRecord[]; total: number; loading: boolean; error: string | null; loaded: boolean; offset: number }
const emptySection = (): SectionState => ({ items: [], total: 0, loading: false, error: null, loaded: false, offset: 0 });
const sections = () => ({ artists: emptySection(), vibeDiffStyle: emptySection(), styleDiffVibe: emptySection(), models: { rows: [], totalCount: 0, truncated: false } as CompareModelSection, modelsLoading: false, modelsError: null as string | null, modelsLoaded: false });
interface CompareState { rowId: number | null; sample: CompareSample | null; sampleLoading: boolean; sampleError: string | null; activeTab: CompareTab; artists: SectionState; vibeDiffStyle: SectionState; styleDiffVibe: SectionState; models: CompareModelSection; modelsLoading: boolean; modelsError: string | null; modelsLoaded: boolean; target: RowRecord | null }
export const useCompare = create<CompareState>(() => ({ rowId: null, sample: null, sampleLoading: false, sampleError: null, activeTab: "artists", target: null, ...sections() }));
const enqueue = createRequestQueue();
const PAGE_SIZE = 24;
let generation = 0;
const errorText = (error: unknown) => error instanceof Error ? error.message : String(error);

export function resetCompare(): void {
  generation++;
  useCompare.setState({ rowId: null, sample: null, sampleError: null, sampleLoading: false, target: null, ...sections() });
}

export async function setCompareSample(rowId: number, force = false): Promise<void> {
  if (!Number.isInteger(rowId) || rowId <= 0) return;
  const state = useCompare.getState();
  if (!force && state.rowId === rowId && (state.sample || state.sampleLoading)) return;
  const requestGeneration = ++generation;
  useCompare.setState({ rowId, sample: null, sampleError: null, sampleLoading: true, target: null, ...sections() });
  try {
    const sample = await enqueue(() => generation === requestGeneration, () => getCompareSample(rowId));
    if (!sample || generation !== requestGeneration) return;
    useCompare.setState({ sample });
    void selectCompareTab(useCompare.getState().activeTab);
  } catch (error) {
    if (generation === requestGeneration) useCompare.setState({ sampleError: errorText(error) });
  } finally {
    if (generation === requestGeneration) useCompare.setState({ sampleLoading: false });
  }
}
export async function refreshCompare(): Promise<void> {
  const rowId = useCompare.getState().rowId;
  if (rowId !== null) await setCompareSample(rowId, true);
}
export async function selectCompareTab(tab: CompareTab): Promise<void> {
  useCompare.setState({ activeTab: tab });
  const state = useCompare.getState();
  if (!state.sample) return;
  if (tab === "models") { if (!state.modelsLoaded) await loadCompareModels(); }
  else if (!state[tab].loaded) await loadCompareSectionPage(tab, true);
}
export async function loadCompareSectionPage(section: CompareSectionKey, first = false, direction: 1 | -1 = 1): Promise<void> {
  const state = useCompare.getState();
  if (state.rowId === null || state[section].loading) return;
  const before = state[section];
  if (!first && (direction === 1 ? before.offset + before.items.length >= before.total : before.offset === 0)) return;
  const offset = first ? 0 : Math.max(0, before.offset + direction * PAGE_SIZE);
  const requestGeneration = generation;
  const update = (patch: Partial<SectionState>) => { if (generation === requestGeneration) useCompare.setState(current => ({ [section]: { ...current[section], ...patch } })); };
  update({ loading: true, error: null });
  try {
    const query = section === "artists" ? queryCompareSameArtists : section === "vibeDiffStyle" ? queryCompareSameVibeDiffStyle : queryCompareSameStyleDiffVibe;
    const page = await enqueue(() => generation === requestGeneration && useCompare.getState().activeTab === section, () => query(state.rowId!, offset, PAGE_SIZE));
    if (page) update({ items: page.rows, total: page.totalCount, offset: page.offset, loaded: true });
  } catch (error) { update({ error: errorText(error) }); }
  finally { update({ loading: false }); }
}
export async function loadCompareModels(): Promise<void> {
  const state = useCompare.getState();
  if (state.rowId === null || state.modelsLoading) return;
  const requestGeneration = generation;
  useCompare.setState({ modelsLoading: true, modelsError: null });
  try {
    const models = await enqueue(() => generation === requestGeneration && useCompare.getState().activeTab === "models", () => queryCompareSameStyleAllModels(state.rowId!));
    if (models && generation === requestGeneration) useCompare.setState({ models, modelsLoaded: true });
  } catch (error) {
    if (generation === requestGeneration) useCompare.setState({ modelsError: errorText(error) });
  } finally { if (generation === requestGeneration) useCompare.setState({ modelsLoading: false }); }
}
export function openSideBySide(target: RowRecord): void { useCompare.setState({ target }); }
export function closeSideBySide(): void { useCompare.setState({ target: null }); }
