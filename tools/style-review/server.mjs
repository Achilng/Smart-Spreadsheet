import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { validateResult, REVIEW } from './core.mjs';
import { randomBytes } from 'node:crypto';
import { ReviewStore } from './store.mjs';

const directory = path.dirname(fileURLToPath(import.meta.url));
const port = Number(process.env.STYLE_REVIEW_PORT || 17324);
const origin = process.env.STYLE_REVIEW_ORIGIN || `http://127.0.0.1:${port}`;
const store = process.env.STYLE_REVIEW_DATA_DIR ? new ReviewStore(process.env.STYLE_REVIEW_DATA_DIR) : null;
const token = randomBytes(24).toString('hex');
const file = process.argv[2];
let initial = null;
if (file) {
  const document = JSON.parse(fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, ''));
  validateResult(document.format === REVIEW ? document.source_result : document);
  initial = { filename: path.basename(file), document };
}
const files = new Map([['/', ['index.html', 'text/html']], ['/app.mjs', ['app.mjs', 'text/javascript']], ['/core.mjs', ['core.mjs', 'text/javascript']], ['/style.css', ['style.css', 'text/css']]]);
const server = http.createServer(async (req, res) => {
  const send = (code, content, type = 'text/plain') => {
    res.writeHead(code, { 'Content-Type': type + '; charset=utf-8', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', 'Content-Security-Policy': "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'" }); res.end(content);
  };
  if (![new URL(origin).host, `127.0.0.1:${port}`].includes(req.headers.host) || (req.headers.origin && req.headers.origin !== origin)) return send(403, '访问来源不匹配');
  try {
  if (req.method === 'GET' && req.url === '/config') return send(200, JSON.stringify({ serverSaved: !!store, token }), 'application/json');
  if (store && req.url === '/saved' && req.method === 'GET') return send(200, JSON.stringify(store.list()), 'application/json');
  const saved = /^\/saved\/([a-f0-9]{64})$/.exec(req.url);
  if (store && saved) {
    if (req.method === 'GET') return send(200, JSON.stringify(store.get(saved[1])), 'application/json');
    if (req.method === 'POST') {
      if (req.headers['x-session-token'] !== token) return send(403, '服务已重新启动，请刷新网页后继续');
      const chunks = [];
      for await (const chunk of req) chunks.push(chunk);
      store.save(saved[1], JSON.parse(Buffer.concat(chunks).toString('utf8')));
      return send(200, JSON.stringify({ saved: true }), 'application/json');
    }
  }
  if (req.method !== 'GET') return send(405, '只读服务');
  if (req.url === '/health') {
    const body = 'smart-spreadsheet.style-review.v1';
    res.writeHead(200, { 'Content-Type': 'text/plain', 'Content-Length': Buffer.byteLength(body), 'Cache-Control': 'no-store' });
    return res.end(body);
  }
  if (req.url === '/initial') return send(200, JSON.stringify(initial || (store?.list()[0] ? store.get(store.list()[0].id) : null)), 'application/json');
  if (req.url === '/favicon.ico') return send(204, '');
  const asset = files.get(req.url);
  if (!asset) return send(404, '没有这个页面');
  try { send(200, fs.readFileSync(path.join(directory, asset[0])), asset[1]); } catch { send(500, '读取页面失败'); }
  } catch (error) { send(400, error.message); }
});
server.listen(port, '127.0.0.1', () => console.log(`画风批改台：http://127.0.0.1:${port}\n${initial ? '已载入：' + initial.filename : '打开网页后选择结果 JSON'}\n仅供人工复核，不调用模型，不改写原始结果。`));
server.on('error', error => { console.error(error.message); process.exitCode = 1; });
