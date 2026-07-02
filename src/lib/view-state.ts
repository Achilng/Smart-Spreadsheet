/**
 * 各主视图的滚动位置记忆：切走再切回时恢复上次浏览位置。
 * 筛选条件或数据集发生实质变化时统一清空（旧位置对新结果没有意义）；
 * 仅视图切换（如 画廊 ↔ 分组）不清空。
 */
const scrollPositions = new Map<string, number>();

export function saveScrollPosition(key: string, top: number): void {
  scrollPositions.set(key, top);
}

export function savedScrollPosition(key: string): number {
  return scrollPositions.get(key) ?? 0;
}

export function clearScrollPositions(): void {
  scrollPositions.clear();
}

/** 提示词文档：记住上次打开的文档，切走再切回时恢复（与筛选无关，不随滚动位置清空）。 */
let lastPromptDocId: string | null = null;

export function rememberPromptDoc(docId: string | null): void {
  lastPromptDocId = docId;
}

export function lastPromptDoc(): string | null {
  return lastPromptDocId;
}
