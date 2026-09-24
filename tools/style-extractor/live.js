(() => {
  const host = document.createElement('section'); host.className = 'live-workspace'; host.setAttribute('aria-label', '并行工作区');
  host.innerHTML = `<div class="live-heading"><strong>并行工作区</strong><span id="live-connection">等待任务</span></div><div class="live-lanes" id="live-lanes"></div><div class="live-note" id="live-note">开始后显示实时通道，点选通道查看原文。</div><section class="live-inspector" id="live-inspector" hidden><div class="live-inspector-head"><span id="live-title"></span><select id="live-item" aria-label="查看本批条目"><option value="">跟随当前输出</option></select><button id="live-close">收起详情</button></div><div class="live-inspector-body"><div><small>正向提示词原文</small><div class="live-text" id="live-source"></div></div><div><small id="live-status">提取片段</small><div class="live-text" id="live-result"></div></div></div></section>`;
  $('message').before(host);
  const lanes = new Map(), phases = {waiting:'等待输出',streaming:'正在输出',receiving:'已校验 · 继续接收',retry:'等待重试',completed:'本批完成',error:'本批有失败',paused:'已暂停'};
  let jobId = '', connection, slot = null, snapshot, detailKey = '', detailSerial = 0, detailBusy = false;
  function clear() { lanes.clear(); $('live-lanes').replaceChildren(); slot = null; snapshot = null; detailKey = ''; ++detailSerial; $('live-inspector').hidden = true; }
  function selectLane(number) { slot = slot === number ? null : number; $('live-item').value = ''; detailKey = ''; paint(snapshot); }
  function paint(data) {
    if (!data) return; snapshot = data;
    const active = new Set(data.lanes.map(l => l.slot));
    for (const [key, ui] of lanes) if (!active.has(key)) { ui.button.remove(); lanes.delete(key); }
    for (const lane of data.lanes) {
      let ui = lanes.get(lane.slot);
      if (!ui) {
        const button = document.createElement('button'); button.className = 'live-lane'; button.type = 'button';
        button.innerHTML = '<span class="live-lane-head"><span class="live-slot"></span><span class="live-label"></span><span class="live-count"></span></span><span class="live-preview"></span><span class="live-segments" aria-hidden="true"></span>';
        button.onclick = () => selectLane(lane.slot); $('live-lanes').append(button);
        ui = {button, label:button.querySelector('.live-label'), preview:button.querySelector('.live-preview'), count:button.querySelector('.live-count'), segments:button.querySelector('.live-segments')};
        button.querySelector('.live-slot').textContent = String(lane.slot).padStart(2,'0'); lanes.set(lane.slot,ui);
      }
      const doneChanged = ui.done !== undefined && lane.done > ui.done && ui.call === lane.call;
      ui.done = lane.done; ui.call = lane.call;
      ui.button.dataset.state = lane.phase; ui.button.setAttribute('aria-pressed',String(slot === lane.slot));
      ui.label.textContent = lane.current ? `第 ${lane.current.number} 条 · ${phases[lane.phase]}` : phases[lane.phase];
      ui.count.textContent = `${lane.done}/${lane.total}`;
      ui.preview.textContent = lane.current?.text || (lane.current?.state === 'none' ? '未发现画风提示词' : lane.mode === 'buffered' ? '接口不支持流式 · 等待整批返回' : lane.phase === 'retry' ? '保留成功项，仅重试未完成内容' : '等待模型返回内容…');
      if (ui.segments.children.length !== lane.total) ui.segments.replaceChildren(...Array.from({length:lane.total},()=>{const s=document.createElement('span');s.className='live-segment';return s}));
      lane.states.forEach((state,i)=>{ui.segments.children[i].dataset.state=state});
      ui.button.setAttribute('aria-label',`通道 ${lane.slot}，${phases[lane.phase]}，已完成 ${lane.done}/${lane.total}`);
      if (doneChanged) { ui.preview.classList.remove('live-flash'); void ui.preview.offsetWidth; ui.preview.classList.add('live-flash'); }
    }
    $('live-note').textContent = data.lanes.length ? '每条校验通过即保存 · 点选通道查看原文与提取片段' : data.job.state === 'running' ? '正在建立模型连接…' : '开始或继续后显示实时通道，已保存结果可下载查看。';
    const lane = data.lanes.find(l=>l.slot===slot); $('live-inspector').hidden = !lane;
    if (lane) {
      const pick = $('live-item');
      if (pick.dataset.batch !== `${slot}:${lane.call}`) {
        const previous = pick.value; pick.replaceChildren(new Option('跟随当前输出',''),...lane.states.map((_,i)=>new Option(`第 ${i+1} 条`,String(i))));
        pick.value = lane.attempt && Number(previous)<lane.total ? previous : ''; pick.dataset.batch = `${slot}:${lane.call}`;
      }
      $('live-title').textContent = `通道 ${String(slot).padStart(2,'0')} · 第 ${lane.call} 次调用${lane.attempt ? ' · 重试 '+lane.attempt : ''}`;
      void detail(lane);
    }
  }
  async function detail(lane) {
    if (!lane) return;
    const key = `${jobId}:${slot}:${lane.call}:${lane.revision}:${$('live-item').value}`;
    if (key === detailKey || detailBusy) return;
    detailBusy = true; const serial = ++detailSerial, requestedJob = jobId, requestedSlot = slot, index = $('live-item').value;
    try {
      const item = await api(`jobs/${jobId}/lane?slot=${slot}${index === '' ? '' : '&index='+index}`);
      if (serial !== detailSerial || jobId !== requestedJob || slot !== requestedSlot || $('live-item').value !== index) return;
      detailKey = key;
      const source = $('live-source'); source.replaceChildren();
      if (!item) { source.textContent='等待第一条文本输出，或从上方选择本批条目。'; $('live-result').textContent=''; $('live-status').textContent='提取片段'; return; }
      const at = item.text && item.source.indexOf(item.text);
      if (item.text && at >= 0 && item.state !== 'error') {
        const mark = document.createElement('mark'); mark.textContent = item.text;
        source.append(document.createTextNode(item.source.slice(0,at)), mark, document.createTextNode(item.source.slice(at+item.text.length)));
      } else source.textContent = item.source;
      $('live-result').textContent = item.state === 'none' ? '未发现画风提示词' : item.error || item.text || '等待输出…';
      $('live-status').textContent = `第 ${item.number} 条 · `+(item.phase === 'paused' && item.state === 'streaming' ? '已暂停 · 片段未确认' : {ok:'已校验并保存',none:'已校验并保存',error:'校验未通过',streaming:'接收中 · 尚未确认',waiting:'等待输出'}[item.state]||item.state);
    } catch { /* The stream reconnect indicator covers connection failures. */ }
    finally { detailBusy = false; }
  }
  $('live-close').onclick=()=>{slot=null;detailKey='';++detailSerial;paint(snapshot)};
  $('live-item').onchange=()=>{detailKey='';++detailSerial;paint(snapshot)};
  async function connect(id,signal) {
    while (!signal.aborted) {
      try {
        $('live-connection').textContent='连接实时进度…';
        const res=await fetch(`/api/jobs/${id}/events`,{headers:{'X-Session-Token':token},signal});
        if(res.status===403){location.reload();return} if(!res.ok)throw Error('连接失败');
        $('live-connection').textContent='实时连接';
        const reader=res.body.getReader(),decoder=new TextDecoder();let pending='';
        while(!signal.aborted){const {done,value}=await reader.read();if(done)break;pending+=decoder.decode(value,{stream:true});let at;
          while((at=pending.indexOf('\n\n'))>=0){const chunk=pending.slice(0,at);pending=pending.slice(at+2);if(!chunk.startsWith('data: '))continue;
            const data=JSON.parse(chunk.slice(6));if(selected!==id||signal.aborted)return;render(data.job);paint(data);
          }
        }
      } catch { if(signal.aborted)return; }
      if(!signal.aborted){$('live-connection').textContent='连接中断 · 自动重连';await new Promise(resolve=>{const t=setTimeout(end,1500);function end(){clearTimeout(t);signal.removeEventListener('abort',end);resolve()}signal.addEventListener('abort',end,{once:true});if(signal.aborted)end()})}
    }
  }
  setInterval(()=>{
    if(selected!==jobId){connection?.abort();jobId=selected;clear();if(jobId){connection=new AbortController();void connect(jobId,connection.signal)}}
    else if(slot!==null&&snapshot)void detail(snapshot.lanes.find(l=>l.slot===slot));
  },250);
})();
