import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { setTimeout as delay } from 'node:timers/promises';
import { JobManager, REQUEST, hash } from './core.mjs';
import { normalizeChannels } from './channels.mjs';
import { publicAddress, publicFetch } from './public-fetch.mjs';

const directory = () => { const root = process.platform === 'win32' ? 'D:/Agent/Agent_temp/style-channel-tests' : '/tmp/style-channel-tests'; fs.mkdirSync(root, { recursive: true }); return fs.mkdtempSync(root + '/run-'); };
const request = n => ({ format: REQUEST, version: 1, export_id: 'channels', items: Array.from({ length: n }, (_, i) => ({ id: hash('artist:' + i), positive_prompt: 'artist:' + i })) });
const channel = (id, concurrency = 1) => ({ id, name: '渠道 ' + id, baseUrl: 'https://same.example/v1', apiKey: 'secret-' + id, model: 'model-' + id, concurrency, enabled: true });
const reply = items => ({ items: items.map(x => ({ id: x.id, status: 'ok', artist_string: x.positive_prompt })) });
async function until(fn) { for (let i = 0; i < 300; i++) { if (fn()) return; await delay(10); } throw Error('condition timed out'); }

test('independent capacities, same URL different keys and models, out-of-order results', async () => {
  const gates = [], seen = [], counts = new Map();
  const manager = new JobManager(directory(), async (items, options) => {
    counts.set(options.id, (counts.get(options.id) || 0) + 1);
    assert.ok(counts.get(options.id) <= options.concurrency);
    assert.equal(options.apiKey, 'secret-' + options.id); assert.equal(options.model, 'model-' + options.id);
    seen.push(...items.map(x => x.id)); await new Promise(resolve => gates.push(resolve));
    counts.set(options.id, counts.get(options.id) - 1); return reply(items);
  });
  const channels = [channel('A', 2), channel('B', 3)], doc = request(17);
  const job = manager.create(doc, { channels, batchSize: 1 }); const work = manager.start(job.id, false, { channels });
  await until(() => gates.length === 5); assert.equal(manager.summary(manager.get(job.id)).inFlight, 5);
  assert.deepEqual([...counts.values()].sort(), [2, 3]);
  while (manager.active) { gates.splice(0).reverse().forEach(resolve => resolve()); await delay(10); }
  await work; assert.equal(seen.length, 17); assert.equal(new Set(seen).size, 17);
  const result = manager.result(job.id); assert.deepEqual(result.items.map(x => x.id), doc.items.map(x => x.id));
  assert.ok(result.items.every(x => x.model === 'model-' + x.channel_id));
  assert.equal(manager.get(job.id).state, 'completed');
});

test('no fixed concurrency ceiling and huge configured capacity allocates only pending work', async () => {
  const gates = []; const manager = new JobManager(directory(), async items => { await new Promise(resolve => gates.push(resolve)); return reply(items); });
  const channels = [channel('A', 1_000_000)]; const job = manager.create(request(80), { channels, batchSize: 1 });
  const work = manager.start(job.id, false, { channels }); await until(() => gates.length === 80);
  assert.equal(manager.snapshot(job.id).laneTotal, 80); assert.equal(manager.snapshot(job.id).lanes.length, 40);
  assert.equal(manager.snapshot(job.id, 40).lanes[0].slot, 41); gates.forEach(resolve => resolve()); await work;
  assert.equal(manager.get(job.id).results.length, 80);
});

test('bad key disables only its channel; uncompleted items fail over without leaking any keys', async () => {
  const dir = directory(), seen = [];
  const channels = [channel('A'), channel('B')];
  const manager = new JobManager(dir, async (items, options) => { seen.push(options.id); if (options.id === 'A') throw Object.assign(Error('unauthorized secret-A secret-B'), { status: 401 }); return reply(items); });
  const job = manager.create(request(6), { channels, batchSize: 1 }); await manager.start(job.id, false, { channels });
  assert.equal(seen.filter(x => x === 'A').length, 1); assert.equal(manager.get(job.id).state, 'completed');
  assert.equal(manager.get(job.id).results.length, 6); assert.ok(manager.get(job.id).results.every(x => x.channel_id === 'B'));
  for (const data of [...fs.readdirSync(dir).map(name => fs.readFileSync(dir + '/' + name, 'utf8')), JSON.stringify(manager.list()), JSON.stringify(manager.result(job.id))]) {
    assert.ok(!data.includes('secret-A')); assert.ok(!data.includes('secret-B'));
  }
});

test('rate-limited channel recovers after Retry-After while other channels proceed', async () => {
  const channels = [channel('A'), channel('B')]; let calls = 0, progressed = 0;
  const manager = new JobManager(directory(), async (items, options) => {
    if (options.id === 'A' && ++calls === 1) throw Object.assign(Error('rate limit'), { status: 429, retryAfterMs: 1000 });
    if (options.id === 'B') { progressed++; await delay(250); } return reply(items);
  });
  const job = manager.create(request(12), { channels, batchSize: 1 }); const work = manager.start(job.id, false, { channels });
  await until(() => manager.active?.stats()[0].state === 'cooldown');
  assert.equal(manager.get(job.id).state, 'running'); await work;
  assert.ok(calls > 1); assert.ok(progressed > 1); assert.equal(manager.get(job.id).results.length, 12);
});

test('simultaneous 429 responses count as one cooldown round and recover automatically', async () => {
  const gates = []; let calls = 0;
  const channels = [channel('A', 20)];
  const manager = new JobManager(directory(), async items => {
    if (++calls <= 13) { await new Promise(resolve => gates.push(resolve)); throw Object.assign(Error('429 concurrency limit'), { status: 429, retryAfterMs: 1000 }); }
    return reply(items);
  });
  const job = manager.create(request(123), { channels, batchSize: 10 }), work = manager.start(job.id, false, { channels });
  await until(() => gates.length === 13); gates.forEach(resolve => resolve());
  await until(() => manager.active?.inFlight === 0);
  assert.equal(manager.active.stats()[0].state, 'cooldown');
  assert.equal(manager.active.channels[0].rateFailures, 1);
  assert.equal(manager.get(job.id).state, 'running');
  await work; assert.equal(manager.get(job.id).results.length, 123); assert.equal(calls, 26);
});

test('three failed recovery rounds pause; parallel errors never exhaust all rounds at once', async () => {
  let calls = 0; const channels = [channel('A', 6)];
  const manager = new JobManager(directory(), async () => { calls++; await delay(30); throw Object.assign(Error('429 rate limit'), { status: 429, retryAfterMs: 1000 }); });
  const job = manager.create(request(6), { channels, batchSize: 1 }); await manager.start(job.id, false, { channels });
  assert.equal(calls, 18); assert.equal(manager.get(job.id).state, 'paused');
  assert.equal(manager.summary(manager.get(job.id)).pending, 6); assert.equal(manager.active, null);
});

test('all channels unavailable pauses, and restart requires keys and preserves successful items', async () => {
  const dir = directory(), channels = [channel('A')]; let first = true;
  const manager = new JobManager(dir, async items => { if (!first) throw Object.assign(Error('quota'), { status: 403 }); first = false; return reply(items); });
  const job = manager.create(request(4), { channels, batchSize: 1 }); await manager.start(job.id, false, { channels });
  assert.equal(manager.get(job.id).state, 'paused'); assert.equal(manager.get(job.id).results.length, 1);
  const seen = []; const restored = new JobManager(dir, async items => { seen.push(...items.map(x => x.id)); return reply(items); });
  await assert.rejects(restored.start(job.id), /API Key/);
  await restored.start(job.id, false, { channels: [channel('B', 50)] });
  assert.equal(seen.length, 3); assert.equal(restored.get(job.id).results.length, 4);
  assert.deepEqual(new Set(restored.result(job.id).items.map(x => x.model)), new Set(['model-A', 'model-B']));
});

test('live updates add capacity and change models only for new requests; disabled channels drain', async () => {
  const gates = [], seen = []; const manager = new JobManager(directory(), async (items, options) => { seen.push({ ids: items.map(x => x.id), ...options }); await new Promise(resolve => gates.push(resolve)); return reply(items); });
  const channels = [channel('A')]; const job = manager.create(request(9), { channels, batchSize: 1 });
  const work = manager.start(job.id, false, { channels }); await until(() => gates.length === 1);
  manager.active.update([{ ...channel('A', 2), model: 'new-A', apiKey: 'new-secret' }, channel('B', 2)]);
  await until(() => gates.length === 4);
  assert.equal(seen[0].model, 'model-A'); assert.ok(seen.some(x => x.model === 'new-A' && x.apiKey === 'new-secret'));
  manager.active.update([{ ...channel('A'), enabled: false }, channel('B', 1)]);
  gates.splice(0).forEach(resolve => resolve()); await until(() => gates.length === 1);
  assert.equal(seen.at(-1).id, 'B'); assert.equal(manager.active.inFlight, 1);
  while (manager.active) { gates.splice(0).forEach(resolve => resolve()); await delay(10); } await work;
  const ids = seen.flatMap(x => x.ids); assert.equal(ids.length, 9); assert.equal(new Set(ids).size, 9);
});

test('partial stream duplicates are revoked; only unfinished items retry and bounded failures end', async () => {
  let calls = 0; const seen = [], manager = new JobManager(directory(), async (items, options, signal, log, emit) => {
    seen.push(items.map(x => x.id)); calls++;
    if (calls === 1) { for (const result of reply(items).items.slice(0, 2)) emit({ type: 'result', result }); emit({ type: 'result', result: { id: items[1].id, status: 'error', artist_string: '', error: 'duplicate' } }); throw Error('stream interrupted'); }
    return { items: items.map(x => ({ id: x.id, status: 'ok', artist_string: x.positive_prompt.endsWith('2') ? 'rewritten' : x.positive_prompt })) };
  });
  const channels = [channel('A')], doc = request(3), job = manager.create(doc, { channels, batchSize: 3 }); await manager.start(job.id, false, { channels });
  assert.deepEqual(seen, [doc.items.map(x => x.id), doc.items.slice(1).map(x => x.id), [doc.items[2].id]]);
  assert.equal(manager.summary(manager.get(job.id)).ok, 2); assert.equal(manager.summary(manager.get(job.id)).errors, 1);
});

test('pause drains active requests and persists complete streamed results', async () => {
  let entered = 0; const manager = new JobManager(directory(), async (items, options, signal, log, emit) => {
    emit({ type: 'result', result: reply(items).items[0] }); entered++;
    await new Promise(resolve => signal.addEventListener('abort', resolve, { once: true })); throw Error('aborted');
  });
  const channels = [channel('A', 2), channel('B', 2)], job = manager.create(request(20), { channels, batchSize: 2 }); const work = manager.start(job.id, false, { channels });
  await until(() => entered === 4); manager.pause(job.id); await work;
  assert.equal(manager.active, null); assert.equal(manager.get(job.id).state, 'paused'); assert.equal(manager.get(job.id).results.length, 4);
});

test('a late error from an old key cannot disable the replacement configuration', async () => {
  let rejectOld; const channels = [channel('A')];
  const manager = new JobManager(directory(), async (items, options) => {
    if (options.apiKey === 'secret-A') await new Promise((resolve, reject) => { rejectOld = reject; });
    return reply(items);
  });
  const job = manager.create(request(3), { channels, batchSize: 1 }), work = manager.start(job.id, false, { channels });
  await until(() => rejectOld);
  manager.active.update([{ ...channel('A'), apiKey: 'fixed-key' }]);
  rejectOld(Object.assign(Error('unauthorized'), { status: 401 })); await work;
  assert.equal(manager.get(job.id).state, 'completed'); assert.equal(manager.get(job.id).results.length, 3);
});

test('validation rejects duplicate ids, invalid limits and model names before mutation', () => {
  for (const values of [[channel('A'), channel('A')], [{ ...channel('A'), concurrency: 0 }], [{ ...channel('A'), model: 'bad name' }], [{ ...channel('A'), baseUrl: 'https://user:secret@example.com' }]]) assert.throws(() => normalizeChannels(values));
  assert.equal(normalizeChannels([channel('A', 10_000_000)])[0].concurrency, 10_000_000);
});

test('disabling every channel drains requests, preserves zero capacity on restart, and can resume', async () => {
  let release; const dir = directory(), channels = [channel('A')];
  const manager = new JobManager(dir, async items => { await new Promise(resolve => { release = resolve; }); return reply(items); });
  const job = manager.create(request(3), { channels, batchSize: 1 }); const work = manager.start(job.id, false, { channels });
  await until(() => release); manager.active.update([{ ...channel('A'), enabled: false }]); release(); await work;
  assert.equal(manager.get(job.id).state, 'paused'); assert.equal(manager.get(job.id).results.length, 1);
  const restored = new JobManager(dir, async items => reply(items)); assert.equal(restored.get(job.id).settings.concurrency, 0);
  await restored.start(job.id, false, { channels }); assert.equal(restored.get(job.id).results.length, 3);
});

test('public custom endpoints cannot access private IPs or redirect to them', async () => {
  for (const ip of ['127.0.0.1','10.0.0.2','172.16.0.1','192.168.1.2','169.254.169.254','100.64.0.1','::1','::ffff:127.0.0.1','fd00::1','2002:7f00:1::']) assert.equal(publicAddress(ip), false, ip);
  assert.equal(publicAddress('8.8.8.8'), true); assert.equal(publicAddress('2606:4700:4700::1111'), true);
  await assert.rejects(publicFetch('https://127.0.0.1/v1'), /公网/);
  await assert.rejects(publicFetch('https://localhost/v1'), /内网/);
});
