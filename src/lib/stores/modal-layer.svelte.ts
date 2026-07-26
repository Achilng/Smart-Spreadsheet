// 全局模态层栈：任何模态浮层（Modal、灯箱等）打开时登记，供
// 1) 全局快捷键（Delete/Ctrl+A/Ctrl+Z、Esc 清选区等）判断是否短路；
// 2) Esc 分层——只有栈顶的模态响应 Esc，避免一次按键连关多层。
let nextToken = 1;

const layer = $state<{ tokens: number[] }>({ tokens: [] });

export function pushModalLayer(): number {
  const token = nextToken;
  nextToken += 1;
  layer.tokens = [...layer.tokens, token];
  return token;
}

export function popModalLayer(token: number): void {
  layer.tokens = layer.tokens.filter(t => t !== token);
}

export function isTopModalLayer(token: number): boolean {
  return layer.tokens.length > 0 && layer.tokens[layer.tokens.length - 1] === token;
}

export function anyModalOpen(): boolean {
  return layer.tokens.length > 0;
}
