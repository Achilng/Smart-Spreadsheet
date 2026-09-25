import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { hash, REQUEST } from './core.mjs';

const root = path.dirname(fileURLToPath(import.meta.url));
test('browser stores and forgets keys locally, requires a key, and sends it only for invocation', async () => {
  const html = fs.readFileSync(path.join(root, 'index.html'), 'utf8');
  const source = html.match(/<script>([\s\S]*?)<\/script>/)[1]
    .replace('__SERVER_MANAGED__', 'true').replace('__API_PUBLIC_BASE__', '"https://proxy.example/v1"')
    .replace('refresh();setInterval(refresh,1500);', '');
  const nodes = new Map(), storage = new Map(), calls = [];
  const element = id => {
    if (!nodes.has(id)) nodes.set(id, { value: '', textContent: '', style: {}, parentElement: { style: {} }, classList: { toggle() {} }, addEventListener() {}, setAttribute() {}, replaceChildren() {} });
    return nodes.get(id);
  };
  element('provider').value = 'api'; element('runConcurrency').value = '1';
  const context = vm.createContext({ URL, performance: { now: () => 0 }, setTimeout() {}, clearTimeout() {}, setInterval() {},
    localStorage: { getItem: k => storage.get(k) ?? null, setItem: (k, v) => storage.set(k, v), removeItem: k => storage.delete(k) },
    document: { getElementById: element, querySelector: element, addEventListener() {} },
    fetch: async (url, options) => { calls.push({ url, body: JSON.parse(options.body || '{}') }); return { ok: true, json: async () => [] }; }
  });
  vm.runInContext(source, context);
  assert.notEqual(element('apiKey').style.display, 'none');
  await vm.runInContext("loadKey('http://127.0.0.1:8317/v1')", context);
  await assert.rejects(vm.runInContext("selected='job';current={settings:{provider:'api',baseUrl:'http://127.0.0.1:8317/v1'}};render=()=>{};startJob()", context), /API Key/);
  assert.equal(calls.length, 0);
  element('apiKey').value = 'browser-only-test';
  await vm.runInContext('saveKey()', context);
  assert.equal(storage.get('style-api-key:https://proxy.example/v1'), 'browser-only-test');
  assert.equal(calls.length, 0);
  element('apiKey').value = '';
  await vm.runInContext("loadKey('http://127.0.0.1:8317/v1');", context);
  assert.equal(element('apiKey').value, 'browser-only-test');
  await vm.runInContext('startJob()', context);
  assert.equal(calls[0].body.apiKey, 'browser-only-test');
  await element('forgetKey').onclick();
  assert.equal(storage.size, 0);
  assert.equal(calls.some(call => call.url.includes('credentials')), false);
});

test('server never uses environment credentials or persists submitted keys, including after restart', async () => {
  const temp = process.platform === 'win32' ? 'D:/Agent/Agent_temp/style-browser-key-tests' : '/tmp/style-browser-key-tests';
  fs.mkdirSync(temp, { recursive: true });
  const directory = fs.mkdtempSync(path.join(temp, 'run-')), seen = [];
  const upstream = http.createServer(async (req, res) => {
    seen.push(req.headers.authorization);
    for await (const chunk of req) { /* drain request */ }
    res.setHeader('Content-Type', 'application/json');
    res.end(JSON.stringify(req.url.endsWith('/models') ? { data: [{ id: 'test-model' }] } : { choices: [{ message: { content: JSON.stringify({ items: [{ id: '1', status: 'none', artist_string: '' }] }) } }] }));
  }).listen(0, '127.0.0.1');
  await once(upstream, 'listening');
  const reservation = http.createServer().listen(0, '127.0.0.1'); await once(reservation, 'listening');
  const port = reservation.address().port; await new Promise(resolve => reservation.close(resolve));
  const base = `http://127.0.0.1:${port}`;
  let child, token;
  const stop = async () => { if (child && child.exitCode === null) { const ended = once(child, 'exit'); child.kill(); await ended; } };
  const start = async () => {
    child = spawn(process.execPath, [path.join(root, 'server.mjs')], { windowsHide: true, stdio: 'ignore', env: { ...process.env, STYLE_PORT: String(port), STYLE_DATA_DIR: directory, STYLE_PUBLIC_ORIGIN: base, STYLE_SERVER_API_BASE: `http://127.0.0.1:${upstream.address().port}/v1`, STYLE_SERVER_API_KEY: 'obsolete-environment-key', STYLE_API_PUBLIC_BASE: 'https://proxy.example/v1' } });
    let page;
    for (let n = 0; n < 100; n++) { try { page = await (await fetch(base)).text(); break; } catch { await delay(50); } }
    assert.ok(page); assert.equal(page.includes('obsolete-environment-key'), false);
    token = page.match(/const token='([^']+)'/)[1];
  };
  const api = async (route, body) => { const r = await fetch(base + '/api/' + route, { method: body === undefined ? 'GET' : 'POST', headers: { 'Content-Type': 'application/json', 'X-Session-Token': token }, body: body === undefined ? undefined : JSON.stringify(body) }); return { status: r.status, data: await r.json() }; };
  try {
    await start();
    assert.equal((await api('credentials', { action: 'save', apiKey: 'browser-only-test' })).status, 410);
    assert.equal((await api('models', {})).status, 400); assert.equal(seen.length, 0);
    assert.equal((await api('models', { apiKey: 'browser-only-test' })).status, 200);
    const created = await api('jobs', { request: { format: REQUEST, version: 1, export_id: 'browser-credentials', items: [{ id: hash('1girl'), positive_prompt: '1girl' }] }, settings: { model: 'test-model' } });
    const id = created.data.id; assert.ok(id);
    assert.equal((await api(`jobs/${id}/start`, {})).status, 400);
    assert.equal((await api(`jobs/${id}/start`, { apiKey: 'browser-only-test' })).status, 200);
    for (let n = 0; n < 100 && (await api(`jobs/${id}`)).data.state === 'running'; n++) await delay(20);
    assert.equal((await api(`jobs/${id}`)).data.none, 1);
    assert.ok(seen.every(value => value === 'Bearer browser-only-test'));
    await stop();
    for (const name of fs.readdirSync(directory)) assert.equal(fs.readFileSync(path.join(directory, name), 'utf8').includes('browser-only-test'), false);
    fs.writeFileSync(path.join(directory, '.resume.json'), JSON.stringify({ id }));
    const count = seen.length; await start(); await delay(100);
    assert.equal(seen.length, count);
    assert.equal((await api('models', {})).status, 400);
    assert.equal(fs.existsSync(path.join(directory, '.credentials.json')), false);
  } finally { await stop(); await new Promise(resolve => upstream.close(resolve)); }
});
