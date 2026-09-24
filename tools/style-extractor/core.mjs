import { createHash, randomUUID } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

export const root = path.dirname(fileURLToPath(import.meta.url));
export const REQUEST = 'smart-spreadsheet.style-extraction.request';
export const RESULT = 'smart-spreadsheet.style-extraction.result';
export const MODEL = 'gpt-6-luna';
export const PROMPT_VERSION = 'style-extraction-v1';
export const hash = text => 'sha256:' + createHash('sha256').update(text, 'utf8').digest('hex');
const prompt = fs.readFileSync(path.join(root, 'prompt.txt'), 'utf8');
const promptHash = hash(prompt);
export function validateRequest(doc) {
  if (doc?.format !== REQUEST || doc.version !== 1 || typeof doc.export_id !== 'string' || !Array.isArray(doc.items) || !doc.items.length) throw Error('不是有效的待处理 JSON（需要非空 items、version 1）。');
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
  const allowed = new Set(batch.map(x => x.id));
  if (reply.items.some(x => !x || !allowed.has(x.id))) throw Error('模型返回了未知编号。');
  return batch.map(input => {
    const found = reply.items.filter(x => x.id === input.id);
    const r = found[0];
    let error;
    if (found.length !== 1) error = '结果缺失或编号重复';
    else if (r.status === 'none' && r.artist_string === '') return { id: input.id, status: 'none', artist_string: '' };
    else if (r.status === 'ok' && typeof r.artist_string === 'string' && r.artist_string.length && input.positive_prompt.includes(r.artist_string)) return { id: input.id, status: 'ok', artist_string: r.artist_string };
    else error = '结果状态不正确，或画风串不是连续原文';
    return { id: input.id, status: 'error', artist_string: '', error };
  });
}
export function atomicJson(filename, value) {
  fs.mkdirSync(path.dirname(filename), { recursive: true });
  const temp = filename + '.tmp';
  const fd = fs.openSync(temp, 'w');
  try { fs.writeFileSync(fd, JSON.stringify(value, null, 2), 'utf8'); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
  fs.renameSync(temp, filename);
}
export function resolveCodex() {
  if (process.env.STYLE_CODEX_BIN) return { command: process.env.STYLE_CODEX_BIN, prefix: [] };
  if (process.platform !== 'win32') return { command: 'codex', prefix: [] };
  const r = spawnSync('powershell.exe', ['-NoProfile', '-Command', '(Get-Command codex -ErrorAction Stop).Source'], { encoding: 'utf8', windowsHide: true });
  if (r.status !== 0) throw Error('没有找到 Codex CLI，请先安装并登录 Codex。');
  const located = r.stdout.trim();
  const js = path.join(path.dirname(located), 'node_modules', '@openai', 'codex', 'bin', 'codex.js');
  if (fs.existsSync(js)) return { command: process.execPath, prefix: [js] };
  if (located.endsWith('.exe')) return { command: located, prefix: [] };
  if (located.endsWith('.ps1')) return { command: 'powershell.exe', prefix: ['-NoProfile', '-File', located] };
  throw Error('无法定位 Codex 程序；可设置 STYLE_CODEX_BIN 为 codex.exe 的完整路径。');
}
const schema = { type: 'object', properties: { items: { type: 'array', items: { type: 'object', properties: { id: { type: 'string' }, status: { type: 'string', enum: ['ok', 'none'] }, artist_string: { type: 'string' } }, required: ['id', 'status', 'artist_string'], additionalProperties: false } } }, required: ['items'], additionalProperties: false };
export async function callCodex(batch, options, signal, log) {
  const dir = path.join(process.env.STYLE_TEMP_DIR || 'D:/Agent/Agent_temp/style-extractor', randomUUID());
  fs.mkdirSync(dir, { recursive: true });
  const output = path.join(dir, 'result.json');
  atomicJson(path.join(dir, 'schema.json'), schema);
  const cli = resolveCodex();
  const args = [...cli.prefix, 'exec', '--ignore-user-config', '--ephemeral', '--sandbox', 'read-only', '--skip-git-repo-check', '-C', dir, '-m', MODEL, '-c', `model_reasoning_effort="${options.effort}"`, '--output-schema', path.join(dir, 'schema.json'), '-o', output, '-'];
  await new Promise((resolve, reject) => {
    let last = '', finished = false;
    const child = spawn(cli.command, args, { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    const kill = () => { if (process.platform === 'win32' && child.pid) spawn('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' }); else child.kill('SIGTERM'); };
    const abort = () => kill();
    signal.addEventListener('abort', abort, { once: true });
    const timeout = setTimeout(() => { last = '调用超过 10 分钟，已停止。'; kill(); }, 600000);
    const finish = error => { if (finished) return; finished = true; clearTimeout(timeout); signal.removeEventListener('abort', abort); error ? reject(error) : resolve(); };
    child.stdout.on('data', () => {});
    child.stderr.on('data', data => { last = (last + data.toString('utf8')).slice(-12000); });
    child.on('error', error => finish(error));
    child.on('close', code => { if (signal.aborted) return finish(Error('已暂停')); if (code !== 0) return finish(Error(`Codex 调用失败（${code}）：${last.slice(-3000)}`)); finish(); });
    child.stdin.on('error', () => {});
    child.stdin.end(prompt + '\n\n【待处理 JSON 数据】\n' + JSON.stringify({ items: batch }));
    if (signal.aborted) abort();
  });
  log('模型已返回，正在校验原文');
  return JSON.parse(fs.readFileSync(output, 'utf8').replace(/^\uFEFF/, ''));
}
export class JobManager {
  constructor(directory, runner = callCodex) {
    this.directory = directory; this.runner = runner; this.jobs = new Map(); this.active = null;
    fs.mkdirSync(directory, { recursive: true });
    for (const name of fs.readdirSync(directory).filter(x => x.endsWith('.json'))) {
      try { const job = JSON.parse(fs.readFileSync(path.join(directory, name), 'utf8')); validateRequest(job.request); if (job.version !== 1 || !Array.isArray(job.results)) continue; if (job.state === 'running') { job.state = 'paused'; job.message = '上次运行中断，点击继续即可恢复。'; } this.jobs.set(job.id, job); } catch { /* Keep unreadable files intact. */ }
    }
  }
  save(job) { atomicJson(path.join(this.directory, job.id + '.json'), job); }
  create(request, settings = {}) {
    validateRequest(request);
    const batchSize = Number(settings.batchSize ?? 10), effort = settings.effort ?? 'high';
    if (!Number.isInteger(batchSize) || batchSize < 1 || batchSize > 100 || !['low', 'medium', 'high'].includes(effort)) throw Error('每批数量应为 1–100，推理等级应为 low / medium / high。');
    const job = { version: 1, id: randomUUID(), request, model: MODEL, promptVersion: PROMPT_VERSION, promptHash, settings: { batchSize, effort }, createdAt: new Date().toISOString(), elapsedMs: 0, calls: 0, state: 'paused', message: '任务已保存，等待开始', results: [], logs: [] };
    this.jobs.set(job.id, job); this.save(job); return this.summary(job);
  }
  get(id) { const job = this.jobs.get(id); if (!job) throw Error('任务不存在'); return job; }
  summary(job) {
    const ok = job.results.filter(x => x.status === 'ok').length, none = job.results.filter(x => x.status === 'none').length, errors = job.results.filter(x => x.status === 'error').length;
    return { id: job.id, exportId: job.request.export_id, settings: job.settings, createdAt: job.createdAt, elapsedMs: job.elapsedMs, calls: job.calls, state: job.state, message: job.message, total: job.request.items.length, ok, none, errors, pending: job.request.items.length - ok - none - errors, logs: job.logs, failures: job.results.filter(x => x.status === 'error').map(x => ({ id: x.id, error: x.error })) };
  }
  list() { return [...this.jobs.values()].sort((a,b) => b.createdAt.localeCompare(a.createdAt)).map(x => this.summary(x)); }
  result(id) { const j = this.get(id); const results = new Map(j.results.map(x => [x.id, x])); return { format: RESULT, version: 1, export_id: j.request.export_id, processor: { model: j.model, prompt_version: j.promptVersion, prompt_hash: j.promptHash }, items: j.request.items.map(input => ({ ...input, ...(results.get(input.id) ?? { status: 'error', artist_string: '', error: '尚未处理' }) })) }; }
  log(job, message) { job.message = message; job.logs.push({ time: new Date().toISOString(), message }); job.logs = job.logs.slice(-100); this.save(job); }
  pause(id) { if (this.active?.id === id) { this.active.controller.abort(); this.log(this.get(id), '正在暂停，已完成结果已保存'); } }
  async start(id, retryErrors = false) {
    if (this.active) throw Error('已有任务正在运行，请先暂停。');
    const job = this.get(id);
    if (job.promptHash !== promptHash || job.model !== MODEL) throw Error('提示词或模型已变化，请创建新任务。旧任务结果仍可下载。');
    if (retryErrors) job.results = job.results.filter(x => x.status !== 'error');
    const controller = new AbortController(); this.active = { id, controller }; job.state = 'running';
    let lastTick = Date.now();
    const tick = () => { const now = Date.now(); job.elapsedMs += now - lastTick; lastTick = now; this.save(job); };
    const timer = setInterval(tick, 1000);
    this.log(job, '开始处理');
    try {
      const done = new Set(job.results.map(x => x.id));
      let queue = job.request.items.filter(x => !done.has(x.id));
      while (queue.length && !controller.signal.aborted) {
        let length = 0; const batch = [];
        while (queue.length && batch.length < job.settings.batchSize) {
          const next = queue[0]; if (batch.length && length + next.positive_prompt.length > 60000) break;
          batch.push(queue.shift()); length += next.positive_prompt.length;
        }
        let pending = batch;
        for (let attempt = 0; attempt < 3 && pending.length && !controller.signal.aborted; attempt++) {
          let results;
          try {
            job.calls++; this.log(job, `第 ${job.calls} 次调用 · ${pending.length} 条${attempt ? ` · 重试 ${attempt}` : ''}`);
            results = validateReply(pending, await this.runner(pending, job.settings, controller.signal, message => this.log(job, message)));
          } catch (error) {
            if (controller.signal.aborted) break;
            if (/unauthorized|not supported|not found|authentication|登录|权限|401|403|quota|usage limit|rate limit|429|没有找到|无法定位/i.test(error.message)) { this.log(job, error.message); job.state = 'paused'; return; }
            results = pending.map(x => ({ id: x.id, status: 'error', artist_string: '', error: error.message }));
          }
          for (const r of results) if (r.status !== 'error' || attempt === 2) job.results.push({ ...r, processed_at: new Date().toISOString() });
          this.save(job);
          const failed = new Set(results.filter(x => x.status === 'error').map(x => x.id)); pending = pending.filter(x => failed.has(x.id));
          if (pending.length && attempt < 2) { this.log(job, `${pending.length} 条暂未通过，稍后重试`); await new Promise(resolve => { const t = setTimeout(end, 2000 * (attempt + 1)); const s = controller.signal; function end() { clearTimeout(t); s.removeEventListener('abort', end); resolve(); } s.addEventListener('abort', end, { once: true }); }); }
        }
      }
      job.state = controller.signal.aborted ? 'paused' : 'completed'; this.log(job, job.state === 'paused' ? '已暂停，可继续剩余项' : '本轮处理完成');
    } catch (error) { job.state = 'paused'; this.log(job, `任务停止：${error.message}`); }
    finally { clearInterval(timer); tick(); this.active = null; this.save(job); }
  }
}
