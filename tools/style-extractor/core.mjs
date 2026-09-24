import { createHash, randomUUID } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { setMaxListeners } from 'node:events';
import { ItemStream, readSse } from './streaming.mjs';
import { codexStream } from './codex-stream.mjs';

export const root = path.dirname(fileURLToPath(import.meta.url));
export const REQUEST = 'smart-spreadsheet.style-extraction.request';
export const RESULT = 'smart-spreadsheet.style-extraction.result';
export const MODEL = 'gpt-6-luna';
export const PROMPT_VERSION = 'style-extraction-v2';
export const hash = text => 'sha256:' + createHash('sha256').update(text, 'utf8').digest('hex');
const prompt = fs.readFileSync(path.join(root, 'prompt.txt'), 'utf8');
const promptHash = hash(prompt);
export function validateRequest(doc) {
  if (doc?.format !== REQUEST || doc.version !== 1 || typeof doc.export_id !== 'string' || !doc.export_id || !Array.isArray(doc.items) || !doc.items.length) throw Error('不是有效的待处理 JSON（需要非空 items、version 1）。');
  const seen = new Set();
  for (const item of doc.items) {
    if (typeof item.positive_prompt !== 'string' || !item.positive_prompt.trim() || item.id !== hash(item.positive_prompt)) throw Error('正文为空或正文哈希不匹配。');
    if (seen.has(item.id)) throw Error('输入包含重复编号，请重新导出去重后的文件。');
    seen.add(item.id);
  }
  return doc;
}
export function validateReply(batch, reply) {
  if (!Array.isArray(reply?.items)) throw Error('模型未返回 items 数组。');
  return batch.map(input => {
    const found = reply.items.filter(x => x?.id === input.id);
    const r = found[0];
    let error;
    if (!found.length) error = '本条结果缺失：模型未返回对应编号，或返回了未知编号';
    else if (found.length > 1) error = '本条编号重复，无法唯一匹配结果';
    else if (r.status === 'none' && r.artist_string === '') return { id: input.id, status: 'none', artist_string: '' };
    else if (r.status === 'ok' && typeof r.artist_string === 'string' && r.artist_string.length && input.positive_prompt.includes(r.artist_string)) return { id: input.id, status: 'ok', artist_string: r.artist_string };
    else error = '结果状态不正确，或画风串不是连续原文';
    return { id: input.id, status: 'error', artist_string: '', error };
  });
}
export function modelBatch(batch) {
  // Model IDs are local to one call. Stable hashes never leave this adapter.
  return batch.map((item, index) => ({ id: String(index + 1), positive_prompt: item.positive_prompt }));
}
export function restoreModelReply(batch, reply, log) {
  if (!Array.isArray(reply?.items)) throw Error('模型未返回 items 数组。');
  const ids = new Map(batch.map((item, i) => [String(i + 1), item.id]));
  const items = []; let unknown = 0;
  for (const item of reply.items) {
    // Accept 1 and "1", but never guess by output position or artist text.
    const shortId = typeof item?.id === 'string' ? item.id : Number.isInteger(item?.id) ? String(item.id) : '';
    const id = ids.get(shortId);
    if (!id) { unknown++; continue; }
    items.push({ id, status: item.status, artist_string: item.artist_string });
  }
  if (unknown) log(`忽略 ${unknown} 条未知编号结果，保留可匹配项，仅重试缺失或异常项`);
  return { items };
}
export function atomicJson(filename, value) {
  fs.mkdirSync(path.dirname(filename), { recursive: true });
  const temp = filename + '.tmp';
  const fd = fs.openSync(temp, 'w');
  try { fs.writeFileSync(fd, JSON.stringify(value, null, 2), 'utf8'); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
  fs.renameSync(temp, filename);
}
export function resolveCodex({ env = process.env, platform = process.platform, run = spawnSync } = {}) {
  if (env.STYLE_CODEX_BIN) return { command: env.STYLE_CODEX_BIN, prefix: [] };
  if (platform !== 'win32') return { command: 'codex', prefix: [] };
  const r = run('powershell.exe', ['-NoProfile', '-Command', 'ConvertTo-Json -Compress -InputObject @((Get-Command codex -All -ErrorAction SilentlyContinue).Source)'], { encoding: 'utf8', windowsHide: true, timeout: 10000 });
  let commands = [];
  try { const values = JSON.parse(r.stdout); commands = (Array.isArray(values) ? values : [values]).filter(x => typeof x === 'string' && fs.existsSync(x)); } catch { /* App discovery below also works without a PATH entry. */ }
  // PATH often lists an older npm shim first. Prefer the desktop app's registered CLI.
  const desktop = commands.find(x => /[\\/]OpenAI[\\/]Codex[\\/]bin[\\/](?:[^\\/]+[\\/])?codex\.exe$/i.test(x));
  if (desktop) return { command: desktop, prefix: [] };
  if (env.LOCALAPPDATA) {
    const bin = path.join(env.LOCALAPPDATA, 'OpenAI', 'Codex', 'bin');
    try {
      const candidates = [path.join(bin, 'codex.exe'), ...fs.readdirSync(bin, { withFileTypes: true }).filter(x => x.isDirectory()).map(x => path.join(bin, x.name, 'codex.exe'))];
      const installed = candidates.flatMap(command => { try { const stat = fs.statSync(command); return stat.isFile() ? [{ command, modified: stat.mtimeMs }] : []; } catch { return []; } }).sort((a, b) => b.modified - a.modified);
      if (installed.length) return { command: installed[0].command, prefix: [] };
    } catch { /* No desktop installation; retain the standalone CLI fallback. */ }
  }
  for (const located of commands) {
    if (located.toLowerCase().endsWith('.exe')) return { command: located, prefix: [] };
    const js = path.join(path.dirname(located), 'node_modules', '@openai', 'codex', 'bin', 'codex.js');
    if (fs.existsSync(js)) return { command: process.execPath, prefix: [js] };
    if (located.toLowerCase().endsWith('.ps1')) return { command: 'powershell.exe', prefix: ['-NoProfile', '-File', located] };
  }
  throw Error('没有找到可用的 Codex CLI；请先安装并登录，或设置 STYLE_CODEX_BIN 为 codex.exe 的完整路径。');
}
export function normalizeBaseUrl(value) {
  const url = new URL(value);
  if ((url.protocol !== 'https:' && !(url.protocol === 'http:' && ['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname))) || url.username || url.password || url.search || url.hash) throw Error('Base URL 需要 HTTPS 地址，或本机 HTTP 地址；不能包含账号、参数或片段。');
  return url.toString().replace(/\/+$/, '');
}
export function modelStream(batch, emit = () => {}) {
  const seen = new Set();
  return new ItemStream(event => {
    const short = String(event.item.id), input = batch.find((_, i) => String(i + 1) === short);
    if (!input) return;
    if (event.type === 'preview') return emit({ type: 'preview', id: input.id, text: event.item.artist_string });
    const result = seen.has(short) ? { id: input.id, status: 'error', artist_string: '', error: '本条编号重复，无法唯一匹配结果' }
      : validateReply([input], { items: [{ ...event.item, id: input.id }] })[0];
    seen.add(short); emit({ type: 'result', result });
  });
}
export async function callApi(batch, options, signal, log, emit = () => {}) {
  if (!options.apiKey?.trim()) throw Error('请填写 API Key 后继续（authentication）。');
  const base = normalizeBaseUrl(options.baseUrl), key = options.apiKey.trim();
  const items = modelBatch(batch);
  const body = {
    model: options.model || MODEL,
    messages: [{ role: 'system', content: prompt }, { role: 'user', content: JSON.stringify({ items }) }],
    stream: true,
    reasoning_effort: options.effort,
    response_format: { type: 'json_schema', json_schema: { name: 'style_extraction', strict: true, schema: modelSchema(items) } },
  };
  // Some compatible gateways do not implement schema or reasoning parameters.
  for (let pass = 0; pass < 4; pass++) {
    const response = await fetch(base + '/chat/completions', { method: 'POST', redirect: 'error', signal: AbortSignal.any([signal, AbortSignal.timeout(180000)]), headers: { Authorization: 'Bearer ' + key, 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    if (!response.ok) {
      const text = await response.text();
      let data; try { data = JSON.parse(text); } catch { data = { message: text }; }
      const detail = String(data.error?.message || data.message || response.statusText).split(key).join('[已隐藏]').slice(0, 1200);
      if (response.status === 400 && /response_format|json_schema|structured.output/i.test(detail) && body.response_format) { delete body.response_format; log('该接口不支持结构化输出参数，改为提示词约束 JSON，并保留本地校验'); continue; }
      if (response.status === 400 && /reasoning_effort|reasoning effort/i.test(detail) && body.reasoning_effort) { delete body.reasoning_effort; log('该接口不支持推理等级参数，使用服务端默认值'); continue; }
      if (response.status === 400 && /stream/i.test(detail) && body.stream) { body.stream = false; log('该接口不支持流式输出，等待整批返回后展示'); continue; }
      throw Error(`API 调用失败（${response.status}）：${detail}`);
    }
    const parser = modelStream(batch, emit);
    if (response.headers.get('content-type')?.includes('text/event-stream')) {
      emit({ type: 'mode', mode: 'stream' });
      let ended = false;
      await readSse(response.body, text => {
        if (text === '[DONE]') { ended = true; return; }
        const data = JSON.parse(text), choice = data.choices?.[0];
        if (data.error) throw Error(`API 流式错误：${String(data.error.message || '未知错误').split(key).join('[已隐藏]')}`);
        if (choice?.delta?.refusal) throw Error('模型拒绝处理这批文本');
        if (typeof choice?.delta?.content === 'string') parser.push(choice.delta.content);
        if (choice?.finish_reason) {
          if (choice.finish_reason !== 'stop') throw Error(`模型输出未正常结束（${choice.finish_reason}），已完成项保留`);
          ended = true;
        }
      });
      if (!ended) throw Error('响应流提前中断，已完成项保留');
      return restoreModelReply(batch, parser.finish(), log);
    }
    emit({ type: 'mode', mode: 'buffered' });
    log('接口返回非流式响应，整批返回后校验');
    const data = await response.json();
    const choice = data.choices?.[0];
    if (choice?.finish_reason === 'length') throw Error('模型输出被截断，请减少每批条数后新建任务');
    if (choice?.message?.refusal) throw Error('模型未处理这批文本（refusal），没有写入空结果');
    const content = choice?.message?.content;
    if (typeof content !== 'string') throw Error('API 未返回可读取的文本结果');
    log('API 已返回，正在校验原文');
    parser.push(content);
    return restoreModelReply(batch, parser.finish(), log);
  }
  throw Error('该接口不支持所需参数，请检查接口兼容性。');
}
const schema = { type: 'object', properties: { items: { type: 'array', items: { type: 'object', properties: { id: { type: 'string' }, status: { type: 'string', enum: ['ok', 'none'] }, artist_string: { type: 'string' } }, required: ['id', 'status', 'artist_string'], additionalProperties: false } } }, required: ['items'], additionalProperties: false };
function modelSchema(items) {
  const value = structuredClone(schema);
  value.properties.items.items.properties.id.enum = items.map(x => x.id);
  return value;
}
export async function callCodex(batch, options, signal, log, emit = () => {}) {
  const dir = path.join(process.env.STYLE_TEMP_DIR || 'D:/Agent/Agent_temp/style-extractor', randomUUID());
  fs.mkdirSync(dir, { recursive: true });
  const items = modelBatch(batch), cli = resolveCodex(), parser = modelStream(batch, emit);
  log(`Codex 流式连接：${cli.prefix.length ? cli.prefix.join(' ') : cli.command}`);
  emit({ type: 'mode', mode: 'stream' });
  await codexStream({ cli, cwd: dir, model: MODEL, effort: options.effort, prompt,
    input: JSON.stringify({ items }), schema: modelSchema(items), signal, onText: text => parser.push(text) });
  return restoreModelReply(batch, parser.finish(), log);
}
function validateConcurrency(value = 1) {
  const concurrency = Number(value);
  if (!Number.isInteger(concurrency) || concurrency < 1 || concurrency > 32) throw Error('并发数应为 1–32 的整数。');
  return concurrency;
}
export class JobManager {
  constructor(directory, runner = (batch, options, signal, log, emit) => options.provider === 'api' ? callApi(batch, options, signal, log, emit) : callCodex(batch, options, signal, log, emit)) {
    this.directory = directory; this.runner = runner; this.jobs = new Map(); this.active = null; this.live = new Map();
    fs.mkdirSync(directory, { recursive: true });
    for (const name of fs.readdirSync(directory).filter(x => /^[a-f0-9-]{36}\.json$/.test(x))) {
      try { const job = JSON.parse(fs.readFileSync(path.join(directory, name), 'utf8')); if (job.id + '.json' !== name) continue; job.request ??= JSON.parse(fs.readFileSync(path.join(directory, job.id + '.request.json'), 'utf8')); validateRequest(job.request); if (job.version !== 1 || !Array.isArray(job.results)) continue; job.settings.concurrency = validateConcurrency(job.settings.concurrency); if (job.state === 'running') { job.state = 'paused'; job.message = '上次运行中断，点击继续即可恢复。'; } this.jobs.set(job.id, job); } catch { /* Keep unreadable files intact. */ }
    }
  }
  save(job) {
    const inputPath = path.join(this.directory, job.id + '.request.json');
    if (!fs.existsSync(inputPath)) atomicJson(inputPath, job.request);
    const { request, ...state } = job;
    atomicJson(path.join(this.directory, job.id + '.json'), state);
  }
  create(request, settings = {}) {
    validateRequest(request);
    const concurrency = validateConcurrency(settings.concurrency);
    const batchSize = Number(settings.batchSize ?? 10), effort = settings.effort ?? 'high', provider = settings.provider ?? 'codex';
    if (!Number.isInteger(batchSize) || batchSize < 1 || batchSize > 100 || !['low', 'medium', 'high'].includes(effort)) throw Error('每批数量应为 1–100，推理等级应为 low / medium / high。');
    if (!['codex', 'api'].includes(provider)) throw Error('调用方式无效');
    const model = provider === 'api' ? String(settings.model || MODEL) : MODEL;
    if (!model.length || model.length > 128 || /[\s\x00-\x1f\x7f]/u.test(model)) throw Error('模型名应为 1–128 个字符，不能包含空白或控制字符');
    const baseUrl = provider === 'api' ? normalizeBaseUrl(settings.baseUrl) : undefined;
    const job = { version: 1, id: randomUUID(), request, model, promptVersion: PROMPT_VERSION, promptHash, settings: { batchSize, concurrency, effort, provider, model, ...(baseUrl ? { baseUrl } : {}) }, createdAt: new Date().toISOString(), elapsedMs: 0, calls: 0, state: 'paused', message: '任务已保存，等待开始', results: [], logs: [] };
    this.jobs.set(job.id, job); this.save(job); return this.summary(job);
  }
  get(id) { const job = this.jobs.get(id); if (!job) throw Error('任务不存在'); return job; }
  snapshot(id) {
    const job = this.get(id);
    return { job: this.summary(job), lanes: (this.live.get(id) || []).map(lane => ({
      slot: lane.slot, call: lane.call, attempt: lane.attempt, phase: lane.phase, mode: lane.mode,
      total: lane.items.length, done: lane.items.filter(x => ['ok', 'none'].includes(x.state)).length,
      states: lane.items.map(x => x.state), revision: lane.revision,
      current: lane.current == null ? null : { number: lane.current + 1, text: lane.items[lane.current].text.slice(0, 180), state: lane.items[lane.current].state },
    })) };
  }
  lane(id, slot, index) {
    this.get(id);
    const lane = this.live.get(id)?.find(x => x.slot === slot);
    if (!lane) return null;
    const selected = Number.isInteger(index) ? index : lane.current;
    const item = lane.items[selected];
    return item ? { slot, number: selected + 1, phase: lane.phase, ...item, revision: lane.revision } : null;
  }
  summary(job) {
    const ok = job.results.filter(x => x.status === 'ok').length, none = job.results.filter(x => x.status === 'none').length, errors = job.results.filter(x => x.status === 'error').length;
    return { id: job.id, exportId: job.request.export_id, settings: job.settings, createdAt: job.createdAt, elapsedMs: job.elapsedMs + (this.active?.id === job.id ? Date.now() - this.active.lastTick : 0), calls: job.calls, inFlight: this.active?.id === job.id ? this.active.inFlight : 0, state: job.state, message: job.message, total: job.request.items.length, ok, none, errors, pending: job.request.items.length - ok - none - errors, logs: job.logs, failures: job.results.filter(x => x.status === 'error').slice(0,100).map(x => ({ id: x.id, error: x.error })) };
  }
  list() { return [...this.jobs.values()].sort((a,b) => b.createdAt.localeCompare(a.createdAt)).map(x => this.summary(x)); }
  result(id) { const j = this.get(id); const results = new Map(j.results.map(x => [x.id, x])); return { format: RESULT, version: 1, export_id: j.request.export_id, processor: { model: j.model, prompt_version: j.promptVersion, prompt_hash: j.promptHash }, items: j.request.items.map(input => ({ ...input, ...(results.get(input.id) ?? { status: 'error', artist_string: '', error: '尚未处理' }) })) }; }
  log(job, message) { job.message = message; job.logs.push({ time: new Date().toISOString(), message }); job.logs = job.logs.slice(-100); this.save(job); }
  pause(id) { if (this.active?.id === id) { this.active.controller.abort(); this.log(this.get(id), '正在暂停，已完成结果已保存'); } }
  async start(id, retryErrors = false, credentials = {}) {
    if (this.active) throw Error('已有任务正在运行，请先暂停。');
    const job = this.get(id);
    if (job.promptHash !== promptHash) throw Error('提示词已变化，请创建新任务。旧任务结果仍可下载。');
    if (job.settings.provider === 'api' && !credentials.apiKey?.trim()) throw Error('请填写该任务的 API Key 后继续。');
    job.settings.concurrency = validateConcurrency(credentials.concurrency ?? job.settings.concurrency);
    if (retryErrors) job.results = job.results.filter(x => x.status !== 'error');
    const controller = new AbortController(); this.active = { id, controller, lastTick: Date.now(), inFlight: 0 }; job.state = 'running';
    setMaxListeners(job.settings.concurrency + 2, controller.signal);
    let lastTick = Date.now();
    const tick = () => { const now = Date.now(); job.elapsedMs += now - lastTick; lastTick = now; if (this.active?.id === id) this.active.lastTick = now; this.save(job); };
    const timer = setInterval(() => { try { tick(); } catch (error) { job.message = `保存进度失败：${error.message}`; controller.abort(); } }, 5000);
    let stopReason;
    try {
      this.log(job, `开始处理 · ${job.settings.concurrency} 并发`);
      const done = new Set(job.results.map(x => x.id));
      const queue = job.request.items.filter(x => !done.has(x.id));
      const lanes = []; this.live.set(id, lanes);
      const worker = async slot => {
        while (queue.length && !controller.signal.aborted) {
          // Claim a distinct batch synchronously before yielding to other workers.
          let length = 0; const batch = [];
          while (queue.length && batch.length < job.settings.batchSize) {
            const next = queue[0]; if (batch.length && length + next.positive_prompt.length > 60000) break;
            batch.push(queue.shift()); length += next.positive_prompt.length;
          }
          const lane = { slot, call: 0, attempt: 0, phase: 'waiting', mode: 'stream', current: null, revision: 0,
            items: batch.map(x => ({ id: x.id, source: x.positive_prompt, text: '', state: 'waiting' })) };
          lanes[slot - 1] = lane;
          const put = r => {
            const index = job.results.findIndex(x => x.id === r.id);
            if (index >= 0) job.results.splice(index, 1);
            if (r.status !== 'error') job.results.push({ ...r, processed_at: new Date().toISOString() });
            this.save(job);
          };
          const event = e => {
            if (controller.signal.aborted) return;
            lane.revision++;
            if (e.type === 'mode') { lane.mode = e.mode; return; }
            const index = lane.items.findIndex(x => x.id === (e.id || e.result?.id));
            if (index < 0) return;
            lane.current = index; const item = lane.items[index];
            if (e.type === 'preview') { if (['ok', 'none'].includes(item.state)) return; item.text = e.text; item.state = 'streaming'; lane.phase = 'streaming'; }
            if (e.type === 'result') {
              put(e.result); item.text = e.result.artist_string; item.state = e.result.status; item.error = e.result.error;
              lane.phase = e.result.status === 'error' ? 'retry' : 'receiving';
            }
          };
          let pending = batch;
          for (let attempt = 0; attempt < 3 && pending.length && !controller.signal.aborted; attempt++) {
            let results;
            const call = ++job.calls;
            lane.call = call; lane.attempt = attempt; lane.phase = attempt ? 'retry' : 'waiting'; lane.revision++;
            try {
              this.log(job, `第 ${call} 次调用 · ${pending.length} 条${attempt ? ` · 重试 ${attempt}` : ''}`);
              this.active.inFlight++;
              try {
                results = validateReply(pending, await this.runner(pending, { ...job.settings, apiKey: credentials.apiKey }, controller.signal, message => {
                  if (!controller.signal.aborted) this.log(job, `第 ${call} 次调用 · ${message}`);
                }, event));
              } finally { this.active.inFlight--; }
              if (controller.signal.aborted) break;
            } catch (error) {
              if (controller.signal.aborted) break;
              if (/unauthorized|not supported|not found|authentication|登录|权限|401|403|quota|usage limit|rate limit|429|没有找到|无法定位/i.test(error.message)) {
                stopReason = error.message; controller.abort(); break;
              }
              results = pending.map(x => job.results.find(r => r.id === x.id && r.status !== 'error') || ({ id: x.id, status: 'error', artist_string: '', error: error.message }));
            }
            for (const r of results) {
              event({ type: 'result', result: r });
              if (r.status === 'error' && attempt === 2) job.results.push({ ...r, processed_at: new Date().toISOString() });
            }
            this.save(job);
            const failed = new Set(results.filter(x => x.status === 'error').map(x => x.id)); pending = pending.filter(x => failed.has(x.id));
            if (pending.length && attempt < 2) {
              lane.phase = 'retry'; lane.revision++;
              this.log(job, `第 ${call} 次调用 · ${pending.length} 条暂未通过，稍后重试`);
              await new Promise(resolve => {
                const t = setTimeout(end, 2000 * (attempt + 1)); const signal = controller.signal;
                function end() { clearTimeout(t); signal.removeEventListener('abort', end); resolve(); }
                signal.addEventListener('abort', end, { once: true });
                if (signal.aborted) end();
              });
            }
          }
          lane.phase = controller.signal.aborted ? 'paused' : pending.length ? 'error' : 'completed'; lane.revision++;
        }
      };
      // Drain all workers before releasing the job lock or allowing another start.
      await Promise.allSettled(Array.from({ length: Math.min(job.settings.concurrency, queue.length) }, (_, i) => worker(i + 1).catch(error => {
        stopReason ??= `任务停止：${error.message}`; controller.abort();
      })));
      job.state = controller.signal.aborted ? 'paused' : 'completed';
      this.log(job, stopReason || (job.state === 'paused' ? '已暂停，可继续剩余项' : '本轮处理完成'));
    } catch (error) { job.state = 'paused'; this.log(job, `任务停止：${error.message}`); }
    finally { clearInterval(timer); try { tick(); } finally { this.active = null; } }
  }
}
