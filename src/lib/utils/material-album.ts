import type { Material } from "../api/materials";

export interface AlbumImage { kind: "cover" | "version"; id: number; label: string }

/** Missing version pictures share one cover; they must not create fake extra photos. */
export function materialAlbum(material: Material, chosenId?: number) {
  const versions = material.versions.length ? material.versions : [{ id: 0, name: "默认版本", text: material.text, hasImage: false }];
  const version = versions.find(item => item.id === chosenId) ?? versions[0];
  const cover: AlbumImage = { kind: "cover", id: material.id, label: material.title };
  const images: AlbumImage[] = [cover, ...versions.filter(item => item.hasImage).map(item => ({ kind: "version" as const, id: item.id, label: item.name }))];
  const front = version.hasImage ? images.find(item => item.kind === "version" && item.id === version.id)! : cover;
  const previews = [front, ...images.filter(item => item !== front)].slice(0, 3);
  return { versions, version, images, previews };
}
