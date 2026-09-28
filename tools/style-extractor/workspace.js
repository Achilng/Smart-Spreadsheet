const $=id=>document.getElementById(id);
let selected=localStorage.getItem('style-job')||'',busy=false,current=null,choiceJob=null;
const clockSamples=new Map();
// Network updates calibrate the clock; they never drive its visible ticks.
let runClock=null,clockTimer,starting=null;
function syncClock(j){
  if(!j){runClock=null;return}
  const running=j.state==='running',now=performance.now();
  if(!runClock||runClock.id!==j.id||runClock.running!==running||runClock.runId!==j.runId){
    runClock={id:j.id,runId:j.runId,running,ms:j.elapsedMs,at:now};
  }else if(!running){runClock.ms=j.elapsedMs;runClock.at=now}
  // A long background suspension or reconnect can require a forward correction.
  else if(j.elapsedMs-(runClock.ms+now-runClock.at)>1500){runClock.ms=j.elapsedMs;runClock.at=now}
}
function drawClock(){
  clearTimeout(clockTimer);const now=performance.now();
  const elapsed=runClock?runClock.ms+(runClock.running?now-runClock.at:0):0;
  const text=time(Math.max(0,elapsed));if($('elapsed').textContent!==text)$('elapsed').textContent=text;
  if(starting?.id===selected&&!runClock?.running){
    $('state').textContent='正在启动 · '+time(now-starting.at);
    $('start').textContent='正在启动…';
  }else{$('start').textContent='开始 / 继续'}
  if(runClock?.running||starting)clockTimer=setTimeout(drawClock,Math.max(16,1000-((starting&&!runClock?.running?now-starting.at:elapsed)%1000)));
}
document.addEventListener('visibilitychange',()=>{if(!document.hidden){drawClock();void refresh()}});
function time(ms){let s=Math.floor(ms/1000);return [Math.floor(s/3600),Math.floor(s/60)%60,s%60].map(x=>String(x).padStart(2,'0')).join(':')}
async function api(route, body) {
  const response = await fetch('/api/' + route, { method: body === undefined ? 'GET' : 'POST', headers: { 'Content-Type': 'application/json', 'X-Session-Token': token }, body: body === undefined ? undefined : JSON.stringify(body) });
  const data = await response.json(); if (!response.ok) throw Error(data.error || '连接失败'); return data;
}
function selectedIds(container) { return [...$(container).querySelectorAll('input:checked')].map(x => x.value); }
function selectionKey(container) { return 'style-channel-selection:' + (container === 'newChannels' ? 'new' : selected); }
function total(container) {
  const ids = new Set(selectedIds(container)), channels = channelStore.load().filter(x => ids.has(x.id) && x.enabled);
  const sum = channels.reduce((value, x) => value + x.concurrency, 0);
  $(container === 'newChannels' ? 'newTotal' : 'runTotal').textContent = `${channels.length} 个渠道 · 合计 ${sum} 并发`;
}
function choices(container, job) {
  const channels = channelStore.load(), stored = localStorage.getItem(selectionKey(container));
  let ids;
  if (stored !== null) ids = JSON.parse(stored);
  else if (job?.settings.channels) ids = job.settings.channels.filter(x => x.enabled).map(x => x.id);
  else if (job) ids = channels.filter(x => (x.baseUrl === job.settings.baseUrl || (serverManaged && /^http:\/\/(127\.0\.0\.1|localhost)/.test(job.settings.baseUrl || '') && x.baseUrl === publicApiBase)) && x.model === job.settings.model).map(x => x.id);
  else ids = channels.filter(x => x.enabled).map(x => x.id);
  const missing = ids.filter(id => !channels.some(x => x.id === id));
  const values = [...channels, ...missing.map(id => ({ id, name: job?.settings.channels?.find(x => x.id === id)?.name || '其他浏览器的渠道', model: '本浏览器没有此渠道，请在管理页添加或取消勾选', concurrency: 0, enabled: true, missing: true }))];
  $(container).replaceChildren(...values.map(channel => {
    const label = document.createElement('label'); label.className = 'channel-choice';
    const input = document.createElement('input'); input.type = 'checkbox'; input.value = channel.id; input.checked = ids.includes(channel.id); input.disabled = !channel.enabled;
    const text = document.createElement('span'), title = document.createElement('strong'), info = document.createElement('small'); title.textContent = channel.name;
    info.textContent = `${channel.model} · ${channel.concurrency} 并发${channel.enabled ? '' : ' · 已停用'}${!channel.apiKey && !channel.missing ? ' · 未填 Key' : ''}`;
    text.append(title, info); label.append(input, text);
    input.onchange = () => { try { localStorage.setItem(selectionKey(container), JSON.stringify(selectedIds(container))); total(container); } catch (error) { $('error').textContent = error.message; } };
    return label;
  }));
  if (!values.length) $(container).textContent = '还没有渠道，请先进入渠道管理添加。'; total(container);
}
function configs(container, withKeys = false, update = false) {
  const channels = channelStore.load(), ids = selectedIds(container);
  if (ids.some(id => !channels.some(x => x.id === id))) throw Error('所选渠道不在当前浏览器，请添加配置或取消勾选缺失的渠道。');
  const values = (update ? channels : channels.filter(x => ids.includes(x.id) && x.enabled)).map(x => channelStore.validate({ ...x, enabled: x.enabled && ids.includes(x.id) }));
  if (!update && !values.length) throw Error('请至少选择一个启用的渠道。');
  if (withKeys) for (const channel of values) if (channel.enabled && !channel.apiKey.trim()) throw Error(`请在渠道管理中填写「${channel.name}」的 API Key。`);
  return withKeys ? values : channelStore.metadata(values);
}
function renderChannels(j) {
  const names = { ready: j.state === 'running' ? '运行中' : j.state === 'paused' ? '已暂停' : '已结束', disabled: '已停用', unavailable: '暂不可用', cooldown: '等待恢复' };
  $('channelStatusList').setAttribute('aria-label', '本轮渠道运行情况');
  $('channelStatusList').replaceChildren(...(j.channels || []).map(channel => {
    const row = document.createElement('div'); row.className = 'channel-status';
    const title = document.createElement('strong'), info = document.createElement('p'), detail = document.createElement('small');
    title.textContent = `${channel.name} · ${names[channel.state] || channel.state}`;
    info.textContent = `${channel.inFlight} / ${channel.concurrency} 个请求 · 本轮成功 ${channel.success} 条 · 失败尝试 ${channel.failures} 条`;
    detail.textContent = channel.model + (channel.error ? ' · ' + channel.error : ''); row.append(title, info, detail); return row;
  }));
}
function render(j) {
  if (j && (j.id !== selected || clockSamples.get(j.id) > j.sampledAt)) return;
  if (j) clockSamples.set(j.id, j.sampledAt); syncClock(j); current = j;
  $('empty').classList.toggle('hidden', !!j); $('detail').classList.toggle('hidden', !j); if (!j) return;
  const isApi = j.settings.provider === 'api';
  if (choiceJob !== j.id) { choiceJob = j.id; $('runConcurrency').value = j.settings.concurrency || 1; if (isApi) choices('runChannels', j); $('runSettings').open = j.state !== 'running'; }
  $('runApi').hidden = !isApi; $('runCodex').hidden = isApi; $('runConcurrency').disabled = busy || j.state === 'running';
  $('state').textContent = { running: '正在提取', paused: '已暂停 / 待开始', completed: '本轮完成' }[j.state];
  $('modelLabel').textContent = isApi ? (j.settings.channels ? j.settings.channels.filter(x => x.enabled).length : 1) + ' 个渠道' : 'Codex CLI';
  $('identity').textContent = `任务 ${j.id.slice(0,8)} · 每次最多 ${j.settings.batchSize} 条 · ${j.settings.concurrency} 并发 · ${j.settings.effort}`;
  $('done').textContent = `${j.ok + j.none} / ${j.total}`; $('none').textContent = j.none; $('errors').textContent = j.errors; drawClock();
  const p = Math.round((j.ok + j.none + j.errors) / j.total * 100); $('fill').style.width = p + '%'; $('progress').setAttribute('aria-valuenow', p);
  $('percent').textContent = `${p}% 已处理 · ${j.pending} 条待处理`; $('calls').textContent = `${j.calls} 次调用 · ${j.inFlight || 0} 个请求进行中`; $('message').textContent = j.message;
  $('start').disabled = busy || j.state === 'running' || !j.pending; $('pause').disabled = busy || j.state !== 'running'; $('retry').disabled = busy || j.state === 'running' || !j.errors;
  $('download').disabled = busy || !(j.ok + j.none + j.errors); $('applyChannels').disabled = busy || j.state !== 'running' || !j.settings.channels;
  $('logs').textContent = j.logs.slice(-14).map(x => new Date(x.time).toLocaleTimeString() + '  ' + x.message).join('\n');
  $('failureCard').classList.toggle('hidden', !j.errors); $('failures').replaceChildren(...j.failures.map(f => { const p = document.createElement('p'); p.textContent = f.id.slice(0,22) + '…\n' + f.error; return p; })); renderChannels(j);
}
let refreshing = false;
async function refresh() {
  if (refreshing) return; refreshing = true;
  try {
    const jobs = await api('jobs'); $('connection').textContent = '已连接 · 进度自动保存';
    $('jobs').replaceChildren(...jobs.map(j => new Option(new Date(j.createdAt).toLocaleString() + ' · ' + j.total + ' 条', j.id)));
    if (!jobs.some(j => j.id === selected)) selected = jobs[0]?.id || ''; $('jobs').value = selected; render(jobs.find(j => j.id === selected) || null);
  } catch (error) { $('connection').textContent = error.message.includes('刷新') ? '服务已重启，请刷新页面' : '连接中断，正在自动重连'; if (error.message.includes('刷新')) location.reload(); }
  finally { refreshing = false; }
}
async function perform(fn) {
  if (busy) return; busy = true; $('create').disabled = true; $('error').textContent = ''; if (current) render(current);
  try { await fn(); await refresh(); } catch (error) { $('error').textContent = error.message; }
  finally { busy = false; $('create').disabled = false; if (current) render(current); }
}
async function startJob(retryErrors = false) {
  const id = selected; starting = { id, at: performance.now() }; drawClock();
  try {
    const options = current.settings.provider === 'api' ? { channels: configs('runChannels', true) } : { concurrency: Number($('runConcurrency').value) };
    const j = await api('jobs/' + id + '/start', { retryErrors, ...options }); if (selected === id) render(j);
  } finally { starting = null; if (current) render(current); }
}
$('create').onclick = () => perform(async () => {
  const file = $('file').files[0]; if (!file) throw Error('请先选择待处理 JSON');
  const request = JSON.parse((await file.text()).replace(/^\uFEFF/, ''));
  const settings = { provider: $('provider').value, batchSize: Number($('batch').value), effort: $('effort').value };
  if (settings.provider === 'api') settings.channels = configs('newChannels'); else settings.concurrency = Number($('concurrency').value);
  const j = await api('jobs', { request, settings }); selected = j.id; localStorage.setItem('style-job', selected); choiceJob = null;
});
$('jobs').onchange = () => { selected = $('jobs').value; localStorage.setItem('style-job', selected); void refresh(); };
$('start').onclick = () => perform(() => startJob()); $('retry').onclick = () => perform(() => startJob(true));
$('pause').onclick = () => perform(() => api('jobs/' + selected + '/pause', {}));
$('applyChannels').onclick = () => perform(() => api('jobs/' + selected + '/channels', { channels: configs('runChannels', true, true) }));
$('download').onclick = () => perform(async () => {
  const doc = await api('jobs/' + selected + '/result'), url = URL.createObjectURL(new Blob([JSON.stringify(doc, null, 2)], { type: 'application/json' }));
  const a = document.createElement('a'); a.href = url; a.download = '画风提取结果-' + selected.slice(0,8) + '.json'; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
});
$('provider').onchange = () => { $('newApi').hidden = $('provider').value !== 'api'; $('codexConcurrency').hidden = $('provider').value === 'api'; };
$('providerField').hidden = serverManaged;
function reloadChoices() { try { choices('newChannels'); if (current?.settings.provider === 'api') choices('runChannels', current); } catch (error) { $('error').textContent = error.message; } }
window.addEventListener('storage', event => { if (event.key === channelStore.storageKey) reloadChoices(); });
window.addEventListener('pageshow', reloadChoices);
reloadChoices(); refresh(); setInterval(refresh, 1500);
