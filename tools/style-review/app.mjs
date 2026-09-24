import { REVIEW, validateResult, cleanReviews, statistics, visibleIndices } from './core.mjs';
const $ = id => document.getElementById(id);
let doc = null, reviews = {}, cursor = 0, datasetId = '', filename = '', loadVersion = 0;
const labels = { correct: '正确', wrong: '错误', unsure: '待定', pending: '未批改', error: '调用失败' };
function notice(message = '') { $('notice').hidden = !message; $('notice').textContent = message; }
function storageKey() { return 'style-review:v1:' + datasetId; }
function persist() {
  try { localStorage.setItem(storageKey(), JSON.stringify({ reviews, cursor, autoNext: $('autoNext').checked })); $('saveState').textContent = '已自动保存到当前浏览器 · ' + new Date().toLocaleTimeString(); }
  catch { $('saveState').textContent = '自动保存失败，请导出批改记录'; notice('浏览器存储不可用或已满，请用“导出批改记录”保存进度。'); }
}
async function loadDocument(input, name) {
  const version = ++loadVersion;
  const report = input?.format === REVIEW ? input : null;
  if (report && report.version !== 1) throw Error('不支持此批改记录版本');
  const source = validateResult(report ? report.source_result : input);
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(JSON.stringify(source)));
  const id = Array.from(new Uint8Array(digest), x => x.toString(16).padStart(2, '0')).join('');
  if (version !== loadVersion) return;
  if (report && report.dataset_id !== id) throw Error('批改记录与所附原始结果不匹配');
  let cached = {};
  try { cached = JSON.parse(localStorage.getItem('style-review:v1:' + id) || '{}'); } catch { /* An exported review can still be loaded. */ }
  const merged = cleanReviews(source.items, cached.reviews);
  if (report) for (const [index, r] of Object.entries(cleanReviews(source.items, report.reviews))) {
    if (!merged[index] || r.updatedAt >= merged[index].updatedAt) merged[index] = r;
  }
  doc = source; reviews = merged; datasetId = id; filename = report?.source_filename || name;
  cursor = Number.isInteger(cached.cursor) && cached.cursor >= 0 && cached.cursor < doc.items.length ? cached.cursor : 0;
  $('autoNext').checked = cached.autoNext !== false;
  $('filter').value = 'all'; $('search').value = ''; notice(); render(); persist();
}
function indices() { return doc ? visibleIndices(doc.items, reviews, $('filter').value, $('search').value) : []; }
function renderOverview() {
  const s = statistics(doc.items, reviews);
  $('filename').textContent = filename + ' · ' + s.total + ' 条';
  $('accuracy').textContent = s.accuracy === null ? '—' : (s.accuracy * 100).toFixed(1) + '%';
  $('formula').textContent = `${s.correct} 正确 ÷ ${s.judged} 已判对错`;
  $('judged').textContent = `${s.judged} / ${s.eligible}`;
  for (const key of ['correct', 'wrong', 'unsure', 'errors']) $(key).textContent = s[key];
  $('progress').max = Math.max(s.eligible, 1); $('progress').value = s.judged;
  $('coverage').textContent = `未批改 ${s.pending} 条 · 待定 ${s.unsure} 条 · 调用失败 ${s.errors} 条不计准确率`;
  $('export').disabled = false; $('mistakes').disabled = !s.wrong;
}
function renderList(list) {
  const fragment = document.createDocumentFragment();
  for (const i of list) {
    const item = doc.items[i], state = item.status === 'error' ? 'error' : reviews[i]?.decision || 'pending';
    const button = document.createElement('button'); button.className = 'sample' + (i === cursor ? ' active' : '');
    button.setAttribute('aria-current', String(i === cursor)); button.title = `第 ${i + 1} 条 · ${labels[state]}`;
    const top = document.createElement('div'); top.className = 'sample-top';
    const number = document.createElement('span'); number.textContent = String(i + 1).padStart(3, '0');
    const badge = document.createElement('span'); badge.className = 'badge ' + state; badge.textContent = labels[state];
    top.append(number, badge);
    const preview = document.createElement('div'); preview.className = 'sample-preview'; preview.textContent = item.positive_prompt.slice(0, 160);
    button.append(top, preview); button.onclick = () => { cursor = i; render(); persist(); }; fragment.append(button);
  }
  $('list').replaceChildren(fragment); $('listCount').textContent = list.length + ' 条符合当前范围';
}
function render() {
  if (!doc) return;
  const list = indices(); renderOverview();
  if (!list.includes(cursor)) cursor = list[0] ?? -1;
  renderList(list); $('empty').hidden = !!list.length; $('review').hidden = !list.length;
  if (!list.length) { $('empty').textContent = '当前范围没有条目。可以切换筛选，继续检查。'; return; }
  const item = doc.items[cursor], r = reviews[cursor] || {}, failed = item.status === 'error';
  $('number').textContent = `样本 ${String(cursor + 1).padStart(3, '0')} / ${doc.items.length}`;
  $('sampleTitle').textContent = failed ? '调用失败，未获得有效提取结果' : '检查画风段的范围与原文';
  $('model').textContent = `${doc.processor?.model || '未知模型'} · ${doc.processor?.prompt_version || ''}`;
  $('source').replaceChildren();
  const start = item.artist_string ? item.positive_prompt.indexOf(item.artist_string) : -1;
  if (!failed && start >= 0) {
    const mark = document.createElement('mark'); mark.textContent = item.artist_string;
    $('source').append(document.createTextNode(item.positive_prompt.slice(0, start)), mark, document.createTextNode(item.positive_prompt.slice(start + item.artist_string.length)));
  } else $('source').textContent = item.positive_prompt;
  $('locate').disabled = failed || start < 0;
  $('source').scrollTop = 0; $('result').scrollTop = 0;
  $('result').textContent = failed ? String(item.error || '模型调用失败') : item.artist_string || '（空：模型认为没有画风段）';
  $('length').textContent = failed ? '未计入准确率' : item.artist_string.length + ' 字符';
  $('resultHint').textContent = failed ? '这是调用或格式校验失败，不等于提取正确或错误。' : item.status === 'none' ? '请判断原文是否确实没有可提取的画风段。' : start < 0 ? '注意：结果不是原文中的连续片段。' : '检查是否多提、漏提，以及权重和末尾分隔符是否保留。';
  for (const [button, decision] of [['markCorrect', 'correct'], ['markWrong', 'wrong'], ['markUnsure', 'unsure']]) { $(button).disabled = failed; $(button).setAttribute('aria-pressed', String(r.decision === decision)); }
  $('clear').disabled = failed || !r.decision;
  $('decision').textContent = failed ? '此条仅供查看，排除在提取准确率之外。' : r.decision ? '你的判定：' + labels[r.decision] : '尚未批改';
  $('note').value = r.note || ''; $('note').disabled = failed;
  $('hasCorrection').checked = typeof r.correction === 'string'; $('hasCorrection').disabled = failed;
  $('correction').value = r.correction ?? ''; $('correction').disabled = failed || !$('hasCorrection').checked;
  const position = list.indexOf(cursor); $('prev').disabled = position <= 0; $('next').disabled = position >= list.length - 1;
}
function record(patch) {
  if (!doc || cursor < 0 || doc.items[cursor].status === 'error') return;
  reviews[cursor] = { decision: '', note: '', correction: null, ...reviews[cursor], ...patch, updatedAt: new Date().toISOString() }; persist();
}
function mark(decision) {
  if (!doc || cursor < 0 || doc.items[cursor].status === 'error') return;
  const list = indices(), position = list.indexOf(cursor);
  record({ decision });
  if ($('autoNext').checked && decision) cursor = list[position + 1] ?? cursor;
  render(); persist();
}
function move(delta) { const list = indices(), next = list[list.indexOf(cursor) + delta]; if (next !== undefined) { cursor = next; render(); persist(); } }
function download(value, type, name) {
  const url = URL.createObjectURL(new Blob([value], { type })), a = document.createElement('a'); a.href = url; a.download = name; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
}
$('load').onclick = () => $('file').click();
$('file').onchange = async () => { const file = $('file').files[0]; if (!file) return; try { if (file.size > 64 * 1024 * 1024) throw Error('文件超过 64 MB'); await loadDocument(JSON.parse((await file.text()).replace(/^\uFEFF/, '')), file.name); } catch (e) { notice(e.message); } finally { $('file').value = ''; } };
$('filter').onchange = () => render(); $('search').oninput = () => render();
$('markCorrect').onclick = () => mark('correct'); $('markWrong').onclick = () => mark('wrong'); $('markUnsure').onclick = () => mark('unsure'); $('clear').onclick = () => mark('');
$('prev').onclick = () => move(-1); $('next').onclick = () => move(1);
$('nextPending').onclick = () => {
  if (!doc) return;
  const pending = doc.items.flatMap((item, i) => item.status !== 'error' && !reviews[i]?.decision ? [i] : []);
  const next = pending.find(i => i > cursor) ?? pending[0];
  if (next === undefined) { notice('没有未批改项了，可切换到“待定的”继续复核。'); return; }
  notice(); $('filter').value = 'all'; $('search').value = ''; cursor = next; render(); persist();
};
$('note').oninput = () => record({ note: $('note').value });
$('correction').oninput = () => record({ correction: $('correction').value });
$('hasCorrection').onchange = () => { record({ correction: $('hasCorrection').checked ? $('correction').value : null }); $('correction').disabled = !$('hasCorrection').checked; };
$('autoNext').onchange = persist;
$('locate').onclick = () => { const mark = $('source').querySelector('mark'); if (mark) $('source').scrollTop = mark.getBoundingClientRect().top - $('source').getBoundingClientRect().top + $('source').scrollTop - 20; };
$('export').onclick = () => {
  if (!doc) return;
  download(JSON.stringify({ format: REVIEW, version: 1, dataset_id: datasetId, source_filename: filename, exported_at: new Date().toISOString(), summary: statistics(doc.items, reviews), source_result: doc, reviews }, null, 2), 'application/json', '画风批改记录-' + datasetId.slice(0, 8) + '.json');
};
$('mistakes').onclick = () => {
  const blocks = doc.items.flatMap((item, i) => reviews[i]?.decision === 'wrong' ? [`第 ${i + 1} 条\n\n【原文】\n${item.positive_prompt}\n\n【模型结果】\n${item.artist_string || '（空）'}\n\n【备注】\n${reviews[i].note || '（无）'}\n\n【人工参考答案】\n${reviews[i].correction === null ? '（未填写）' : reviews[i].correction || '（应为空）'}`] : []);
  download('\uFEFF' + blocks.join('\n\n' + '─'.repeat(60) + '\n\n'), 'text/plain;charset=utf-8', '画风提取错题-' + datasetId.slice(0, 8) + '.txt');
};
document.addEventListener('keydown', e => {
  if (e.ctrlKey || e.metaKey || e.altKey || e.repeat || /INPUT|TEXTAREA|SELECT/.test(e.target.tagName) || e.target.isContentEditable) return;
  const action = { '1': () => mark('correct'), '2': () => mark('wrong'), '3': () => mark('unsure'), ArrowLeft: () => move(-1), ArrowRight: () => move(1) }[e.key];
  if (action) { e.preventDefault(); action(); }
});
try { const initial = await (await fetch('/initial')).json(); if (initial) await loadDocument(initial.document, initial.filename); } catch (e) { notice('自动载入失败：' + e.message + '。也可以点击打开结果手动选择文件。'); }
