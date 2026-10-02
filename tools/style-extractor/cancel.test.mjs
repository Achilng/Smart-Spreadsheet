import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { JobManager, REQUEST, hash } from './core.mjs';
import { ResumeController } from './resume.mjs';

const directory = () => {
  const root = process.platform === 'win32' ? 'D:/Agent/Agent_temp/style-cancel-tests' : '/tmp/style-cancel-tests';
  fs.mkdirSync(root, { recursive: true }); return fs.mkdtempSync(root + '/run-');
};
const request = { format: REQUEST, version: 1, export_id: 'cancel', items: ['a','b'].map(positive_prompt => ({ id: hash(positive_prompt), positive_prompt })) };
const channel = { id: 'test', name: 'test', baseUrl: 'http://127.0.0.1:1/v1', model: 'test', concurrency: 1, apiKey: 'test-key' };

test('cancel waits for active requests, discards partial results and cannot resurrect after restart', async () => {
  for (const multi of [false, true]) {
    let began, release;
    const ready = new Promise(r => began = r), held = new Promise(r => release = r);
    const dir = directory(), manager = new JobManager(dir, async (batch, options, signal, log, emit) => {
      emit({ type: 'result', result: { id: batch[0].id, status: 'none', artist_string: '' } }); began();
      await held;
      emit({ type: 'result', result: { id: batch[1].id, status: 'none', artist_string: '' } });
      return { items: batch.map(x => ({ id: x.id, status: 'none', artist_string: '' })) };
    });
    const controller = new ResumeController(manager, true);
    const job = manager.create(request, multi ? { channels: [channel] } : { provider: 'api', baseUrl: channel.baseUrl });
    const keep = manager.create({ ...request, export_id: 'keep' }, { channels: [channel] });
    const keepBytes = fs.readFileSync(dir + '/' + keep.id + '.json');
    controller.start(job.id, false, multi ? { channels: [channel] } : { apiKey: 'test-key' }); await ready;
    assert.equal(manager.get(job.id).results.length, 1);
    let ended = false; const cancelling = controller.cancel(job.id).then(() => ended = true);
    assert.throws(() => controller.start(job.id, false, {}), /取消/);
    await Promise.resolve(); assert.equal(ended, false); assert.equal(manager.active.controller.signal.aborted, true);
    const duplicate = controller.cancel(job.id); release(); await Promise.all([cancelling, duplicate]);
    assert.equal(manager.active, null); assert.equal(manager.jobs.has(job.id), false); assert.equal(manager.live.has(job.id), false);
    assert.equal(fs.existsSync(dir + '/' + job.id + '.json'), false); assert.equal(fs.existsSync(dir + '/' + job.id + '.request.json'), false);
    assert.deepEqual(fs.readFileSync(dir + '/' + keep.id + '.json'), keepBytes);
    const reloaded = new JobManager(dir); assert.deepEqual(reloaded.list().map(x => x.id), [keep.id]);
    assert.equal(JSON.parse(fs.readFileSync(controller.file, 'utf8')).id, null);
    await controller.cancel(keep.id); assert.equal(manager.list().length, 0);
  }
});

test('discarding a different saved job does not interrupt the running job', async () => {
  let release; const gate = new Promise(r => release = r);
  const manager = new JobManager(directory(), async batch => { await gate; return { items: batch.map(x => ({ id:x.id,status:'none',artist_string:'' })) }; });
  const controller = new ResumeController(manager, false), a = manager.create(request, { channels:[channel] }), b = manager.create(request, { channels:[channel] });
  const run = controller.start(a.id, false, { channels:[channel] });
  await controller.cancel(b.id); assert.equal(manager.active.id, a.id); assert.equal(manager.active.controller.signal.aborted, false);
  release(); await run; assert.equal(manager.summary(manager.get(a.id)).none, 2);
  await controller.cancel(a.id); assert.equal(new JobManager(manager.directory).list().length, 0);
});
