export const RESULT = 'smart-spreadsheet.style-extraction.result';
export const REVIEW = 'smart-spreadsheet.style-review';
export const decisions = ['correct', 'wrong', 'unsure'];
export function validateResult(doc) {
  if (doc?.format !== RESULT || doc.version !== 1 || !Array.isArray(doc.items) || !doc.items.length) throw Error('请选择画风提取结果 JSON，或本工具导出的批改记录。');
  for (const item of doc.items) {
    if (!item || typeof item.id !== 'string' || typeof item.positive_prompt !== 'string' || typeof item.artist_string !== 'string' || !['ok', 'none', 'error'].includes(item.status)) throw Error('结果中有不完整的条目，无法批改。');
  }
  return doc;
}
export function cleanReviews(items, input = {}) {
  return Object.fromEntries(items.flatMap((item, index) => {
    const r = input?.[index];
    if (!r || item.status === 'error') return [];
    return [[index, { decision: decisions.includes(r.decision) ? r.decision : '', note: typeof r.note === 'string' ? r.note : '', correction: typeof r.correction === 'string' ? r.correction : null, updatedAt: typeof r.updatedAt === 'string' ? r.updatedAt : '' }]];
  }));
}
export function statistics(items, reviews) {
  const result = { total: items.length, eligible: 0, errors: 0, correct: 0, wrong: 0, unsure: 0, pending: 0, judged: 0, accuracy: null };
  items.forEach((item, i) => {
    if (item.status === 'error') { result.errors++; return; }
    result.eligible++;
    const decision = reviews[i]?.decision;
    if (decisions.includes(decision)) result[decision]++; else result.pending++;
  });
  result.judged = result.correct + result.wrong;
  result.accuracy = result.judged ? result.correct / result.judged : null;
  return result;
}
export function visibleIndices(items, reviews, filter, search) {
  const needle = search.toLocaleLowerCase();
  return items.flatMap((item, i) => {
    const state = item.status === 'error' ? 'error' : reviews[i]?.decision || 'pending';
    if (filter !== 'all' && state !== filter) return [];
    if (needle && !(item.positive_prompt + '\n' + item.artist_string + '\n' + (reviews[i]?.note || '')).toLocaleLowerCase().includes(needle)) return [];
    return [i];
  });
}
