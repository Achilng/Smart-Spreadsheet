const $ = id => document.getElementById(id);
let channels = [], editing = null, dirty = false, modelOptions = [], revision = 0;
function notify(text = '', error = '') { $('notice').textContent = text; $('error').textContent = error; }
function resetModels() { revision++; modelOptions = []; $('modelPicker').hidden = true; $('fetchModels').disabled = false; $('modelStatus').textContent = '支持各渠道使用不同模型，也支持不同的模型名称。'; }
function renderList() {
  $('count').textContent = channels.length + ' 个';
  $('channelList').replaceChildren(...channels.map(channel => {
    const button = document.createElement('button'); button.className = 'channel-item'; button.setAttribute('aria-pressed', String(editing === channel.id));
    const title = document.createElement('strong'), meta = document.createElement('small'); title.textContent = channel.name;
    meta.textContent = `${channel.model} · ${channel.concurrency} 并发${channel.enabled ? '' : ' · 已停用'}${channel.apiKey ? '' : ' · 未填 Key'}`;
    button.append(title, meta); button.onclick = () => { if (!dirty || confirm('当前修改尚未保存，放弃修改并切换渠道？')) open(channel); }; return button;
  }));
}
function open(channel) {
  editing = channel?.id || null; dirty = false; resetModels(); notify();
  const value = channel || { name: '', baseUrl: publicApiBase || '', apiKey: '', model: '', concurrency: 1, enabled: true };
  for (const id of ['name', 'baseUrl', 'apiKey', 'model', 'concurrency']) $(id).value = value[id];
  $('enabled').checked = value.enabled; $('remove').disabled = !editing;
  $('editorTitle').textContent = editing ? '编辑渠道' : '添加渠道'; $('savedState').textContent = editing ? '已保存' : '新渠道'; renderList();
}
function persist(next) { channelStore.save(next); channels = next; }
$('editor').oninput = () => { dirty = true; $('savedState').textContent = '尚未保存'; };
$('baseUrl').addEventListener('input', () => { $('apiKey').value = ''; resetModels(); $('modelStatus').textContent = '地址已更改，请填写这个接口的 Key。'; });
$('apiKey').addEventListener('input', resetModels);
$('add').onclick = () => { if (!dirty || confirm('当前修改尚未保存，放弃修改并添加渠道？')) { open(null); $('name').focus(); } };
$('editor').onsubmit = event => {
  event.preventDefault(); notify();
  try {
    const channel = channelStore.validate({ id: editing || crypto.randomUUID(), name: $('name').value.trim(), baseUrl: $('baseUrl').value.trim(), apiKey: $('apiKey').value.trim(), model: $('model').value.trim(), concurrency: Number($('concurrency').value), enabled: $('enabled').checked });
    // Re-read to preserve unrelated changes made in another tab.
    const current = channelStore.load(), index = current.findIndex(x => x.id === channel.id);
    if (index < 0) current.push(channel); else current[index] = channel;
    persist(current); open(channel); notify('已保存。返回工作台选择渠道后即可使用。');
  } catch (error) { notify('', error.message); }
};
$('clearKey').onclick = () => {
  try {
    if (editing) {
      const current = channelStore.load(), old = current.find(x => x.id === editing);
      if (old) { localStorage.removeItem('style-api-key:' + old.baseUrl); old.apiKey = ''; persist(current); }
    }
    $('apiKey').value = ''; resetModels(); notify('已清除该渠道保存的 Key。正在进行的调用会继续，停止调用请回工作台暂停。');
  } catch (error) { notify('', error.message); }
};
$('remove').onclick = () => {
  if (!editing || !confirm('删除此渠道配置？已保存的任务结果会保留，正在运行的任务不会自动停止。')) return;
  try { persist(channelStore.load().filter(x => x.id !== editing)); open(channels[0]); notify('渠道已删除。'); } catch (error) { notify('', error.message); }
};
function filterModels() {
  const query = $('modelSearch').value.trim().toLowerCase();
  $('modelChoices').replaceChildren(...modelOptions.filter(x => x.toLowerCase().includes(query)).map(x => new Option(x, x)));
  $('modelChoices').selectedIndex = -1;
}
$('fetchModels').onclick = async () => {
  const request = ++revision; $('fetchModels').disabled = true; $('modelStatus').textContent = '正在读取模型列表…';
  try {
    const response = await fetch('/api/models', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Session-Token': token }, body: JSON.stringify({ baseUrl: $('baseUrl').value.trim(), apiKey: $('apiKey').value.trim() }) });
    const data = await response.json(); if (!response.ok) throw Error(data.error || '读取失败');
    if (request !== revision) return;
    modelOptions = data.models; $('modelSearch').value = ''; filterModels(); $('modelPicker').hidden = !modelOptions.length;
    $('modelStatus').textContent = modelOptions.length ? `已获取 ${modelOptions.length} 个模型，点击选择。` : '列表为空，可手动填写模型名。';
  } catch (error) { if (request === revision) $('modelStatus').textContent = error.message; }
  finally { if (request === revision) $('fetchModels').disabled = false; }
};
$('modelSearch').oninput = filterModels;
$('modelChoices').onchange = () => { if ($('modelChoices').value) { $('model').value = $('modelChoices').value; dirty = true; $('savedState').textContent = '尚未保存'; } };
window.addEventListener('beforeunload', event => { if (dirty) { event.preventDefault(); event.returnValue = ''; } });
window.addEventListener('storage', event => { if (event.key === channelStore.storageKey && !dirty) { try { channels = channelStore.load(); open(channels.find(x => x.id === editing) || channels[0]); } catch (error) { notify('', error.message); } } });
try { channels = channelStore.load(); open(channels[0]); } catch (error) { notify('', error.message); $('editor').inert = true; }
