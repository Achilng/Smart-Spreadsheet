/**
 * 并排对比用的提示词差异：把逗号/换行分隔的正向提示词拆成项，做
 * 计数感知（multiset）比较，输出“仅样本有 / 共有 / 仅目标有”。
 * 比较忽略大小写与首尾空白，展示保留原文；官方质量词标记为淡化显示
 * （与 Rust 端 `pipeline::style_signature` 的总表同一口径）。
 */

/** 官方质量词并集（剥权重外壳后比对，小写）。 */
const QUALITY_WORDS: ReadonlySet<string> = new Set([
  "location",
  "very aesthetic",
  "masterpiece",
  "no text",
  "best quality",
  "amazing quality",
  "absurdres",
  "rating:general",
]);

/** 必须整串精确命中的官方加权默认项（V4.5 Curated）。 */
const WEIGHTED_QUALITY_WORDS: ReadonlySet<string> = new Set(["-0.8::feet::"]);

export interface PromptToken {
  /** 展示原文（保留大小写与空格）。 */
  display: string;
  /** 比较键：小写、内部空白折叠。 */
  normalized: string;
  /** 官方质量词 → 淡化显示。 */
  isQuality: boolean;
}

export interface PromptFieldDiff {
  onlyLeft: PromptToken[];
  shared: PromptToken[];
  onlyRight: PromptToken[];
}

export function diffPromptField(
  left: string | null | undefined,
  right: string | null | undefined,
): PromptFieldDiff {
  const leftTokens = tokenizePrompt(left);
  const rightTokens = tokenizePrompt(right);

  // 右侧先按规范化文本计数；左侧逐项配对并扣减，扣完后剩余的就是仅右侧有
  const rightCounts = new Map<string, number>();
  for (const token of rightTokens) {
    rightCounts.set(token.normalized, (rightCounts.get(token.normalized) ?? 0) + 1);
  }

  const onlyLeft: PromptToken[] = [];
  const shared: PromptToken[] = [];
  for (const token of leftTokens) {
    const count = rightCounts.get(token.normalized) ?? 0;
    if (count > 0) {
      rightCounts.set(token.normalized, count - 1);
      shared.push(token);
    } else {
      onlyLeft.push(token);
    }
  }

  const onlyRight: PromptToken[] = [];
  for (const token of rightTokens) {
    const count = rightCounts.get(token.normalized) ?? 0;
    if (count > 0) {
      rightCounts.set(token.normalized, count - 1);
      onlyRight.push(token);
    }
  }

  return { onlyLeft, shared, onlyRight };
}

function tokenizePrompt(value: string | null | undefined): PromptToken[] {
  if (!value) return [];
  return value
    .split(/[,\n]/)
    .map(token => token.trim())
    .filter(Boolean)
    .map(token => {
      const normalized = token.split(/\s+/).join(" ").toLowerCase();
      return {
        display: token,
        normalized,
        isQuality: isQualityToken(normalized),
      };
    });
}

function isQualityToken(normalized: string): boolean {
  return (
    WEIGHTED_QUALITY_WORDS.has(normalized) ||
    QUALITY_WORDS.has(stripWeightShell(normalized))
  );
}

/** 剥离 NovelAI 权重外壳：嵌套 `{...}` / `[...]` 与数值权重 `N::...::`。 */
function stripWeightShell(tag: string): string {
  let current = tag;
  for (;;) {
    let changed = false;
    while (
      (current.startsWith("{") && current.endsWith("}")) ||
      (current.startsWith("[") && current.endsWith("]"))
    ) {
      current = current.slice(1, -1).trim();
      changed = true;
    }
    const numeric = /^(-?[\d.]+)::([\s\S]+)::$/.exec(current);
    if (numeric && numeric[1] && !Number.isNaN(Number(numeric[1]))) {
      current = numeric[2].trim();
      changed = true;
    }
    if (!changed) {
      return current;
    }
  }
}
