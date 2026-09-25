import fs from 'node:fs';
import path from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import { validateResult, cleanReviews, REVIEW } from './core.mjs';
export class ReviewStore {
  constructor(directory) { this.directory = directory; fs.mkdirSync(directory, { recursive: true }); }
  file(id) { if (!/^[a-f0-9]{64}$/.test(id)) throw Error('批改编号无效'); return path.join(this.directory, id + '.json'); }
  get(id) { const file = this.file(id); return fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : null; }
  list() { return fs.readdirSync(this.directory).filter(x => /^[a-f0-9]{64}\.json$/.test(x)).map(x => { const value = this.get(x.slice(0, -5)); return { id: x.slice(0, -5), filename: value.filename, updatedAt: value.updatedAt }; }).sort((a,b) => b.updatedAt.localeCompare(a.updatedAt)); }
  save(id, value) {
    const report = value.document, source = validateResult(report?.source_result);
    const actual = createHash('sha256').update(JSON.stringify(source)).digest('hex');
    if (actual !== id || report.dataset_id !== id || report.format !== REVIEW || report.version !== 1) throw Error('批改内容与编号不匹配');
    const previous = this.get(id), merged = cleanReviews(source.items, previous?.document.reviews);
    for (const [key, item] of Object.entries(cleanReviews(source.items, report.reviews))) {
      if (!merged[key] || item.updatedAt >= merged[key].updatedAt) merged[key] = item;
    }
    const result = { filename: String(value.filename || '批改记录').slice(0, 250), updatedAt: new Date().toISOString(), cursor: Number.isInteger(value.cursor) ? value.cursor : 0, autoNext: value.autoNext !== false,
      document: { format: REVIEW, version: 1, dataset_id: id, source_result: source, source_filename: String(report.source_filename || value.filename || '').slice(0, 250), reviews: merged } };
    const target = this.file(id), temporary = target + '.' + randomUUID() + '.tmp';
    const fd = fs.openSync(temporary, 'w', 0o600);
    try { fs.writeFileSync(fd, JSON.stringify(result)); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
    fs.renameSync(temporary, target);
    return result;
  }
}
