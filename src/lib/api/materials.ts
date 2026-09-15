import { invoke } from "@tauri-apps/api/core";
import type { TagSummary } from "./tags";

export interface Material { id: number; title: string; text: string; tags: string[]; updatedAt: string }
export interface MaterialDraft { id: number | null; title: string; text: string; tags: string[]; imagePath: string | null }
export interface MetadataSection { id: string; label: string; text: string }
export interface MaterialInspection { title: string; preview: number[]; sections: MetadataSection[]; warning: string | null }
export const listMaterials = (search: string, tags: string[], untagged: boolean, offset: number) =>
  invoke<{ items: Material[]; total: number }>("list_materials", { search, tags, untagged, offset });
export const materialTagCounts = () => invoke<TagSummary[]>("material_tag_counts");
export const materialIdsForTag = (name: string) => invoke<number[]>("material_ids_for_tag", { name });
export const restoreMaterialTag = (name: string, ids: number[]) => invoke<void>("restore_material_tag", { name, ids });
export const inspectMaterialImage = (path: string) => invoke<MaterialInspection>("inspect_material_image", { path });
export const inspectMaterialLibraryImage = (rowId: number) => invoke<{ path: string; inspection: MaterialInspection }>("inspect_material_library_image", { rowId });
export const saveMaterial = (draft: MaterialDraft) => invoke<Material>("save_material", { draft });
export const deleteMaterial = (id: number) => invoke<void>("delete_material", { id });
export const materialImage = (id: number, thumbnail: boolean) => invoke<ArrayBuffer>("material_image", { id, thumbnail });
