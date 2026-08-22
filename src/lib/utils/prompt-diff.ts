/**
 * 并排对比用的提示词差异：把逗号（或换行，角色提示词按角色分行）分隔的
 * 提示词拆成项，做计数感知（multiset）比较，输出“仅左侧有 / 共有 / 仅右侧有”。
 * 比较忽略大小写与首尾空白，展示时保留原文。
 */
export interface PromptFieldDiff {
  onlyLeft: string[];
  shared: string[];
  onlyRight: string[];
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

  const onlyLeft: string[] = [];
  const shared: string[] = [];
  for (const token of leftTokens) {
    const count = rightCounts.get(token.normalized) ?? 0;
    if (count > 0) {
      rightCounts.set(token.normalized, count - 1);
      shared.push(token.display);
    } else {
      onlyLeft.push(token.display);
    }
  }

  const onlyRight: string[] = [];
  for (const token of rightTokens) {
    const count = rightCounts.get(token.normalized) ?? 0;
    if (count > 0) {
      rightCounts.set(token.normalized, count - 1);
      onlyRight.push(token.display);
    }
  }

  return { onlyLeft, shared, onlyRight };
}

interface PromptToken {
  normalized: string;
  display: string;
}

function tokenizePrompt(value: string | null | undefined): PromptToken[] {
  if (!value) return [];
  return value
    .split(/[,\n]/)
    .map(token => token.trim())
    .filter(Boolean)
    .map(token => ({ normalized: token.toLocaleLowerCase(), display: token }));
}
