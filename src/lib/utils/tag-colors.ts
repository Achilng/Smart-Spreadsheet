export interface TagColorTone {
  background: string;
  text: string;
}

/** 用户选定的画廊 Tag 循环色板。 */
export const TAG_COLOR_PALETTE: readonly TagColorTone[] = [
  { background: "#3C66BF", text: "#FFFFFF" },
  { background: "#5F9E56", text: "#102B0E" },
  { background: "#D1A54C", text: "#302300" },
  { background: "#E07EAD", text: "#3B1027" },
  { background: "#C36537", text: "#1F0C04" },
  { background: "#714BCA", text: "#FFFFFF" },
];

let cachedLibrary: readonly { name: string }[] | null = null;
let cachedIndexes = new Map<string, number>();

/**
 * Tag 库由后端按名称稳定排序；按其序号循环取色，让侧栏与所有预览卡片保持一致。
 * 尚未进入 Tag 库的临时筛选项使用确定性哈希兜底。
 */
export function tagColorFor(
  name: string,
  libraryTags: readonly { name: string }[],
): TagColorTone {
  const normalized = name.trim().toLocaleLowerCase();
  if (cachedLibrary !== libraryTags) {
    cachedLibrary = libraryTags;
    cachedIndexes = new Map(
      libraryTags.map((tag, index) => [tag.name.trim().toLocaleLowerCase(), index]),
    );
  }
  const libraryIndex = cachedIndexes.get(normalized) ?? -1;
  const colorIndex = libraryIndex >= 0
    ? libraryIndex % TAG_COLOR_PALETTE.length
    : stableColorIndex(normalized);
  return TAG_COLOR_PALETTE[colorIndex];
}

function stableColorIndex(value: string): number {
  let hash = 2166136261;
  for (const character of value) {
    hash ^= character.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0) % TAG_COLOR_PALETTE.length;
}
