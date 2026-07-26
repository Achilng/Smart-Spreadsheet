// 全局模态层计数：任何 Modal 打开时登记，供全局快捷键（Delete/Ctrl+A/Ctrl+Z、
// Esc 清选区等）判断是否应当短路，避免模态之下的资料库操作被误触发。
export const modalLayer = $state({ count: 0 });

export function pushModalLayer(): void {
  modalLayer.count += 1;
}

export function popModalLayer(): void {
  modalLayer.count = Math.max(0, modalLayer.count - 1);
}

export function anyModalOpen(): boolean {
  return modalLayer.count > 0;
}
