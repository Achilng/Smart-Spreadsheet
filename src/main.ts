import "./styles.css";

const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
  throw new Error("Missing #app root element");
}

app.innerHTML = `
  <main class="app-shell">
    <header class="topbar">
      <div>
        <p class="eyebrow">SMART SPREADSHEET</p>
        <h1>智能表格</h1>
      </div>
      <div class="status-pill" aria-label="应用状态">
        <span class="status-dot"></span>
        核心服务已就绪
      </div>
    </header>

    <section class="workspace" aria-labelledby="setup-title">
      <div class="workspace-copy">
        <span class="step-label">首次设置</span>
        <h2 id="setup-title">选择一个数据目录开始</h2>
        <p>
          数据库、工作簿副本和图片缓存会统一保存在这里。之后更改目录时，应用会完整迁移数据。
        </p>
        <div class="actions">
          <button class="primary-action" type="button" disabled>选择数据目录</button>
          <button class="secondary-action" type="button" disabled>打开已有目录</button>
        </div>
        <p class="implementation-note">目录选择与后端命令将在下一小目标接通。</p>
      </div>

      <aside class="principles" aria-label="数据规则">
        <h3>数据规则</h3>
        <dl>
          <div>
            <dt>原 Excel</dt>
            <dd>始终只读，不修改、不覆盖</dd>
          </div>
          <div>
            <dt>Tag</dt>
            <dd>区分大小写，支持批量编辑</dd>
          </div>
          <div>
            <dt>导出</dt>
            <dd>生成新文件，Tags 写入最后一列</dd>
          </div>
        </dl>
      </aside>
    </section>
  </main>
`;
