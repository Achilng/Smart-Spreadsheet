import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

// One ephemeral, read-only app-server thread per call, using saved Codex auth.
export function codexStream({ cli, cwd, model, effort, prompt, input, schema, signal, onText }) {
  return new Promise((resolve, reject) => {
    const child = spawn(cli.command, [...cli.prefix, 'app-server', '--listen', 'stdio://', '-c', 'mcp_servers={}', '-c', 'features.apps=false', '-c', 'web_search="disabled"'], { cwd, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    const lines = createInterface({ input: child.stdout });
    let settled = false, stopped = false, last = '', threadId, finalText = '', messageId, resolveValue, resolveError;
    const kill = () => { if (stopped) return; stopped = true; if (process.platform === 'win32' && child.pid) spawn('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' }); else child.kill('SIGTERM'); };
    const finish = (error, value) => { if (settled) return; settled = true; resolveError = error; resolveValue = value; clearTimeout(timeout); signal.removeEventListener('abort', abort); lines.close(); kill(); };
    const abort = () => finish(Error('已暂停'));
    const timeout = setTimeout(() => finish(Error('Codex 调用超过 10 分钟')), 600000);
    signal.addEventListener('abort', abort, { once: true });
    const send = value => { if (!settled) child.stdin.write(JSON.stringify(value) + '\n'); };
    child.stdin.on('error', error => finish(error));
    child.stderr.setEncoding('utf8'); child.stderr.on('data', text => { last = (last + text).slice(-3000); });
    child.on('error', error => { finish(error); reject(error); });
    child.on('close', code => {
      if (!settled) finish(Error(`Codex 流式进程提前退出（${code}）：${last.slice(-800)}`));
      resolveError ? reject(resolveError) : resolve(resolveValue);
    });
    lines.on('line', line => {
      if (settled) return;
      try {
        const m = JSON.parse(line), p = m.params;
        if (m.error) throw Error(m.error.message || JSON.stringify(m.error));
        if (m.id === 0) {
          send({ method: 'initialized', params: {} });
          send({ id: 1, method: 'thread/start', params: { model, modelProvider: 'openai', cwd, ephemeral: true, approvalPolicy: 'never', sandbox: 'read-only', baseInstructions: prompt, config: { 'model_reasoning_effort': effort } } });
        } else if (m.id === 1) {
          threadId = m.result.thread.id;
          send({ id: 2, method: 'turn/start', params: { threadId, model, effort, input: [{ type: 'text', text: input }], outputSchema: schema } });
        } else if (m.method && m.id !== undefined) {
          // No interactive approvals or tool requests belong in extraction.
          send({ id: m.id, error: { code: -32601, message: 'Text extraction does not support tool requests' } });
          throw Error('提取任务意外请求工具或授权，已停止');
        } else if (m.method === 'item/agentMessage/delta' && p.threadId === threadId) {
          if (messageId && messageId !== p.itemId) throw Error('Codex 返回多个消息，无法合并提取结果');
          messageId = p.itemId; finalText += p.delta; onText(p.delta);
        } else if (m.method === 'item/completed' && p.threadId === threadId && p.item?.type === 'agentMessage') {
          if (!finalText) { messageId = p.item.id; finalText = p.item.text; onText(finalText); }
          else if (p.item.id === messageId && p.item.text !== finalText) throw Error('Codex 最终消息与流式片段不一致');
        } else if (m.method === 'turn/completed' && p.threadId === threadId) {
          if (p.turn.status !== 'completed') throw Error(p.turn.error?.message || `Codex ${p.turn.status}`);
          if (!finalText) throw Error('Codex 没有返回文本');
          finish(null, finalText);
        } else if (m.method === 'error' && p?.willRetry === false) throw Error(p.error?.message || 'Codex 流式调用失败');
      } catch (error) { finish(error); }
    });
    if (signal.aborted) abort();
    else send({ id: 0, method: 'initialize', params: { clientInfo: { name: 'smart_spreadsheet_style_extractor', title: 'Style Extractor', version: '0.1.0' } } });
  });
}
