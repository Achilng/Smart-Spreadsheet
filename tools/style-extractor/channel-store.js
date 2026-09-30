(() => {
  const storageKey = 'style-channels-v1';
  function load() {
    const raw = localStorage.getItem(storageKey);
    if (raw !== null) {
      const data = JSON.parse(raw);
      if (!Array.isArray(data)) throw Error('渠道配置无法读取，请勿清除浏览器数据，联系维护者恢复。');
      return data;
    }
    const channels = [];
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (!key.startsWith('style-api-key:')) continue;
      const baseUrl = key.slice('style-api-key:'.length);
      channels.push({ id: crypto.randomUUID(), name: baseUrl === publicApiBase ? '默认渠道' : `已保存渠道 ${channels.length + 1}`, baseUrl, apiKey: localStorage.getItem(key), model: baseUrl === publicApiBase ? 'gpt-6-luna' : 'gpt-6-luna【神秘】', concurrency: 1, enabled: true });
    }
    if (!channels.length) channels.push({ id: crypto.randomUUID(), name: '默认渠道', baseUrl: publicApiBase || 'https://oneapi.hakoyu.com/v1', apiKey: '', model: publicApiBase ? 'gpt-6-luna' : 'gpt-6-luna【神秘】', concurrency: 1, enabled: true });
    save(channels); return channels;
  }
  function save(channels) { localStorage.setItem(storageKey, JSON.stringify(channels)); }
  function validate(channel) {
    const base = new URL(channel.baseUrl);
    if ((base.protocol !== 'https:' && !(base.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(base.hostname))) || base.username || base.password || base.search || base.hash) throw Error('请输入 HTTPS Base URL，不能包含账号、参数或片段。');
    if (!channel.name.trim() || channel.name.length > 100) throw Error('渠道名称应为 1–100 字。');
    if (!channel.model || channel.model.length > 128 || /[\s\x00-\x1f\x7f]/u.test(channel.model)) throw Error('请填写有效模型名，不含空白，最长 128 字符。');
    if (!Number.isSafeInteger(channel.concurrency) || channel.concurrency < 1) throw Error('并发数应为正整数。');
    return { ...channel, baseUrl: base.toString().replace(/\/+$/, '') };
  }
  window.channelStore = { load, save, validate, storageKey, metadata: values => values.map(({ apiKey, ...value }) => value) };
})();
