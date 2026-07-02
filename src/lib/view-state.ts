/**
 * 各主视图的滚动位置记忆：切走再切回时恢复上次浏览位置。
 * 筛选条件或数据集发生实质变化时统一清空（旧位置对新结果没有意义）；
 * 仅视图切换（如 画廊 ↔ 分组）不清空。
 */
const scrollPositions = new Map<string, number>();
let scrollPositionsVersion = 0;

export function saveScrollPosition(key: string, top: number): void {
  scrollPositions.set(key, top);
}

export function savedScrollPosition(key: string): number {
  return scrollPositions.get(key) ?? 0;
}

export function clearScrollPositions(): void {
  scrollPositions.clear();
  scrollPositionsVersion += 1;
}

export function scrollPositionVersion(): number {
  return scrollPositionsVersion;
}

/**
 * 恢复保存的滚动位置。挂载瞬间内容高度可能尚未就绪（成员异步加载、
 * content-visibility 离屏区块只有预估高度），首次赋值会被浏览器钳制；
 * 因此按帧重试直到生效，用户主动滚动或元素卸载时立即放弃。
 */
export function restoreScrollPosition(
  el: HTMLElement,
  key: string,
  maxFrames = 60,
  onApplied?: (top: number) => void,
): void {
  const target = savedScrollPosition(key);
  if (target <= 0) {
    return;
  }
  let applied = -1;
  let frames = 0;
  const attempt = (): void => {
    if (!el.isConnected || (applied >= 0 && Math.abs(el.scrollTop - applied) > 1)) {
      return;
    }
    el.scrollTop = target;
    applied = el.scrollTop;
    onApplied?.(applied);
    frames += 1;
    if (Math.abs(applied - target) > 1 && frames < maxFrames) {
      requestAnimationFrame(attempt);
    }
  };
  attempt();
}

/** 提示词文档：记住上次打开的文档，切走再切回时恢复（与筛选无关，不随滚动位置清空）。 */
let lastPromptDocId: string | null = null;

export function rememberPromptDoc(docId: string | null): void {
  lastPromptDocId = docId;
}

export function lastPromptDoc(): string | null {
  return lastPromptDocId;
}
