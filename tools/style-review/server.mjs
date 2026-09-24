import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { validateResult, REVIEW } from './core.mjs';

const directory = path.dirname(fileURLToPath(import.meta.url));
const port = Number(process.env.STYLE_REVIEW_PORT || 17324);
const file = process.argv[2];
let initial = null;
if (file) {
  if (fs.statSync(file).size > 64 * 1024 * 1024) throw Error('文件超过 64 MB');
  const document = JSON.parse(fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, ''));
  validateResult(document.format === REVIEW ? document.source_result : document);
  initial = { filename: path.basename(file), document };
}
const files = new Map([['/', ['index.html', 'text/html']], ['/app.mjs', ['app.mjs', 'text/javascript']], ['/core.mjs', ['core.mjs', 'text/javascript']], ['/style.css', ['style.css', 'text/css']]]);
const server = http.createServer((req, res) => {
  const send = (code, content, type = 'text/plain') => {
    res.writeHead(code, { 'Content-Type': type + '; charset=utf-8', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', 'Content-Security-Policy': "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'" }); res.end(content);
  };
  if (req.headers.host !== `127.0.0.1:${port}` || (req.headers.origin && req.headers.origin !== `http://127.0.0.1:${port}`)) return send(403, '请使用本机地址');
  if (req.method !== 'GET') return send(405, '只读服务');
  if (req.url === '/initial') return send(200, JSON.stringify(initial), 'application/json');
  if (req.url === '/favicon.ico') return send(204, '');
  const asset = files.get(req.url);
  if (!asset) return send(404, '没有这个页面');
  try { send(200, fs.readFileSync(path.join(directory, asset[0])), asset[1]); } catch { send(500, '读取页面失败'); }
});
server.listen(port, '127.0.0.1', () => console.log(`画风批改台：http://127.0.0.1:${port}\n${initial ? '已载入：' + initial.filename : '打开网页后选择结果 JSON'}\n仅供人工复核，不调用模型，不改写原始结果。`));
server.on('error', error => { console.error(error.message); process.exitCode = 1; });
