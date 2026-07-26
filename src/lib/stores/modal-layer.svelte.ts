// 全局模态层栈：任何模态浮层（Modal、右键菜单、灯箱等）打开时登记，供
// 1) 全局快捷键（Delete/Ctrl+A/Ctrl+Z、Esc 清选区等）判断是否短路；
// 2) Esc 分层——只有栈顶的模态响应 Esc，避免一次按键连关多层。
//
// 注意：登记方通常在 $effect 里调用 push/pop，因此这两个函数内部
// 不得读取响应式状态（否则 effect 会依赖整个栈，互相触发无限循环）。
// tokens 用普通数组维护，反应式状态只写不读。
let nextToken = 1;
let tokens: number[] = [];

const layer = $state({ count: 0, top: 0 });

export function pushModalLayer(): number {
  const token = nextToken;
  nextToken += 1;
  tokens.push(token);
  layer.count = tokens.length;
  layer.top = token;
  return token;
}

export function popModalLayer(token: number): void {
  tokens = tokens.filter(t => t !== token);
  layer.count = tokens.length;
  layer.top = tokens.length > 0 ? tokens[tokens.length - 1] : 0;
}

export function isTopModalLayer(token: number): boolean {
  return layer.count > 0 && layer.top === token;
}

export function anyModalOpen(): boolean {
  return layer.count > 0;
}
