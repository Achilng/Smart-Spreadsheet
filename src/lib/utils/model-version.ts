/** 预览图左上角版本徽章的显示内容与配色档位。 */
export interface ModelVersionBadge {
  label: string;
  className: string;
}

/**
 * 社区已查明的新版 Source 哈希后缀 → 具体变体。NovelAI 网站现在的出图
 * 不再写 Full/Curated，而是在模型名后附 8 位十六进制构建哈希（如
 * "NovelAI Diffusion V4.5 4BDE2A90"），此表按哈希识别已知构建。
 */
const KNOWN_MODEL_HASHES: ReadonlyMap<string, { version: string; full: boolean }> = new Map([
  ["4bde2a90", { version: "v4.5", full: true }], // V4.5 Full
  ["37442fca", { version: "v4", full: true }], // V4 Full
  ["4f49ec75", { version: "v4", full: true }], // V4 Full（早期构建）
  ["f6e18726", { version: "v4", full: false }], // V4 Curated Preview
  ["79f47848", { version: "v4", full: false }], // V4 Curated
]);

/**
 * 把图片元数据里的完整模型名（如 NovelAI Diffusion V4.5 Full 或带哈希
 * 后缀的 NovelAI Diffusion V4.5 4BDE2A90）映射成简写徽章；无法识别时
 * 返回 null（不显示徽章）。
 */
export function modelVersionBadge(
  model: string | null | undefined,
): ModelVersionBadge | null {
  const normalized = model?.trim().toLowerCase();
  if (!normalized) {
    return null;
  }
  // Furry V3 要先于普通 V3 判断，v4.5 要先于 v4 判断（子串包含关系）
  if (normalized.includes("furry") && normalized.includes("v3")) {
    return { label: "v3 FR", className: "v3-furry" };
  }
  if (normalized.includes("v5")) {
    return variantBadge("v5", normalized);
  }
  if (normalized.includes("v4.5")) {
    return variantBadge("v4.5", normalized);
  }
  if (normalized.includes("v4")) {
    return variantBadge("v4", normalized);
  }
  if (normalized.includes("v3")) {
    return { label: "v3", className: "v3-anime" };
  }
  return null;
}

function variantBadge(version: string, normalized: string): ModelVersionBadge | null {
  const full = normalized.includes("full");
  const curated = normalized.includes("curated");
  const stem = version.replace(".", "");
  // 模型名不带 Full/Curated 后缀时（如首发 V5 的 “NovelAI Diffusion V5”
  // 或带构建哈希的 “NovelAI Diffusion V4.5 4BDE2A90”），先按末尾哈希查
  // 已知构建表，查不到再降级为纯版本号徽章
  if (full === curated) {
    return hashedVariantBadge(normalized) ?? { label: version, className: `${stem}-plain` };
  }
  const suffix = full ? "F" : "C";
  return {
    label: `${version} ${suffix}`,
    className: `${stem}-${full ? "full" : "curated"}`,
  };
}

function hashedVariantBadge(normalized: string): ModelVersionBadge | null {
  const hash = /\b([0-9a-f]{8})\s*$/.exec(normalized)?.[1];
  const known = hash ? KNOWN_MODEL_HASHES.get(hash) : undefined;
  if (!known) {
    return null;
  }
  const stem = known.version.replace(".", "");
  return {
    label: `${known.version} ${known.full ? "F" : "C"}`,
    className: `${stem}-${known.full ? "full" : "curated"}`,
  };
}
