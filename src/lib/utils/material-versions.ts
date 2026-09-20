import type { Material, MaterialVersionDraft } from "../api/materials";

export function newMaterialVersion(name = "新版本"): MaterialVersionDraft {
  return { key: crypto.randomUUID(), id: null, name, text: "", imagePath: null, imageSourceId: null };
}

export function materialVersionDrafts(material: Material | null): MaterialVersionDraft[] {
  return material?.versions.length ? material.versions.map(version => ({
    key: crypto.randomUUID(), id: version.id, name: version.name, text: version.text,
    imagePath: null, imageSourceId: version.hasImage ? version.id : null,
  })) : [{ ...newMaterialVersion("默认版本"), text: material?.text ?? "" }];
}

export function moveMaterialVersion<T>(versions: T[], from: number, to: number): T[] {
  if (from < 0 || to < 0 || from >= versions.length || to >= versions.length || from === to) return versions;
  const result = [...versions];
  const [version] = result.splice(from, 1);
  result.splice(to, 0, version);
  return result;
}

export function duplicateMaterialVersion(version: MaterialVersionDraft): MaterialVersionDraft {
  return { ...version, key: crypto.randomUUID(), id: null, name: `${version.name} 副本` };
}
