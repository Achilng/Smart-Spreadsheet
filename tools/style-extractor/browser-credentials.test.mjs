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
test('browser migrates legacy keys once and keeps same-address channel keys separate', () => {
  const storage = new Map([['style-api-key:https://proxy.example/v1', 'old-key']]);
  const context = vm.createContext({ URL, crypto: { randomUUID: () => 'default-id' }, publicApiBase: 'https://proxy.example/v1', window: {},
    localStorage: { get length() { return storage.size; }, key: index => [...storage.keys()][index], getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value), removeItem: key => storage.delete(key) }
  });
  vm.runInContext(fs.readFileSync(path.join(root, 'channel-store.js'), 'utf8'), context);
  const store = context.window.channelStore, values = store.load();
  assert.equal(values[0].apiKey, 'old-key');
  values.push({ ...values[0], id: 'second', apiKey: 'second-key', model: 'different-model', concurrency: 500 }); store.save(values);
  assert.equal(store.load()[0].apiKey, 'old-key'); assert.equal(store.load()[1].apiKey, 'second-key');
  assert.equal(JSON.stringify(store.metadata(values)).includes('old-key'), false);
  store.save([]); assert.equal(store.load().length, 0); // Deleted channels must not be resurrected by legacy keys.
  storage.set('style-channels-v1', 'broken'); assert.throws(() => store.load());
});

test('server never uses environment credentials or persists submitted keys, including after restart', async () => {
  const temp = process.platform === 'win32' ? 'D:/Agent/Agent_temp/style-browser-key-tests' : '/tmp/style-browser-key-tests';
  fs.mkdirSync(temp, { recursive: true });
  const directory = fs.mkdtempSync(path.join(temp, 'run-')), seen = [];
  let holdCalls = null;
  const upstream = http.createServer(async (req, res) => {
    seen.push(req.headers.authorization);
    for await (const chunk of req) { /* drain request */ }
    if (!req.url.endsWith('/models') && holdCalls) await holdCalls;
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
    const channels = ['A', 'B'].map(id => ({ id, name: 'channel-' + id, baseUrl: 'https://proxy.example/v1', model: 'model-' + id, concurrency: 40, apiKey: 'channel-secret-' + id }));
    const multi = await api('jobs', { request: { format: REQUEST, version: 1, export_id: 'multi-server', items: ['one','two'].map(positive_prompt => ({ id: hash(positive_prompt), positive_prompt })) }, settings: { channels, batchSize: 1 } });
    assert.equal(multi.status, 200); assert.equal(JSON.stringify(multi.data).includes('channel-secret'), false);
    let release; holdCalls = new Promise(resolve => { release = resolve; });
    assert.equal((await api(`jobs/${multi.data.id}/start`, { channels })).status, 200);
    const blocked = await api(`jobs/${id}/start`, { apiKey: 'browser-only-test' });
    assert.equal(blocked.status, 409); assert.equal(blocked.data.activeJobId, multi.data.id);
    release(); holdCalls = null;
    for (let n = 0; n < 100 && (await api(`jobs/${multi.data.id}`)).data.state === 'running'; n++) await delay(20);
    const result = (await api(`jobs/${multi.data.id}/result`)).data;
    assert.equal(result.items.length, 2); assert.deepEqual(new Set(result.items.map(x => x.model)), new Set(['model-A', 'model-B']));
    assert.ok(seen.includes('Bearer channel-secret-A')); assert.ok(seen.includes('Bearer channel-secret-B'));
    assert.equal((await api('models', { baseUrl: 'http://127.0.0.1:1/v1', apiKey: 'browser-only-test' })).status, 400);
    assert.equal((await fetch(base + '/channels')).status, 200);
    for (const name of ['channels.js','channel-store.js','workspace.js','app.css']) assert.equal((await fetch(base + '/' + name)).status, 200);

    await stop();
    for (const name of fs.readdirSync(directory)) { const text = fs.readFileSync(path.join(directory, name), 'utf8'); assert.equal(text.includes('browser-only-test'), false); assert.equal(text.includes('channel-secret-'), false); }
    fs.writeFileSync(path.join(directory, '.resume.json'), JSON.stringify({ id }));
    const count = seen.length; await start(); await delay(100);
    assert.equal(seen.length, count);
    assert.equal((await api('models', {})).status, 400);
    assert.equal(fs.existsSync(path.join(directory, '.credentials.json')), false);
  } finally { await stop(); await new Promise(resolve => upstream.close(resolve)); }
});
