import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import { once } from 'node:events';
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { REQUEST, hash } from './core.mjs';
import { readManagedConfig } from './managed-config.mjs';

test('simple server uses operator credentials, ignores browser configuration and cancels through the API', async () => {
  const root = process.platform === 'win32' ? 'D:/Agent/Agent_temp/style-managed-tests' : '/tmp/style-managed-tests';
  fs.mkdirSync(root, { recursive:true }); const dir = fs.mkdtempSync(root + '/run-'), configFile = path.join(dir, 'operator.json');
  const seen = []; let hold = false;
  const upstream = http.createServer(async (req,res) => {
    let body=''; for await (const chunk of req) body+=chunk;
    seen.push({key:req.headers.authorization, ...JSON.parse(body)});
    if (hold) { res.on('close', () => {}); return; }
    const items=JSON.parse(seen.at(-1).messages[1].content).items;
    res.setHeader('Content-Type','application/json');res.end(JSON.stringify({choices:[{message:{content:JSON.stringify({items:items.map(x=>({id:x.id,status:'none',artist_string:''}))})}}]}));
  }).listen(0,'127.0.0.1'); await once(upstream,'listening');
  const reservation=http.createServer().listen(0,'127.0.0.1');await once(reservation,'listening');const port=reservation.address().port;await new Promise(r=>reservation.close(r));
  const base=`http://127.0.0.1:${port}`, channel={id:'codex',name:'codex',baseUrl:'https://configured.example/v1',model:'operator-model',concurrency:2,apiKey:'operator-secret'};
  let batchSize=1;
  const writeConfig=()=>fs.writeFileSync(configFile,JSON.stringify({channels:[channel],batchSize,effort:'high'}));writeConfig();
  let child,token;
  const start=async()=>{
    child=spawn(process.execPath,[path.join(path.dirname(fileURLToPath(import.meta.url)),'server.mjs')],{windowsHide:true,stdio:'ignore',env:{...process.env,STYLE_PORT:String(port),STYLE_PUBLIC_ORIGIN:base,STYLE_CHANNEL_CONFIG:configFile,STYLE_DATA_DIR:path.join(dir,'data'),STYLE_API_PUBLIC_BASE:channel.baseUrl,STYLE_SERVER_API_BASE:`http://127.0.0.1:${upstream.address().port}/v1`}});
    for(let i=0;i<100;i++){try{const page=await(await fetch(base)).text();token=page.match(/const token='([^']+)'/)[1];return page;}catch{await delay(30);}}throw Error('Server did not start');
  };
  const stop=async()=>{if(child?.exitCode===null){const exited=once(child,'exit');child.kill();await exited;}};
  const api=async(route,body)=>{const r=await fetch(base+'/api/'+route,{method:body===undefined?'GET':'POST',headers:{'X-Session-Token':token,'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body)});return {status:r.status,data:await r.json()};};
  const create=async()=> (await api('jobs',{request:{format:REQUEST,version:1,export_id:'simple',items:[{id:hash('hello'),positive_prompt:'hello'}]},settings:{channels:[{...channel,model:'browser-model',apiKey:'browser-secret',baseUrl:'https://untrusted.example/v1'}],batchSize:99}})).data;
  try {
    const page=await start(); assert.match(page,/const simpleMode=true/);assert.equal(page.includes('operator-secret'),false);assert.equal(page.includes('src="\/channel-store.js"'),false);
    assert.equal((await fetch(base+'/channels')).status,404);assert.equal((await api('models',{apiKey:'browser-secret'})).status,403);
    const job=await create();assert.equal(job.settings.channels[0].model,'operator-model');assert.equal(job.settings.batchSize,1);assert.equal(JSON.stringify(job).includes('operator-secret'),false);
    assert.equal((await api(`jobs/${job.id}/channels`,{channels:[]})).status,403);
    assert.equal((await api(`jobs/${job.id}/start`,{channels:[{...channel,apiKey:'browser-secret'}]})).status,200);
    for(let n=0;n<100&&(await api(`jobs/${job.id}`)).data.state==='running';n++)await delay(20);
    assert.equal(seen[0].key,'Bearer operator-secret');assert.equal(seen[0].model,'operator-model');assert.equal((await api(`jobs/${job.id}`)).data.none,1);
    for(const name of fs.readdirSync(path.join(dir,'data')))assert.equal(fs.readFileSync(path.join(dir,'data',name),'utf8').includes('operator-secret'),false);
    hold=true;channel.apiKey='changed-secret';writeConfig();const busy=await create();
    batchSize=20;writeConfig();const resumed=await api(`jobs/${busy.id}/start`,{batchSize:99});
    assert.equal(resumed.data.id,busy.id);assert.equal(resumed.data.settings.batchSize,20);
    for(let n=0;n<100&&seen.length<2;n++)await delay(20);assert.equal(seen.at(-1).key,'Bearer changed-secret');
    const events=await fetch(base+`/api/jobs/${busy.id}/events`,{headers:{'X-Session-Token':token}}), reader=events.body.getReader();await reader.read();
    assert.equal((await api(`jobs/${busy.id}/cancel`,{})).data.cancelled,true);
    while(!(await reader.read()).done){}
    assert.equal((await fetch(base+'/health')).status,200);assert.equal((await api('jobs')).data.length,1);
    assert.equal((await api(`jobs/${busy.id}/start`,{})).status,400);
    await stop();await start();assert.deepEqual((await api('jobs')).data.map(x=>x.id),[job.id]);
    fs.writeFileSync(configFile,'{"apiKey":"do-not-leak"');assert.throws(()=>readManagedConfig(configFile),/^Error: 服务配置暂不可用/);
    const unavailable=await create();assert.equal(unavailable.error,'服务配置暂不可用，请联系管理员。');
    await api(`jobs/${job.id}/cancel`,{});assert.equal((await api('jobs')).data.length,0);
  } finally { await stop();upstream.closeAllConnections();await new Promise(r=>upstream.close(r)); }
});
