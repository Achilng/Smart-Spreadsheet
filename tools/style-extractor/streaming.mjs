// Incremental parsing is only for previews and individually closed objects.
// The full document is still parsed and validated by the caller at completion.
export class ItemStream {
  constructor(emit = () => {}) { this.emit = emit; this.text = ''; this.cursor = 0; this.started = false; this.objectStart = -1; this.depth = 0; this.quoted = false; this.escape = false; this.preview = ''; }
  push(delta) {
    this.text += delta;
    if (this.text.length > 8 * 1024 * 1024) throw Error('模型输出超过 8 MB 上限');
    if (this.ended) return;
    if (!this.started) {
      const prefix = /^\s*(?:```(?:json)?\s*)?\{\s*"items"\s*:\s*\[/i.exec(this.text);
      if (!prefix) return;
      this.cursor = prefix[0].length; this.started = true;
    }
    for (; this.cursor < this.text.length; this.cursor++) {
      const c = this.text[this.cursor];
      if (this.objectStart < 0) { if (c === '{') { this.objectStart = this.cursor; this.depth = 1; } else if (c === ']') { this.ended = true; break; } continue; }
      if (this.quoted) { if (this.escape) this.escape = false; else if (c === '\\') this.escape = true; else if (c === '"') this.quoted = false; }
      else if (c === '"') this.quoted = true;
      else if (c === '{' || c === '[') this.depth++;
      else if (c === '}' || c === ']') {
        if (--this.depth === 0) {
          const raw = this.text.slice(this.objectStart, this.cursor + 1);
          this.objectStart = -1; this.preview = '';
          let item; try { item = JSON.parse(raw); } catch { continue; }
          this.emit({ type: 'item', item });
        }
      }
    }
    if (this.objectStart >= 0) {
      const fields = partialFields(this.text.slice(this.objectStart));
      if (fields.id !== undefined && typeof fields.artist_string === 'string') {
        const key = JSON.stringify(fields);
        if (key !== this.preview) { this.preview = key; this.emit({ type: 'preview', item: fields }); }
      }
    }
  }
  finish() { return JSON.parse(this.text.trim().replace(/^```(?:json)?\s*/i, '').replace(/\s*```$/, '')); }
}

function readString(text, start) {
  let value = '', i = start + 1;
  for (; i < text.length; i++) {
    const c = text[i];
    if (c === '"') return { value, end: i + 1, complete: true };
    if (c !== '\\') { value += c; continue; }
    const escaped = text[++i]; if (escaped === undefined) break;
    if (escaped === 'u') { const hex = text.slice(i + 1, i + 5); if (!/^[\da-f]{4}$/i.test(hex)) break; value += String.fromCharCode(parseInt(hex, 16)); i += 4; }
    else { const map = { '"': '"', '\\': '\\', '/': '/', b: '\b', f: '\f', n: '\n', r: '\r', t: '\t' }; if (!(escaped in map)) break; value += map[escaped]; }
  }
  // A split surrogate pair must not render as an invalid character.
  return { value: value.replace(/[\uD800-\uDBFF]$/, ''), end: i, complete: false };
}
function partialFields(text) {
  const fields = {}; let i = 1;
  while (i < text.length) {
    while (/[\s,]/.test(text[i] ?? '') && i < text.length) i++;
    if (text[i] !== '"') break;
    const key = readString(text, i); if (!key.complete) break; i = key.end;
    while (/\s/.test(text[i] ?? '') && i < text.length) i++;
    if (text[i++] !== ':') break;
    while (/\s/.test(text[i] ?? '') && i < text.length) i++;
    if (text[i] === '"') {
      const v = readString(text, i);
      if (v.complete || key.value === 'artist_string') fields[key.value] = v.value;
      if (!v.complete) break; i = v.end;
    } else { const v = /^\d+(?=[\s,}])/.exec(text.slice(i)); if (!v) break; fields[key.value] = Number(v[0]); i += v[0].length; }
  }
  return fields;
}

export async function readSse(body, onData) {
  const decoder = new TextDecoder(); let pending = '', data = [];
  function line(value) {
    if (value === '') { if (data.length) onData(data.join('\n')); data = []; }
    else if (value.startsWith('data:')) data.push(value.slice(5).replace(/^ /, ''));
  }
  function drain(final = false) {
    let at;
    while ((at = pending.search(/[\r\n]/)) >= 0) {
      if (!final && pending[at] === '\r' && at === pending.length - 1) break;
      const size = pending[at] === '\r' && pending[at + 1] === '\n' ? 2 : 1;
      const value = pending.slice(0, at); pending = pending.slice(at + size); line(value);
    }
  }
  if (!body) throw Error('接口未返回响应流');
  for await (const bytes of body) { pending += decoder.decode(bytes, { stream: true }); if (pending.length > 8 * 1024 * 1024) throw Error('响应事件超过大小上限'); drain(); }
  pending += decoder.decode(); drain(true); if (pending) line(pending); line('');
}
