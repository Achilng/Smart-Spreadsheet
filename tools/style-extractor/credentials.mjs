import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { atomicJson, normalizeBaseUrl } from './core.mjs';

// Secrets travel over stdin/stdout, never command arguments or temporary files.
function protect(value, decrypt = false) {
  if (process.platform !== 'win32') return Promise.reject(Error('本机加密保存目前仅支持 Windows。'));
  const operation = decrypt
    ? '[Text.Encoding]::UTF8.GetString([Security.Cryptography.ProtectedData]::Unprotect([Convert]::FromBase64String($value),$null,[Security.Cryptography.DataProtectionScope]::CurrentUser))'
    : '[Convert]::ToBase64String([Security.Cryptography.ProtectedData]::Protect([Text.Encoding]::UTF8.GetBytes($value),$null,[Security.Cryptography.DataProtectionScope]::CurrentUser))';
  const code = "$ErrorActionPreference='Stop'; [Console]::InputEncoding=[Text.UTF8Encoding]::new(); [Console]::OutputEncoding=[Text.UTF8Encoding]::new(); Add-Type -AssemblyName System.Security; $value=[Console]::In.ReadToEnd(); [Console]::Out.Write(" + operation + ')';
  return new Promise((resolve, reject) => {
    const child = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', code], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    let output = ''; const timer = setTimeout(() => child.kill(), 10000);
    child.stdout.on('data', data => output += data.toString('utf8'));
    child.stderr.on('data', () => {});
    child.stdin.on('error', () => {});
    child.on('error', () => { clearTimeout(timer); reject(Error('无法使用 Windows 本机加密服务。')); });
    child.on('close', exit => { clearTimeout(timer); exit === 0 ? resolve(output) : reject(Error(decrypt ? '无法解密保存的 Key，请在当前 Windows 账户下重新保存。' : '保存 Key 时加密失败。')); });
    child.stdin.end(value, 'utf8');
  });
}
export class CredentialStore {
  constructor(directory) { this.filename = path.join(directory, '.credentials.json'); this.pending = Promise.resolve(); }
  run(operation) { const next = this.pending.then(operation); this.pending = next.catch(() => {}); return next; }
  read() {
    if (!fs.existsSync(this.filename)) return { version: 1, entries: {} };
    const value = JSON.parse(fs.readFileSync(this.filename, 'utf8'));
    if (value.version !== 1 || !value.entries || typeof value.entries !== 'object') throw Error('本机凭据文件格式异常');
    return value;
  }
  get(baseUrl) { return this.run(() => this.load(baseUrl)); }
  async load(baseUrl) {
    const entry = this.read().entries[normalizeBaseUrl(baseUrl)];
    return entry ? protect(entry, true) : '';
  }
  save(baseUrl, apiKey) { return this.run(async () => {
    const base = normalizeBaseUrl(baseUrl), key = typeof apiKey === 'string' ? apiKey.trim() : '';
    if (!key || key.length > 4096 || /[\r\n]/.test(key)) throw Error('请填写有效的 API Key');
    // Avoid repeated encryption and disk writes on every batch / restart click.
    if (await this.load(base).catch(() => '') === key) return;
    const encrypted = await protect(key);
    const data = this.read(); data.entries[base] = encrypted; atomicJson(this.filename, data);
  }); }
  forget(baseUrl) { return this.run(() => { const data = this.read(); delete data.entries[normalizeBaseUrl(baseUrl)]; atomicJson(this.filename, data); }); }
}
