import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import { once } from 'node:events';
import { setImmediate as nextTurn } from 'node:timers/promises';
import { ItemStream, readSse } from './streaming.mjs';
import { hash, REQUEST, JobManager, modelStream, callApi } from './core.mjs';
const input = positive_prompt => ({id:hash(positive_prompt),positive_prompt});
const directory = () => {fs.mkdirSync('D:/Agent/Agent_temp/style-stream-tests',{recursive:true});return fs.mkdtempSync('D:/Agent/Agent_temp/style-stream-tests/run-')};
const request = texts => ({format:REQUEST,version:1,export_id:'stream-test',items:texts.map(input)});

test('incremental objects and previews survive every byte boundary, escaped quotes and Unicode', () => {
  const artist='0.8::画师🙂, "A{}", B::,\r\n';
  const doc={items:[{id:'2',status:'none',artist_string:''},{id:'1',status:'ok',artist_string:artist}]};
  const text=JSON.stringify(doc).replace('🙂','\\ud83d\\ude42');
  for(let width=1;width<25;width++){
    const events=[],parser=new ItemStream(e=>events.push(e));
    for(let i=0;i<text.length;i+=width)parser.push(text.slice(i,i+width));
    assert.deepEqual(parser.finish(),doc);
    assert.deepEqual(events.filter(e=>e.type==='item').map(e=>e.item),doc.items);
    for(const e of events.filter(e=>e.type==='preview'&&e.item.id==='1'))assert.ok(artist.startsWith(e.item.artist_string));
  }
});
test('unknown IDs are ignored; duplicate IDs revoke the earlier individual success',()=>{
  const batch=[input('A'),input('B')],events=[],parser=modelStream(batch,e=>events.push(e));
  parser.push('{"items":[{"id":"01","status":"ok","artist_string":"A"},{"id":2,"status":"ok","artist_string":"B"},');
  assert.equal(events.length,1);assert.equal(events[0].result.id,batch[1].id);
  parser.push('{"id":"2","status":"ok","artist_string":"B"}]}');
  assert.equal(events[1].result.status,'error');
});
test('SSE decoder preserves split UTF-8, CRLF, comments and multiline data',async()=>{
  const bytes=Buffer.from(': hi\r\ndata: 画师🙂\r\ndata: second\r\n\r\ndata: [DONE]\n\n'),events=[];
  await readSse((async function*(){for(const byte of bytes)yield Buffer.from([byte])})(),x=>events.push(x));
  assert.deepEqual(events,['画师🙂\nsecond','[DONE]']);
});
test('stream persists a closed item before call finishes, pause and reload resume only the rest',async()=>{
  const dir=directory();let sent;
  const first=new Promise(r=>sent=r);
  const manager=new JobManager(dir,async(batch,options,signal,log,emit)=>{
    emit({type:'preview',id:batch[0].id,text:'A'});
    emit({type:'result',result:{id:batch[0].id,status:'ok',artist_string:'A'}});sent();
    await new Promise(r=>signal.addEventListener('abort',r,{once:true}));
    emit({type:'result',result:{id:batch[1].id,status:'ok',artist_string:'B'}});
    throw Error('aborted');
  });
  const j=manager.create(request(['A','B']));const work=manager.start(j.id);await first;
  assert.equal(manager.snapshot(j.id).lanes[0].done,1);
  assert.equal(new JobManager(dir).get(j.id).results.length,1);
  manager.pause(j.id);await work;assert.equal(manager.get(j.id).results.length,1);
  const seen=[],restored=new JobManager(dir,async batch=>{seen.push(...batch.map(x=>x.positive_prompt));return{items:batch.map(x=>({id:x.id,status:'none',artist_string:''}))}});
  await restored.start(j.id);assert.deepEqual(seen,['B']);assert.equal(restored.get(j.id).results.length,2);
});
test('ten simultaneous streams keep stable lanes and save out-of-order per-item results',async()=>{
  const gates=[];const manager=new JobManager(directory(),async(batch,options,signal,log,emit)=>{
    const results=batch.map(x=>({id:x.id,status:'ok',artist_string:x.positive_prompt}));
    emit({type:'result',result:results[1]});await new Promise(r=>gates.push(r));
    emit({type:'result',result:results[0]});return{items:results};
  });
  const j=manager.create(request(Array.from({length:20},(_,i)=>'artist:'+i)),{concurrency:10,batchSize:2});
  const work=manager.start(j.id);await nextTurn();const snapshot=manager.snapshot(j.id);
  assert.equal(snapshot.job.ok,10);assert.equal(snapshot.lanes.length,10);assert.ok(snapshot.lanes.every(l=>l.done===1&&l.current.number===2));
  gates.reverse().forEach(r=>r());await work;assert.equal(manager.get(j.id).results.length,20);
  assert.ok(manager.snapshot(j.id).lanes.every(l=>l.phase==='completed'));
});
test('stream interruption preserves valid items and retries only unfinished or revoked duplicate IDs',async()=>{
  let call=0;const batches=[];
  const manager=new JobManager(directory(),async(batch,options,signal,log,emit)=>{
    batches.push(batch.map(x=>x.positive_prompt));call++;
    const parser=modelStream(batch,emit);
    if(call===1){parser.push('{"items":[{"id":"1","status":"ok","artist_string":"A"},{"id":"2","status":"ok","artist_string":"B"},{"id":"2","status":"ok","artist_string":"B"},');throw Error('stream disconnected')}
    return{items:batch.map(x=>({id:x.id,status:'ok',artist_string:x.positive_prompt}))};
  });
  const j=manager.create(request(['A','B','C']));await manager.start(j.id);
  assert.deepEqual(batches,[['A','B','C'],['B','C']]);assert.equal(manager.get(j.id).results.length,3);
});
test('API forwards real deltas before final SSE event; truncated transport rejects without losing emitted item',async t=>{
  let release;const gate=new Promise(r=>release=r);t.after(()=>release());
  const server=http.createServer(async(req,res)=>{
    for await(const _ of req){}res.writeHead(200,{'Content-Type':'text/event-stream'});
    res.write('data: '+JSON.stringify({choices:[{delta:{content:'{"items":[{"id":"1","status":"ok","artist_string":"A"},'}}]})+'\n\n');
    await gate;res.end();
  });server.listen(0,'127.0.0.1');await once(server,'listening');t.after(()=>server.close());
  let first;const emitted=new Promise(r=>first=r);const events=[];
  const work=callApi([input('A'),input('B')],{apiKey:'test',model:'mock',baseUrl:`http://127.0.0.1:${server.address().port}`,effort:'high'},new AbortController().signal,()=>{},e=>{events.push(e);if(e.type==='result')first()});
  const failed=assert.rejects(work,/提前中断/);await emitted;
  assert.equal(events.find(e=>e.type==='result').result.status,'ok');release();await failed;
});
