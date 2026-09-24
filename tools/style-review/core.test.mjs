import test from 'node:test';
import assert from 'node:assert/strict';
import { RESULT, validateResult, cleanReviews, statistics, visibleIndices } from './core.mjs';
const items = ['ok', 'none', 'ok', 'ok', 'error'].map((status, i) => ({ id: String(i), status, positive_prompt: 'artist:' + i, artist_string: status === 'ok' ? 'artist:' + i : '' }));
test('accuracy excludes unsure, pending and failed calls, including when restored records label failures', () => {
  const reviews = cleanReviews(items, { 0: { decision: 'correct' }, 1: { decision: 'wrong' }, 2: { decision: 'unsure' }, 4: { decision: 'correct' } });
  const s = statistics(items, reviews);
  assert.deepEqual(s, { total: 5, eligible: 4, errors: 1, correct: 1, wrong: 1, unsure: 1, pending: 1, judged: 2, accuracy: .5 });
  assert.equal(statistics(items, {}).accuracy, null);
});
test('filters retain original indices and preserve empty reference answers', () => {
  const reviews = cleanReviews(items, { 1: { decision: 'wrong', correction: '', note: '漏提' }, 2: { decision: 'invented' } });
  assert.equal(reviews[1].correction, ''); assert.equal(reviews[2].correction, null);
  assert.deepEqual(visibleIndices(items, reviews, 'wrong', '漏提'), [1]);
  assert.deepEqual(visibleIndices(items, reviews, 'error', ''), [4]);
  assert.deepEqual(visibleIndices(items, reviews, 'pending', 'ARTIST:2'), [2]);
  assert.equal(validateResult({ format: RESULT, version: 1, items }).items.length, 5);
  assert.throws(() => validateResult({ items }), /结果 JSON/);
});
