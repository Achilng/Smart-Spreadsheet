let clearHistoryCallback: () => void = () => {};

/** 由历史模块在加载时注册，避免 app-state 反向依赖历史执行器。 */
export function registerHistoryClearer(clearer: () => void): void {
  clearHistoryCallback = clearer;
}

export function clearOperationHistory(): void {
  clearHistoryCallback();
}
