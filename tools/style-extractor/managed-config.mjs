import fs from 'node:fs';
import { normalizeChannels } from './channels.mjs';

// Operator-owned file, outside the public code and task directories. Never send
// its contents to browsers; reload only when a task is created or started.
export function readManagedConfig(filename) {
  try {
    const data = JSON.parse(fs.readFileSync(filename, 'utf8'));
    const channels = normalizeChannels(data.channels, true);
    const batchSize = Number(data.batchSize ?? 10), effort = data.effort ?? 'high';
    if (!channels.some(x => x.enabled) || !Number.isInteger(batchSize) || batchSize < 1 || batchSize > 100 || !['low', 'medium', 'high'].includes(effort)) throw Error();
    return { provider: 'api', channels, batchSize, effort };
  } catch { throw Error('服务配置暂不可用，请联系管理员。'); }
}
