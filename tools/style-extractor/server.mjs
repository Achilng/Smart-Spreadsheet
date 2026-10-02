import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { randomBytes } from 'node:crypto';
import { JobManager, root, listApiModels, normalizeBaseUrl, callApi, callCodex } from './core.mjs';
import { publicFetch } from './public-fetch.mjs';
import { ResumeController } from './resume.mjs';
import { readManagedConfig } from './managed-config.mjs';

const port = Number(process.env.STYLE_PORT || 17321);
const serverBase = process.env.STYLE_SERVER_API_BASE;
const managed = !!serverBase;
const publicBase = process.env.STYLE_API_PUBLIC_BASE || serverBase || '';
const configFile = process.env.STYLE_CHANNEL_CONFIG;
const simpleMode = !!configFile;
const routeBase = base => managed && (base === publicBase || base === serverBase) ? serverBase : normalizeBaseUrl(base);
const requestFor = base => managed && base !== serverBase ? publicFetch : fetch;
const manager = new JobManager(process.env.STYLE_DATA_DIR || path.join(root, 'data'), (batch, options, signal, log, emit) => {
  if (options.provider !== 'api') return callCodex(batch, options, signal, log, emit);
  const baseUrl = routeBase(options.baseUrl);
  return callApi(batch, { ...options, baseUrl, fetch: requestFor(baseUrl) }, signal, log, emit);
});
const origin = process.env.STYLE_PUBLIC_ORIGIN || `http://127.0.0.1:${port}`;
const resume = new ResumeController(manager, false);
const token = randomBytes(24).toString('hex');
const page = filename => fs.readFileSync(path.join(root, filename), 'utf8').replace('<script src="/channel-store.js"></script>', simpleMode ? '' : '<script src="/channel-store.js"></script>').replace('__SESSION_TOKEN__', token).replace('__SIMPLE_MODE__', JSON.stringify(simpleMode)).replace('__SIMPLE_CLASS__', simpleMode ? 'simple-mode' : '').replace('__SERVER_MANAGED__', JSON.stringify(managed)).replace('__API_PUBLIC_BASE__', JSON.stringify(simpleMode ? '' : publicBase).replaceAll('<', String.fromCharCode(92) + 'u003c'));
const server = http.createServer(async (req, res) => {
  const send = (status, value, type = 'application/json; charset=utf-8') => { res.writeHead(status, { 'Content-Type': type, 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', 'Content-Security-Policy': "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'" }); res.end(typeof value === 'string' ? value : JSON.stringify(value)); };
  try {
    if (![new URL(origin).host, `127.0.0.1:${port}`].includes(req.headers.host)) return send(403, { error: '访问地址不匹配。' });
    const url = new URL(req.url, `http://127.0.0.1:${port}`);
    if (simpleMode && ['/channels', '/channels.js', '/channel-store.js'].includes(url.pathname)) return send(404, { error: '页面不存在' });
    if (req.method === 'GET' && url.pathname === '/health') {
      const body = 'smart-spreadsheet.style-extractor.v1';
      res.writeHead(200, { 'Content-Type': 'text/plain', 'Content-Length': Buffer.byteLength(body), 'Cache-Control': 'no-store' });
      return res.end(body);
    }
    if (req.method === 'GET' && url.pathname === '/favicon.ico') { res.writeHead(204); return res.end(); }
    if (req.method === 'GET' && ['/', '/channels'].includes(url.pathname)) return send(200, page(url.pathname === '/' ? 'index.html' : 'channels.html'), 'text/html; charset=utf-8');
    if (req.method === 'GET' && ['/live.js', '/live.css', '/app.css', '/channel-store.js', '/channels.js', '/workspace.js'].includes(url.pathname)) return send(200, fs.readFileSync(path.join(root, url.pathname.slice(1)), 'utf8'), url.pathname.endsWith('.js') ? 'text/javascript; charset=utf-8' : 'text/css; charset=utf-8');
    if (!url.pathname.startsWith('/api/')) return send(404, { error: '页面不存在' });
    if (req.headers['x-session-token'] !== token) return send(403, { error: '服务已重新启动，请刷新网页后继续。' });
    if (req.headers.origin && req.headers.origin !== origin) return send(403, { error: '来源不匹配' });
    let body = {};
    if (req.method === 'POST') {
      const chunks = [];
      for await (const chunk of req) chunks.push(chunk);
      body = JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}');
    }
    if (req.method === 'POST' && url.pathname === '/api/credentials') {
      return send(410, { error: 'Key 仅保存在当前浏览器，请刷新网页；服务器不再提供凭据存取。' });
    }
    if (req.method === 'POST' && url.pathname === '/api/models') {
      if (simpleMode) return send(403, { error: '渠道由管理员配置。' });
      const base = routeBase(body.baseUrl || serverBase);
      const key = typeof body.apiKey === 'string' ? body.apiKey.trim() : '';
      return send(200, { models: await listApiModels(base, key, requestFor(base)) });
    }
    if (req.method === 'GET' && url.pathname === '/api/jobs') return send(200, manager.list());
    if (req.method === 'POST' && url.pathname === '/api/jobs') return send(200, manager.create(body.request, simpleMode ? readManagedConfig(configFile) : managed ? { ...body.settings, provider: 'api', ...(body.settings?.channels ? {} : { baseUrl: serverBase }) } : body.settings));
    const match = /^\/api\/jobs\/([a-f0-9-]+)(?:\/(start|pause|cancel|result|events|lane|channels))?$/.exec(url.pathname);
    if (!match) return send(404, { error: '接口不存在' });
    const [, id, action] = match; const job = manager.get(id);
    if (req.method === 'GET' && action === 'lane') return send(200, manager.lane(id, Number(url.searchParams.get('slot')), url.searchParams.has('index') ? Number(url.searchParams.get('index')) : undefined));
    if (req.method === 'GET' && action === 'events') {
      res.writeHead(200, { 'Content-Type': 'text/event-stream; charset=utf-8', 'Cache-Control': 'no-store', Connection: 'keep-alive', 'X-Content-Type-Options': 'nosniff' });
      let previous = '', lastSent = 0;
      const push = () => {
        if (res.writableNeedDrain || res.destroyed) return;
        if (!manager.jobs.has(id)) { res.end(); return; }
        const offset = Math.max(0, Math.floor(Number(url.searchParams.get('offset')) || 0));
        const snapshot = manager.snapshot(id, offset);
        // Throttle clock-only events without discarding subsecond precision.
        const comparison = JSON.stringify({ ...snapshot, job: { ...snapshot.job, sampledAt: 0, elapsedMs: Math.floor(snapshot.job.elapsedMs / 1000) } });
        if (comparison !== previous) { res.write('data: ' + JSON.stringify(snapshot) + '\n\n'); previous = comparison; lastSent = Date.now(); }
        else if (Date.now() - lastSent > 10000) { res.write(': heartbeat\n\n'); lastSent = Date.now(); }
      };
      push(); const timer = setInterval(push, 250);
      res.on('close', () => clearInterval(timer)); return;
    }
    if (req.method === 'GET' && action === 'result') return send(200, manager.result(id));
    if (req.method === 'GET' && !action) return send(200, manager.summary(job));
    if (req.method === 'POST' && action === 'pause') { resume.pause(id); return send(200, manager.summary(job)); }
    if (req.method === 'POST' && action === 'cancel') { await resume.cancel(id); return send(200, { id, cancelled: true }); }
    if (req.method === 'POST' && action === 'channels') {
      if (simpleMode) return send(403, { error: '渠道由管理员配置。' });
      if (manager.active?.id !== id || !manager.active.update) throw Error('只有正在运行的多渠道任务可以应用渠道调整。');
      manager.active.update(body.channels);
      return send(200, manager.summary(job));
    }
    if (req.method === 'POST' && action === 'start') {
      if (manager.active) return send(409, { error: `任务 ${manager.active.id.slice(0, 8)} 正在运行，请先查看或暂停该任务。`, activeJobId: manager.active.id });
      // start executes synchronously through validation before its first await.
      if (managed && job.settings.provider !== 'api') throw Error('服务器仅支持 API 渠道。');
      resume.start(id, !!body.retryErrors, simpleMode ? { channels: readManagedConfig(configFile).channels } : { apiKey: typeof body.apiKey === 'string' ? body.apiKey.trim() : '', concurrency: body.concurrency, channels: body.channels });
      await Promise.resolve(); if (!manager.active && job.state !== 'completed') throw Error(job.message);
      return send(200, manager.summary(job));
    }
    send(405, { error: '不支持的操作' });
  } catch (error) { send(400, { error: error.message }); }
});
server.listen(port, '127.0.0.1', () => {
  console.log(`画风提取工具：http://127.0.0.1:${port}\n任务保存于：${manager.directory}\n关闭后重新启动可恢复任务。按 Ctrl+C 停止。`);
  // Keys exist only in a running request/job closure. After restart the browser must supply one again.
});
server.on('error', error => { console.error(error.message); process.exitCode = 1; });
async function shutdown() {
  const timer = setTimeout(() => process.exit(1), 20000); timer.unref();
  server.close(); await resume.stop(); process.exit(0);
}
process.on('SIGINT', shutdown); process.on('SIGTERM', shutdown);
