import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { JobManager, REQUEST, hash } from './core.mjs';
import { appendResult, replayResults } from './result-journal.mjs';

const directory = () => {
  const root = process.platform === 'win32' ? 'D:/Agent/Agent_temp/style-journal-tests' : '/tmp/style-journal-tests';
  fs.mkdirSync(root, { recursive: true }); return fs.mkdtempSync(root + '/run-');
};
const doc = count => ({ format: REQUEST, version: 1, export_id: 'journal-test', items: Array.from({ length: count }, (_, i) => ({ id: hash('artist:' + i), positive_prompt: 'artist:' + i })) });
const channel = { id: 'codex', name: 'codex', baseUrl: 'http://127.0.0.1:1/v1', model: 'test', apiKey: 'never-persist-me', concurrency: 1 };
const result = item => ({ id: item.id, status: 'ok', artist_string: item.positive_prompt });

test('journal recovery applies replacement and deletion, ignores torn tail, and checkpoints idempotently', () => {
  const dir = directory(), manager = new JobManager(dir), job = manager.create(doc(3), { channels: [channel] });
  const [a,b,c] = manager.get(job.id).request.items, file = manager.journalPath(job.id);
  appendResult(file, a.id, result(a)); appendResult(file, b.id, result(b)); appendResult(file, a.id, null);
  appendResult(file, b.id, { ...result(b), status: 'none', artist_string: '' });
  const complete = fs.readFileSync(file); fs.appendFileSync(file, '{"id":"unfinished');
  const restored = new JobManager(dir);
  assert.deepEqual(restored.get(job.id).results, [{ ...result(b), status: 'none', artist_string: '' }]);
  assert.equal(fs.statSync(file).size, 0);
  // Simulate a crash after snapshot replacement but before journal truncation.
  fs.writeFileSync(file, complete); const twice = new JobManager(dir);
  assert.deepEqual(twice.get(job.id).results, restored.get(job.id).results);
  appendResult(file, c.id, result(c));
  assert.equal(new JobManager(dir).get(job.id).results.length, 2);
  fs.writeFileSync(file, '{"broken":true}\n'); assert.throws(() => replayResults(file, []), /损坏/);
});

test('large resumed task journals only new results, keeps checkpoint stable during stream, and uses batches of 20', async () => {
  const dir = directory(), input = doc(10040), seen = [], batches = [];
  let release, began; const gate = new Promise(r => release = r), ready = new Promise(r => began = r);
  const manager = new JobManager(dir, async (items, options, signal, log, emit) => {
    seen.push(...items.map(x => x.id)); batches.push(items.length);
    const before = fs.readFileSync(path.join(dir, job.id + '.json'));
    for (const item of items) emit({ type: 'result', result: result(item) });
    assert.deepEqual(fs.readFileSync(path.join(dir, job.id + '.json')), before);
    if (batches.length === 1) { began(); await gate; }
    return { items: items.map(result) };
  });
  const job = manager.create(input, { channels: [channel], batchSize: 10 });
  manager.get(job.id).results = input.items.slice(0, 10000).map(x => ({ ...result(x), model: 'old-model', processed_at: 'original' })); manager.save(manager.get(job.id));
  let writes = 0; const record = manager.recordResult.bind(manager); manager.recordResult = (...args) => { writes++; record(...args); };
  const work = manager.start(job.id, false, { channels: [channel], batchSize: 20 }); await ready;
  try {
    assert.equal(writes, 20); const journal = fs.readFileSync(manager.journalPath(job.id), 'utf8');
    assert.ok(journal.length < 20000); assert.equal(journal.includes(channel.apiKey), false);
    const crashDir = directory();
    for (const suffix of ['.json', '.request.json', '.results.jsonl']) fs.copyFileSync(path.join(dir, job.id + suffix), path.join(crashDir, job.id + suffix));
    const crash = new JobManager(crashDir); assert.equal(crash.get(job.id).results.length, 10020);
    assert.equal(crash.get(job.id).state, 'paused'); assert.equal(crash.get(job.id).results[0].processed_at, 'original');
  } finally { release(); await work; }
  assert.deepEqual(batches, [20,20]); assert.deepEqual(seen, input.items.slice(10000).map(x => x.id));
  assert.equal(writes, 40); // Streaming and final validation must not write each result twice.
  const restored = new JobManager(dir); assert.equal(restored.get(job.id).results.length, 10040);
  assert.equal(fs.statSync(manager.journalPath(job.id)).size, 0);
  assert.deepEqual(restored.get(job.id).results.slice(0,10000), manager.get(job.id).results.slice(0,10000));
  restored.discard(job.id); assert.equal(fs.existsSync(manager.journalPath(job.id)), false);
});

test('failed checkpoint keeps the durable journal available for recovery', () => {
  const dir = directory(), manager = new JobManager(dir), created = manager.create(doc(1), { channels: [channel] }), job = manager.get(created.id);
  const value = result(job.request.items[0]); manager.recordResult(job, value.id, value); job.results.push(value);
  const rename = fs.renameSync;
  try { fs.renameSync = () => { throw Error('checkpoint failed'); }; assert.throws(() => manager.save(job), /checkpoint failed/); }
  finally { fs.renameSync = rename; }
  assert.ok(fs.statSync(manager.journalPath(job.id)).size > 0);
  assert.deepEqual(new JobManager(dir).get(job.id).results, [value]);
});

test('journal storage failure pauses instead of acknowledging or retrying unsaved results', async () => {
  let calls = 0;
  const manager = new JobManager(directory(), async (items, options, signal, log, emit) => {
    calls++; emit({ type: 'result', result: result(items[0]) }); return { items: items.map(result) };
  });
  const job = manager.create(doc(2), { channels: [channel], batchSize: 1 });
  manager.recordResult = () => { throw Object.assign(Error('Disk full'), { storageFailure: true }); };
  await manager.start(job.id, false, { channels: [channel] });
  assert.equal(calls, 1); assert.equal(manager.get(job.id).state, 'paused'); assert.equal(manager.get(job.id).results.length, 0);
  assert.equal(manager.active, null); assert.match(manager.get(job.id).message, /Disk full/);
});
