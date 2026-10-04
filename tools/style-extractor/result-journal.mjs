import fs from 'node:fs';

// Each acknowledged result is durable before it is shown. Checkpoints replace
// the full state first, then clear this idempotent upsert/delete journal.
export function appendResult(filename, id, result) {
  let fd;
  try {
    fd = fs.openSync(filename, 'a');
    const line = Buffer.from(JSON.stringify({ id, result }) + '\n');
    let written = 0;
    while (written < line.length) written += fs.writeSync(fd, line, written, line.length - written);
    fs.fsyncSync(fd);
  } catch (error) {
    error.storageFailure = true; throw error;
  } finally { if (fd !== undefined) fs.closeSync(fd); }
}

export function replayResults(filename, results) {
  if (!fs.existsSync(filename)) return null;
  const text = fs.readFileSync(filename, 'utf8');
  if (!text) return null;
  const values = new Map(results.map(result => [result.id, result]));
  // A killed writer may leave one incomplete final line; earlier fsynced lines
  // remain valid. A corrupt complete record is never silently skipped.
  for (const line of text.slice(0, text.lastIndexOf('\n') + 1).split('\n')) {
    if (!line) continue;
    const { id, result } = JSON.parse(line);
    if (!/^sha256:[a-f0-9]{64}$/.test(id) || (result !== null && (!result || result.id !== id || !['ok', 'none', 'error'].includes(result.status)))) throw Error('结果增量记录损坏');
    if (result === null) values.delete(id); else values.set(id, result);
  }
  return [...values.values()];
}

export function clearResults(filename) {
  if (!fs.existsSync(filename) || !fs.statSync(filename).size) return;
  const fd = fs.openSync(filename, 'w');
  try { fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
}
