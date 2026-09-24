import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { CredentialStore } from './credentials.mjs';
test('Windows credentials are encrypted, scoped by endpoint and survive restart until forgotten', { skip: process.platform !== 'win32' }, async () => {
  const root = 'D:/Agent/Agent_temp/style-extractor-credentials-tests'; fs.mkdirSync(root, { recursive: true });
  const dir = fs.mkdtempSync(root + '/run-'), store = new CredentialStore(dir), secret = 'test-secret-for-dpapi-only';
  await store.save('https://example.com/v1/', secret);
  assert.equal(fs.readFileSync(store.filename, 'utf8').includes(secret), false);
  const restored = new CredentialStore(dir);
  assert.equal(await restored.get('https://example.com/v1'), secret);
  assert.equal(await restored.get('https://different.example/v1'), '');
  const resave = store.save('https://example.com/v1', 'replacement-secret');
  const forget = store.forget('https://example.com/v1');
  await Promise.all([resave, forget]); assert.equal(await restored.get('https://example.com/v1'), '');
});
