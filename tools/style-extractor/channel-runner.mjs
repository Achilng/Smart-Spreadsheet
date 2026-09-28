import { randomUUID } from 'node:crypto';
import { setMaxListeners } from 'node:events';
import { validateReply } from './core.mjs';
import { normalizeChannels, publicChannels, totalConcurrency } from './channels.mjs';

// One shared queue, with independent channel capacity. Never allocate workers from
// the configured limit: only actual pending batches create promises and lanes.
export async function runChannels(manager, job, retryErrors, values) {
  const initial = normalizeChannels(values, true);
  if (!initial.some(x => x.enabled)) throw Error('请至少启用一个渠道。');
  const controller = new AbortController();
  setMaxListeners(0, controller.signal);
  const active = { id: job.id, controller, runId: randomUUID(), lastTick: Date.now(), inFlight: 0, channels: [] };
  let wake = () => {}, stopReason = '', cursor = 0;
  const secrets = new Set(initial.map(x => x.apiKey).filter(Boolean));
  const clean = message => { let text = String(message); for (const key of secrets) text = text.split(key).join('[已隐藏]'); return text.slice(0, 1200); };
  const log = text => manager.log(job, clean(text));
  active.update = values => {
    const configs = normalizeChannels(values, true);
    configs.forEach(x => { if (x.apiKey) secrets.add(x.apiKey); });
    const old = new Map(active.channels.map(x => [x.config.id, x]));
    for (const state of active.channels) state.config = { ...state.config, enabled: false };
    const next = configs.map(config => {
      const state = old.get(config.id) || { inFlight: 0, calls: 0, success: 0, failures: 0 };
      Object.assign(state, { config, disabled: false, until: 0, rateFailures: 0, error: '' });
      old.delete(config.id); return state;
    });
    active.channels = [...next, ...[...old.values()].filter(x => x.inFlight)];
    job.settings.channels = publicChannels(configs);
    job.settings.provider = 'api';
    job.settings.concurrency = totalConcurrency(configs);
    job.settings.model = job.model = [...new Set(configs.map(x => x.model))].join(' / ');
    manager.save(job); wake();
  };
  active.stats = () => active.channels.map(x => ({ ...publicChannels([x.config])[0], inFlight: x.inFlight, calls: x.calls, success: x.success, failures: x.failures, error: x.error,
    state: !x.config.enabled ? 'disabled' : x.disabled ? 'unavailable' : x.until > Date.now() ? 'cooldown' : 'ready', retryAt: x.until || null }));
  job.results = job.results.map(result => result.model ? result : { ...result, model: job.model });
  active.update(initial);
  if (retryErrors) job.results = job.results.filter(x => x.status !== 'error');
  const saved = new Map(job.results.map(x => [x.id, x]));
  const queue = job.request.items.filter(x => !saved.has(x.id));
  const attempts = new Map(), tasks = new Set(), lanes = [];
  manager.live.set(job.id, lanes); manager.active = active; job.state = 'running';
  const save = () => { job.results = [...saved.values()]; manager.save(job); };
  const tick = () => { const now = Date.now(); job.elapsedMs += now - active.lastTick; active.lastTick = now; save(); };
  const fatal = error => { stopReason = clean(error.message); controller.abort(); wake(); };
  const timer = setInterval(() => { try { tick(); } catch (error) { fatal(error); } }, 5000);
  const abort = () => wake(); controller.signal.addEventListener('abort', abort);
  function batch() {
    const items = []; let length = 0;
    while (cursor < queue.length && items.length < job.settings.batchSize) {
      const next = queue[cursor];
      if (items.length && length + next.positive_prompt.length > 60000) break;
      cursor++; items.push(next); length += next.positive_prompt.length;
    }
    return items;
  }
  async function invoke(state, items, slot) {
    // Capture credentials and model: edits affect only subsequent requests.
    const config = { ...state.config }, call = ++job.calls;
    const currentConfig = () => state.config.baseUrl === config.baseUrl && state.config.apiKey === config.apiKey && state.config.model === config.model;
    const lane = { slot: slot + 1, channelId: config.id, channelName: config.name, model: config.model, call, attempt: Math.max(...items.map(x => attempts.get(x.id) || 0)), phase: 'waiting', mode: 'stream', current: null, revision: 0,
      items: items.map(x => ({ id: x.id, source: x.positive_prompt, text: '', state: 'waiting' })) };
    lanes[slot] = lane;
    const provenance = { channel_id: config.id, channel_name: config.name, model: config.model };
    const put = result => {
      const previous = saved.get(result.id);
      if (previous && previous.status !== 'error') state.success--;
      if (result.status === 'error') saved.delete(result.id);
      else { saved.set(result.id, { ...result, ...provenance, processed_at: new Date().toISOString() }); state.success++; }
      save();
    };
    const emit = event => {
      if (controller.signal.aborted) return;
      lane.revision++;
      if (event.type === 'mode') { lane.mode = event.mode; return; }
      const index = lane.items.findIndex(x => x.id === (event.id || event.result?.id));
      if (index < 0) return;
      lane.current = index; const item = lane.items[index];
      if (event.type === 'preview' && !['ok', 'none'].includes(item.state)) { item.text = event.text; item.state = 'streaming'; lane.phase = 'streaming'; }
      if (event.type === 'result') { put(event.result); item.text = event.result.artist_string; item.state = event.result.status; item.error = event.result.error; lane.phase = 'receiving'; }
    };
    items.forEach(x => attempts.set(x.id, (attempts.get(x.id) || 0) + 1));
    state.calls++; state.inFlight++; active.inFlight++;
    let message = '结果未通过原文校验', channelFault = false;
    try {
      const reply = await manager.runner(items, { ...job.settings, ...config, provider: 'api' }, controller.signal, text => { if (!controller.signal.aborted) log(`${config.name} · ${text}`); }, emit);
      if (!controller.signal.aborted) {
        for (const result of validateReply(items, reply)) emit({ type: 'result', result });
        if (currentConfig()) state.rateFailures = 0;
      }
    } catch (error) {
      if (!controller.signal.aborted) {
        message = clean(error.message);
        const rate = error.status === 429 || /rate limit|429/i.test(message);
        const terminal = [401, 403, 404].includes(error.status) || /unauthorized|authentication|权限|quota|usage limit|insufficient_quota|model.*not.*found/i.test(message);
        if (rate || terminal) {
          channelFault = true;
          if (currentConfig()) {
            state.error = message;
            if (rate && ++state.rateFailures < 3) state.until = Date.now() + Math.max(1000, error.retryAfterMs || 5000 * state.rateFailures);
            else state.disabled = true;
            log(`${config.name} · ${state.disabled ? '渠道暂不可用' : '限流，等待恢复'}：${message}`);
          }
        }
      }
    } finally {
      state.inFlight--; active.inFlight--;
    }
    const pending = items.filter(x => !saved.has(x.id));
    if (!controller.signal.aborted) {
      state.failures += pending.length;
      for (const item of pending) {
        // Invalid credentials disable a channel once; healthy channels still get
        // a chance to process its unfinished items, without exhausting item retries.
        if (channelFault) attempts.set(item.id, Math.max(0, attempts.get(item.id) - 1));
        if ((attempts.get(item.id) || 0) < 3) queue.push(item);
        else saved.set(item.id, { id: item.id, status: 'error', artist_string: '', error: message, ...provenance, processed_at: new Date().toISOString() });
      }
      // Network / invalid-response retries are bounded and briefly back off this channel.
      if (pending.length && !channelFault && currentConfig()) state.until = Math.max(state.until || 0, Date.now() + 1000);
      save();
    }
    lane.phase = controller.signal.aborted ? 'paused' : pending.length ? 'error' : 'completed'; lane.revision++;
  }
  try {
    log(`开始处理 · ${initial.filter(x => x.enabled).length} 个渠道 · ${totalConcurrency(initial)} 并发`);
    let round = 0;
    while (!controller.signal.aborted && (cursor < queue.length || tasks.size)) {
      let dispatched = false;
      for (let n = 0; cursor < queue.length && n < active.channels.length; n++) {
        const state = active.channels[round++ % active.channels.length];
        if (!state.config.enabled || state.disabled || state.until > Date.now() || state.inFlight >= state.config.concurrency) continue;
        const slot = lanes.findIndex(x => !['waiting', 'streaming', 'receiving'].includes(x.phase));
        const task = invoke(state, batch(), slot < 0 ? lanes.length : slot).catch(fatal).finally(() => { tasks.delete(task); wake(); });
        tasks.add(task); dispatched = true;
      }
      if (dispatched) { await new Promise(resolve => setImmediate(resolve)); continue; }
      if (!tasks.size && !active.channels.some(x => x.config.enabled && !x.disabled)) { stopReason = '没有可用渠道，请在渠道管理中修正配置后继续。'; break; }
      if (!tasks.size && cursor >= queue.length) break;
      await new Promise(resolve => {
        const times = active.channels.filter(x => x.config.enabled && !x.disabled && x.until > Date.now()).map(x => x.until - Date.now());
        const timeout = times.length ? setTimeout(done, Math.min(2147483647, Math.max(1, Math.min(...times)))) : null;
        function done() { clearTimeout(timeout); wake = () => {}; resolve(); }
        wake = done; if (controller.signal.aborted) done();
      });
    }
    await Promise.allSettled([...tasks]);
    job.state = controller.signal.aborted || cursor < queue.length ? 'paused' : 'completed';
    job.channelStats = active.stats();
    log(stopReason || (job.state === 'paused' ? '已暂停，可继续剩余项' : '本轮处理完成'));
  } catch (error) { fatal(error); await Promise.allSettled([...tasks]); job.state = 'paused'; log(`任务停止：${stopReason}`); }
  finally { clearInterval(timer); controller.signal.removeEventListener('abort', abort); try { tick(); } finally { manager.active = null; } }
}
