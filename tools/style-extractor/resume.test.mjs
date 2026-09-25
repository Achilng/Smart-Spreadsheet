import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { JobManager, REQUEST, hash } from './core.mjs';
import { ResumeController } from './resume.mjs';
const directory=()=>{const root=process.platform==='win32'?'D:/Agent/Agent_temp/style-resume-tests':'/tmp/style-resume-tests';fs.mkdirSync(root,{recursive:true});return fs.mkdtempSync(root+'/run-')};
test('service shutdown restores only unfinished items; user pause prevents automatic restart',async()=>{
  for(const manual of [false,true]){
    const dir=directory();let ready;const first=new Promise(r=>ready=r);
    const request={format:REQUEST,version:1,export_id:'resume-test',items:['A','B'].map(positive_prompt=>({id:hash(positive_prompt),positive_prompt}))};
    const manager=new JobManager(dir,async(batch,options,signal,log,emit)=>{
      emit({type:'result',result:{id:batch[0].id,status:'none',artist_string:''}});ready();
      await new Promise(r=>signal.addEventListener('abort',r,{once:true}));throw Error('stopped');
    });
    const job=manager.create(request,{provider:'api',baseUrl:'http://127.0.0.1:12345/v1',model:'test'});
    const controller=new ResumeController(manager,true);controller.start(job.id,false,{apiKey:'test'});await first;
    if(manual)controller.pause(job.id);
    await controller.stop();
    const seen=[];
    const reloaded=new JobManager(dir,async batch=>{seen.push(...batch.map(x=>x.positive_prompt));return{items:batch.map(x=>({id:x.id,status:'none',artist_string:''}))}});
    const restored=new ResumeController(reloaded,true);await restored.restore({apiKey:'test'});
    assert.deepEqual(seen,manual?[]:['B']);
    if(!manual){assert.equal(reloaded.summary(reloaded.get(job.id)).state,'completed');assert.equal(JSON.parse(fs.readFileSync(restored.file)).id,null)}
  }
});
test('failed start clears persistent auto-resume intent',async()=>{
 const manager=new JobManager(directory());const job=manager.create({format:REQUEST,version:1,export_id:'bad-key',items:[{id:hash('A'),positive_prompt:'A'}]},{provider:'api',baseUrl:'http://127.0.0.1:12345/v1'});
 const controller=new ResumeController(manager,true);await controller.start(job.id,false,{});
 assert.equal(JSON.parse(fs.readFileSync(controller.file)).id,null);
});
