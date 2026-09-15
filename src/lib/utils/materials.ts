import type { MetadataSection } from "../api/materials";

export const MATERIAL_IMAGE_EXTENSIONS = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];
export function isMaterialImagePath(path: string): boolean {
  return MATERIAL_IMAGE_EXTENSIONS.includes(path.split(".").pop()?.toLowerCase() ?? "");
}
/** Keep metadata order and exact text; never add labels to copied prompts. */
export function mergeMaterialMetadata(sections: MetadataSection[], selected: string[]): string {
  const ids = new Set(selected);
  return sections.filter(section => ids.has(section.id)).map(section => section.text).join("\n\n");
}
export function splitMaterialTags(value: string): string[] {
  return [...new Set(value.split(/[,，\n\r]/).map(name => name.trim()).filter(Boolean))];
}
