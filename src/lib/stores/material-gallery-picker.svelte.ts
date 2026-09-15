import type { RowRecord } from "../api/rows";
import { app } from "./app-state.svelte";

export const materialGalleryPicker = $state({
  active: false,
  selected: null as RowRecord | null,
});

let complete: ((rowId: number | null) => void) | null = null;

/** Keep the editor mounted while the existing gallery handles browsing and filtering. */
export function startMaterialGalleryPick(callback: (rowId: number | null) => void): () => void {
  complete = callback;
  materialGalleryPicker.selected = null;
  materialGalleryPicker.active = true;
  app.viewMode = "gallery";
  return () => {
    if (complete !== callback) return;
    complete = null;
    materialGalleryPicker.active = false;
    materialGalleryPicker.selected = null;
  };
}

export function finishMaterialGalleryPick(useSelected: boolean): void {
  if (!materialGalleryPicker.active) return;
  const rowId = useSelected ? materialGalleryPicker.selected?.id : null;
  if (useSelected && rowId == null) return;
  const callback = complete;
  complete = null;
  materialGalleryPicker.active = false;
  materialGalleryPicker.selected = null;
  app.viewMode = "materials";
  callback?.(rowId ?? null);
}
