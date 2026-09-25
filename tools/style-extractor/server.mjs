import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { randomBytes } from 'node:crypto';
import { JobManager, root, listApiModels, normalizeBaseUrl } from './core.mjs';
import { ResumeController } from './resume.mjs';

const port = Number(process.env.STYLE_PORT || 17321);
const manager = new JobManager(process.env.STYLE_DATA_DIR || path.join(root, 'data'));
const serverBase = process.env.STYLE_SERVER_API_BASE;
const managed = !!serverBase;
const origin = process.env.STYLE_PUBLIC_ORIGIN || `http://127.0.0.1:${port}`;
const resume = new ResumeController(manager, false);
const token = randomBytes(24).toString('hex');
const page = fs.readFileSync(path.join(root, 'index.html'), 'utf8').replace('__SESSION_TOKEN__', token).replace('__SERVER_MANAGED__', JSON.stringify(managed)).replace('__API_PUBLIC_BASE__', JSON.stringify(process.env.STYLE_API_PUBLIC_BASE || serverBase || '').replaceAll('<', String.fromCharCode(92) + 'u003c'));
const server = http.createServer(async (req, res) => {
  const send = (status, value, type = 'application/json; charset=utf-8') => { res.writeHead(status, { 'Content-Type': type, 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', 'Content-Security-Policy': "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'" }); res.end(typeof value === 'string' ? value : JSON.stringify(value)); };
  try {
    if (![new URL(origin).host, `127.0.0.1:${port}`].includes(req.headers.host)) return send(403, { error: '访问地址不匹配。' });
    const url = new URL(req.url, `http://127.0.0.1:${port}`);
    if (req.method === 'GET' && url.pathname === '/health') {
      const body = 'smart-spreadsheet.style-extractor.v1';
      res.writeHead(200, { 'Content-Type': 'text/plain', 'Content-Length': Buffer.byteLength(body), 'Cache-Control': 'no-store' });
      return res.end(body);
    }
    if (req.method === 'GET' && url.pathname === '/favicon.ico') { res.writeHead(204); return res.end(); }
    if (req.method === 'GET' && url.pathname === '/') return send(200, page, 'text/html; charset=utf-8');
    if (req.method === 'GET' && ['/live.js', '/live.css'].includes(url.pathname)) return send(200, fs.readFileSync(path.join(root, url.pathname.slice(1)), 'utf8'), url.pathname.endsWith('.js') ? 'text/javascript; charset=utf-8' : 'text/css; charset=utf-8');
    if (!url.pathname.startsWith('/api/')) return send(404, { error: '页面不存在' });
    if (req.headers['x-session-token'] !== token) return send(403, { error: '服务已重新启动，请刷新网页后继续。' });
    if (req.headers.origin && req.headers.origin !== origin) return send(403, { error: '来源不匹配' });
    let body = {};
    if (req.method === 'POST') {
      const chunks = []; let size = 0;
      for await (const chunk of req) { size += chunk.length; if (size > 64 * 1024 * 1024) throw Error('文件超过 64 MB 上限'); chunks.push(chunk); }
      body = JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}');
    }
    if (req.method === 'POST' && url.pathname === '/api/credentials') {
      return send(410, { error: 'Key 仅保存在当前浏览器，请刷新网页；服务器不再提供凭据存取。' });
    }
    if (req.method === 'POST' && url.pathname === '/api/models') {
      const base = managed ? serverBase : normalizeBaseUrl(body.baseUrl);
      const key = typeof body.apiKey === 'string' ? body.apiKey.trim() : '';
      return send(200, { models: await listApiModels(base, key) });
    }
    if (req.method === 'GET' && url.pathname === '/api/jobs') return send(200, manager.list());
    if (req.method === 'POST' && url.pathname === '/api/jobs') return send(200, manager.create(body.request, managed ? { ...body.settings, provider: 'api', baseUrl: serverBase } : body.settings));
    const match = /^\/api\/jobs\/([a-f0-9-]+)(?:\/(start|pause|result|events|lane))?$/.exec(url.pathname);
    if (!match) return send(404, { error: '接口不存在' });
    const [, id, action] = match; const job = manager.get(id);
    if (req.method === 'GET' && action === 'lane') return send(200, manager.lane(id, Number(url.searchParams.get('slot')), url.searchParams.has('index') ? Number(url.searchParams.get('index')) : undefined));
    if (req.method === 'GET' && action === 'events') {
      res.writeHead(200, { 'Content-Type': 'text/event-stream; charset=utf-8', 'Cache-Control': 'no-store', Connection: 'keep-alive', 'X-Content-Type-Options': 'nosniff' });
      let previous = '', lastSent = 0;
      const push = () => {
        if (res.writableNeedDrain || res.destroyed) return;
        const snapshot = manager.snapshot(id);
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
    if (req.method === 'POST' && action === 'start') {
      if (manager.active) throw Error('已有任务正在运行，请先暂停。');
      // start executes synchronously through validation before its first await.
      if (managed && (job.settings.provider !== 'api' || job.settings.baseUrl !== serverBase)) throw Error('该任务不属于当前服务器接口，请重新创建任务。');
      resume.start(id, !!body.retryErrors, { apiKey: typeof body.apiKey === 'string' ? body.apiKey.trim() : '', concurrency: body.concurrency });
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
