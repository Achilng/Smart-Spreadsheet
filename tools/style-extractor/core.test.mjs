import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import { once } from 'node:events';
import { hash, validateRequest, validateReply, JobManager, REQUEST, callApi, normalizeBaseUrl } from './core.mjs';
const tempRoot = 'D:/Agent/Agent_temp/style-extractor-tests';
fs.mkdirSync(tempRoot, { recursive: true });
const item = positive_prompt => ({ id: hash(positive_prompt), positive_prompt });
const request = texts => ({ format: REQUEST, version: 1, export_id: 'test', items: texts.map(item) });
const directory = () => fs.mkdtempSync(path.join(tempRoot, 'run-'));
test('exact text identity includes whitespace, case, newlines and Unicode', () => {
  assert.notEqual(hash('A'), hash('A ')); assert.notEqual(hash('A\r\n'), hash('A\n'));
  assert.throws(() => validateRequest(request(['A', 'A'])), /重复/);
  assert.equal(validateRequest(request(['画师🙂\\"\n', 'A '])).items.length, 2);
});
test('reply rejects rewrites, missing IDs, duplicate IDs, and unknown IDs', () => {
  const batch = [item('0.8::A, B::, girl'), item('girl')];
  const result = validateReply(batch, { items: [{ id: batch[0].id, status: 'ok', artist_string: '0.8::A, B::, ' }, { id: batch[1].id, status: 'none', artist_string: '' }] });
  assert.deepEqual(result.map(x => x.status), ['ok', 'none']);
  assert.equal(validateReply(batch, { items: [{ id: batch[0].id, status: 'ok', artist_string: '0.8::a, B::' }] })[0].status, 'error');
  assert.throws(() => validateReply(batch, { items: [{ id: 'invented' }] }), /未知/);
  assert.equal(validateReply(batch, { items: [result[0], result[0]] })[0].status, 'error');
});
test('batch size, save, restart, and completed items are not repeated', async () => {
  const calls = []; const runner = async batch => { calls.push(batch.length); return { items: batch.map(x => ({ id: x.id, status: 'ok', artist_string: x.positive_prompt })) }; };
  const dir = directory(), manager = new JobManager(dir, runner);
  const job = manager.create(request(['A', 'B', 'C', 'D', 'E']), { batchSize: 2 });
  await manager.start(job.id); assert.deepEqual(calls, [2, 2, 1]);
  const restored = new JobManager(dir, runner); await restored.start(job.id);
  assert.deepEqual(calls, [2, 2, 1]); assert.equal(restored.result(job.id).items.length, 5);
  assert.equal(restored.summary(restored.get(job.id)).ok, 5);
});
test('pause interrupts a call, reload resumes pending items only', async () => {
  const dir = directory(); let began;
  const started = new Promise(resolve => began = resolve);
  const manager = new JobManager(dir, async (batch, settings, signal) => { began(); await new Promise(resolve => signal.addEventListener('abort', resolve, { once: true })); throw Error('cancelled'); });
  const job = manager.create(request(['A', 'B']), { batchSize: 1 }); const work = manager.start(job.id); await started; manager.pause(job.id); await work;
  assert.equal(manager.get(job.id).state, 'paused'); assert.equal(manager.get(job.id).results.length, 0);
  const restored = new JobManager(dir, async batch => ({ items: batch.map(x => ({ id: x.id, status: 'none', artist_string: '' })) }));
  await restored.start(job.id); assert.equal(restored.summary(restored.get(job.id)).none, 2);
});
test('authentication error pauses without classifying unprocessed items as none', async () => {
  const manager = new JobManager(directory(), async () => { throw Error('401 unauthorized'); }); const job = manager.create(request(['A']));
  await manager.start(job.id); assert.equal(manager.get(job.id).state, 'paused'); assert.equal(manager.get(job.id).results.length, 0);
  assert.equal(manager.result(job.id).items[0].status, 'error');
});

test('compatible API falls back for unsupported options and preserves exact Unicode inputs', async t => {
  const batch = [item('0.5::画师🙂, Artist::, \r\n1girl')], bodies = [];
  const server = http.createServer(async (req, res) => {
    const chunks = []; for await (const chunk of req) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks)); bodies.push(body);
    assert.equal(req.url, '/v1/chat/completions'); assert.equal(req.headers.authorization, 'Bearer test-only');
    res.setHeader('Content-Type', 'application/json');
    if (body.response_format || body.reasoning_effort) {
      res.statusCode = 400; res.end(JSON.stringify({ error: { message: (body.response_format ? 'response_format' : 'reasoning_effort') + ' unsupported' } }));
    } else res.end(JSON.stringify({ choices: [{ message: { content: JSON.stringify({ items: [{ id: batch[0].id, status: 'ok', artist_string: '0.5::画师🙂, Artist::, \r\n' }] }) }, finish_reason: 'stop' }] }));
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening'); t.after(() => server.close());
  const reply = await callApi(batch, { baseUrl: `http://127.0.0.1:${server.address().port}/v1/`, model: 'gpt-6-luna【神秘】', apiKey: 'test-only', effort: 'high' }, new AbortController().signal, () => {});
  assert.equal(bodies.length, 3); assert.equal(bodies[0].model, 'gpt-6-luna【神秘】');
  assert.deepEqual(JSON.parse(bodies[0].messages[1].content).items, batch);
  assert.equal(validateReply(batch, reply)[0].status, 'ok');
});

test('API credentials remain transient across save, restart and error logging', async t => {
  const key = 'test-secret-not-persisted', dir = directory();
  const server = http.createServer((req, res) => { res.writeHead(401, { 'Content-Type': 'application/json' }); res.end(JSON.stringify({ error: { message: 'Invalid key ' + key } })); });
  server.listen(0, '127.0.0.1'); await once(server, 'listening'); t.after(() => server.close());
  const manager = new JobManager(dir), job = manager.create(request(['A']), { provider: 'api', model: 'gpt-6-luna【神秘】', baseUrl: `http://127.0.0.1:${server.address().port}/v1`, apiKey: key });
  await manager.start(job.id, false, { apiKey: key });
  assert.equal(manager.get(job.id).state, 'paused'); assert.equal(manager.get(job.id).results.length, 0);
  for (const name of fs.readdirSync(dir)) assert.equal(fs.readFileSync(path.join(dir, name), 'utf8').includes(key), false);
  assert.equal(JSON.stringify(manager.list()).includes(key), false);
  assert.equal(JSON.stringify(manager.result(job.id)).includes(key), false);
  const restored = new JobManager(dir); await assert.rejects(restored.start(job.id), /API Key/);
  assert.throws(() => normalizeBaseUrl('https://user:password@example.com/v1'));
  assert.throws(() => normalizeBaseUrl('https://example.com/v1?key=secret'));
});
