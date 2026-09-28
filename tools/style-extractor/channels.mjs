export function validateConcurrency(value = 1) {
  const n = Number(value);
  if (!Number.isSafeInteger(n) || n < 1) throw Error('并发数应为正整数。');
  return n;
}

export function normalizeBaseUrl(value) {
  const url = new URL(value);
  if ((url.protocol !== 'https:' && !(url.protocol === 'http:' && ['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname))) || url.username || url.password || url.search || url.hash) throw Error('Base URL 需要 HTTPS 地址，或本机 HTTP 地址；不能包含账号、参数或片段。');
  return url.toString().replace(/\/+$/, '');
}

export function normalizeChannels(values, credentials = false) {
  if (!Array.isArray(values) || !values.length) throw Error('请至少选择一个渠道。');
  const seen = new Set();
  const channels = values.map(value => {
    const id = String(value.id || ''), name = String(value.name || '').trim(), model = String(value.model || '').trim();
    if (!/^[a-zA-Z0-9_-]{1,80}$/.test(id) || seen.has(id)) throw Error('渠道编号无效或重复。');
    seen.add(id);
    if (!name || name.length > 100) throw Error('请填写渠道名称（不超过 100 字）。');
    if (!model || model.length > 128 || /[\s\x00-\x1f\x7f]/u.test(model)) throw Error('请填写有效模型名（不含空白，最长 128 字符）。');
    const channel = { id, name, baseUrl: normalizeBaseUrl(value.baseUrl), model, concurrency: validateConcurrency(value.concurrency), enabled: value.enabled !== false };
    if (credentials) {
      channel.apiKey = typeof value.apiKey === 'string' ? value.apiKey.trim() : '';
      if (channel.enabled && !channel.apiKey) throw Error(`请在渠道管理中填写「${name}」的 API Key。`);
    }
    return channel;
  });
  const total = channels.filter(x => x.enabled).reduce((sum, x) => sum + x.concurrency, 0);
  if (!Number.isSafeInteger(total)) throw Error('并发合计超过可精确表示的整数范围。');
  return channels;
}

export const totalConcurrency = channels => channels.filter(x => x.enabled).reduce((sum, x) => sum + x.concurrency, 0);
export const publicChannels = channels => channels.map(({ apiKey, ...channel }) => channel);
