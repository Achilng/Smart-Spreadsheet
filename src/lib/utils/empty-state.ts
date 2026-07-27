import { rowStore } from "../stores/row-store.svelte";

/**
 * 结果集为空时按实际生效的筛选维度生成文案。
 * 搜索打错字时绝不能显示"没有数据行"——用户会以为资料库被清空。
 */
export function emptyResultText(): { text: string; canClear: boolean } {
  const parts: string[] = [];
  if (rowStore.search.trim() !== "") {
    parts.push(`搜索「${rowStore.search.trim()}」`);
  }
  if (rowStore.tags.length > 0) {
    parts.push(`${rowStore.tags.length} 个 Tag 筛选`);
  }
  if (rowStore.dedupe !== "none") {
    parts.push("去重筛选");
  }
  if (rowStore.singleArtistOnly) {
    parts.push("单画师筛选");
  }
  if (rowStore.hasVibe) {
    parts.push("VIBE 筛选");
  }
  if (rowStore.untaggedOnly) {
    parts.push("无 Tag 筛选");
  }
  if (rowStore.hideGrouped) {
    parts.push("隐藏已分组");
  }
  if (parts.length === 0) {
    return { text: "资料库中还没有图片。", canClear: false };
  }
  return {
    text: `${parts.join("＋")}下没有匹配的图片。`,
    canClear: true,
  };
}
