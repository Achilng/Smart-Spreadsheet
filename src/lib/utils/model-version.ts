/** 预览图左上角版本徽章的显示内容与配色档位。 */
export interface ModelVersionBadge {
  label: string;
  className: string;
}

/**
 * 把图片元数据里的完整模型名（如 NovelAI Diffusion V4.5 Full）
 * 映射成简写徽章；无法识别时返回 null（不显示徽章）。
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
  if (full === curated) {
    // 既没写 Full 也没写 Curated，或两者同时出现，视为无法识别
    return null;
  }
  const suffix = full ? "F" : "C";
  const stem = version.replace(".", "");
  return {
    label: `${version} ${suffix}`,
    className: `${stem}-${full ? "full" : "curated"}`,
  };
}
