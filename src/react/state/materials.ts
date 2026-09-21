import { create } from "zustand";
import { open } from "@tauri-apps/plugin-dialog";
import { listMaterials, materialTagCounts, materialImage, materialVersionImage, deleteMaterial, type Material } from "../../lib/api/materials";
import type { TagSummary } from "../../lib/api/tags";
import { ImageLoader } from "../../lib/images/image-loader";
import { isMaterialImagePath, MATERIAL_IMAGE_EXTENSIONS } from "../../lib/utils/materials";
import { errorText } from "../../lib/utils/format";
import { useLibrary, refreshTags } from "./library";
import { useTasks, runTask } from "./tasks";
import { notify } from "./notices";

export const MATERIAL_PAGE_SIZE = 48;
export const materialThumbnails = new ImageLoader(id => materialImage(id, true), 4, 144, "image/png");
export const materialCovers = new ImageLoader(id => materialImage(id, false), 2, 8, "image/png");
export const materialVersionCovers = new ImageLoader(materialVersionImage, 2, 16, "image/png");
export const materialCardVersionImages = new ImageLoader(materialVersionImage, 3, 144, "image/png");
interface MaterialsState {
  search: string; selectedTags: string[]; untagged: boolean; tags: TagSummary[]; tagError: string;
  pages: Map<number, Material[]>; total: number; loading: boolean; error: string; revision: number;
  selected: Material | null; detailOpen: boolean; scrollTop: number; initialized: boolean;
  editorOpen: boolean; editing: Material | null; editingVersion?: number; pendingPaths: string[]; editorKey: number;
  pendingDelete: Material | null; deleteError: string; deleting: boolean;
  cardVersions: Record<number, number>; openCardId: number | null;
}
const initial: MaterialsState = { search: "", selectedTags: [], untagged: false, tags: [], tagError: "", pages: new Map(), total: 0, loading: false, error: "", revision: 0, selected: null, detailOpen: true, scrollTop: 0, initialized: false, editorOpen: false, editing: null, pendingPaths: [], editorKey: 0, pendingDelete: null, deleteError: "", deleting: false, cardVersions: {}, openCardId: null };
export const useMaterials = create<MaterialsState>(() => initial);
let generation = 0;
let pending = new Set<number>();
function clearImages() { materialThumbnails.clear(); materialCovers.clear(); materialVersionCovers.clear(); materialCardVersionImages.clear(); }
useLibrary.subscribe((state, previous) => {
  if (state.snapshot?.dataDirectory !== previous.snapshot?.dataDirectory) {
    generation++; pending = new Set(); clearImages(); useMaterials.setState({ ...initial, pages: new Map(), revision: useMaterials.getState().revision + 1 });
  } else if (state.tags !== previous.tags && useMaterials.getState().initialized) {
    void reloadMaterials();
  }
});
export function initializeMaterials() { if (!useMaterials.getState().initialized) void reloadMaterials(); }
export async function reloadMaterials(keepSelection = true) {
  generation++; pending = new Set();
  const token = generation;
  useMaterials.setState({ pages: new Map(), total: 0, loading: true, error: "", initialized: true, scrollTop: 0, openCardId: null, ...(!keepSelection && { selected: null }) });
  await Promise.all([loadMaterialPage(0), materialTagCounts().then(tags => { if (token === generation) useMaterials.setState({ tags, tagError: "" }); }).catch(error => { if (token === generation) useMaterials.setState({ tagError: errorText(error) }); })]);
}
export async function loadMaterialPage(page: number) {
  const state = useMaterials.getState();
  if (state.pages.has(page) || pending.has(page) || state.error) return;
  const token = generation; pending.add(page);
  try {
    const result = await listMaterials(state.search, state.selectedTags, state.untagged, page * MATERIAL_PAGE_SIZE);
    if (token !== generation) return;
    const pages = new Map(useMaterials.getState().pages); pages.set(page, result.items);
    useMaterials.setState({ pages, total: result.total, loading: false });
  } catch (error) { if (token === generation) useMaterials.setState({ loading: false, error: errorText(error) }); }
  finally { if (token === generation) pending.delete(page); }
}
export function setMaterialSearch(search: string) { if (search === useMaterials.getState().search) return; useMaterials.setState({ search }); void reloadMaterials(false); }
export function toggleMaterialTag(name: string) { const tags = useMaterials.getState().selectedTags; useMaterials.setState({ selectedTags: tags.includes(name) ? tags.filter(tag => tag !== name) : [...tags, name], untagged: false }); void reloadMaterials(false); }
export function setMaterialUntagged(untagged: boolean) { useMaterials.setState({ untagged, selectedTags: [] }); void reloadMaterials(false); }
export function clearMaterialFilters() { useMaterials.setState({ search: "", selectedTags: [], untagged: false }); void reloadMaterials(false); }
export function createMaterial() { if (useTasks.getState().busy) return; useMaterials.setState(state => ({ editing: null, editingVersion: undefined, pendingPaths: [], editorOpen: true, editorKey: state.editorKey + 1 })); }
export function editMaterial(material = useMaterials.getState().selected, versionId?: number) { if (!material || useTasks.getState().busy) return; useMaterials.setState(state => ({ selected: material, editing: material, editingVersion: versionId, pendingPaths: [], editorOpen: true, editorKey: state.editorKey + 1 })); }
export function closeMaterialEditor() { useMaterials.setState({ editorOpen: false, editing: null, pendingPaths: [] }); }
export function beginMaterialImport(paths: string[]) {
  if (useMaterials.getState().editorOpen || useMaterials.getState().pendingDelete || useTasks.getState().busy) return;
  const valid = paths.filter(isMaterialImagePath);
  if (!valid.length) { notify("请拖入 PNG、JPG、WebP 等图片文件。", "error"); return; }
  if (valid.length < paths.length) notify(`已跳过 ${paths.length - valid.length} 个非图片项目。`, "error");
  useMaterials.setState(state => ({ editing: null, editingVersion: undefined, pendingPaths: valid, editorKey: state.editorKey + 1, editorOpen: true }));
}
export async function chooseMaterialImages() {
  try { const paths = await open({ multiple: true, directory: false, title: "导入素材展示图", filters: [{ name: "图片", extensions: MATERIAL_IMAGE_EXTENSIONS }] }); if (paths) beginMaterialImport(typeof paths === "string" ? [paths] : paths); }
  catch (error) { notify(errorText(error), "error"); }
}
export function materialSaved(material: Material) {
  const state = useMaterials.getState(); clearImages();
  useMaterials.setState({ selected: material, search: "", selectedTags: [], untagged: false, revision: state.revision + 1 });
  void reloadMaterials(); void refreshTags(); notify(`素材「${material.title}」已保存。`);
  if (!state.editing && state.pendingPaths.length > 1) useMaterials.setState({ pendingPaths: state.pendingPaths.slice(1), editorKey: state.editorKey + 1 });
  else closeMaterialEditor();
}
export async function copyMaterial(material: Material, text = material.versions[0]?.text ?? material.text) {
  if (!text) { notify("这份素材还没有文本，请先编辑内容。", "error"); return; }
  try { await navigator.clipboard.writeText(text); notify(`已复制「${material.title}」的文本。`); }
  catch (error) { notify(`复制失败：${errorText(error)}`, "error"); }
}
export function requestDeleteMaterial(item: Material) { if (!useTasks.getState().busy) useMaterials.setState({ pendingDelete: item, deleteError: "" }); }
export async function confirmDeleteMaterial() {
  const item = useMaterials.getState().pendingDelete;
  if (!item || useMaterials.getState().deleting) return;
  const directory = useLibrary.getState().snapshot?.dataDirectory;
  useMaterials.setState({ deleting: true, deleteError: "" });
  try {
    await runTask("删除素材", () => deleteMaterial(item.id));
    if (directory !== useLibrary.getState().snapshot?.dataDirectory) return;
    clearImages(); useMaterials.setState(state => ({ pendingDelete: null, selected: state.selected?.id === item.id ? null : state.selected, revision: state.revision + 1 }));
    await reloadMaterials(); notify("素材已删除。");
  } catch (error) { if (directory === useLibrary.getState().snapshot?.dataDirectory) useMaterials.setState({ deleteError: `删除失败：${errorText(error)}` }); }
  finally { useMaterials.setState({ deleting: false }); }
}
