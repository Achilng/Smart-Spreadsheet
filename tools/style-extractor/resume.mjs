import fs from 'node:fs';
import path from 'node:path';
import { atomicJson } from './core.mjs';

// Persist intent separately from job state: an explicit pause must never auto-resume.
export class ResumeController {
  constructor(manager, enabled) {
    this.manager = manager; this.enabled = enabled; this.stopping = false; this.running = null;
    this.file = path.join(manager.directory, '.resume.json');
  }
  clear() { if (this.enabled) atomicJson(this.file, { id: null }); }
  start(id, retry, credentials) {
    if (this.manager.active) throw Error('已有任务正在运行，请先暂停。');
    if (this.stopping) throw Error('服务正在重启，请稍后重试。');
    const job = this.manager.get(id);
    if (this.enabled) atomicJson(this.file, { id });
    this.running = this.manager.start(id, retry, credentials).catch(error => this.manager.log(job, error.message))
      .finally(() => { if (!this.stopping) this.clear(); this.running = null; });
    return this.running;
  }
  pause(id) { if (this.manager.active?.id === id) this.clear(); this.manager.pause(id); }
  restore(credentials) {
    if (!this.enabled || !fs.existsSync(this.file)) return;
    const { id } = JSON.parse(fs.readFileSync(this.file, 'utf8'));
    if (id) return this.start(id, false, credentials);
  }
  async stop() {
    this.stopping = true;
    if (this.manager.active) this.manager.pause(this.manager.active.id);
    await this.running;
  }
}
