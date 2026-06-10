import { open } from "@tauri-apps/plugin-dialog";

import {
  getAppSnapshot,
  importWorkbook,
  initializeDataDirectory,
  openDataDirectory,
  type AppSnapshot,
} from "./api";
import { VirtualTable } from "./virtual-table";
import "./styles.css";

const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
  throw new Error("Missing #app root element");
}
const appRoot: HTMLDivElement = app;

let snapshot: AppSnapshot | null = null;
let busy = false;
let notice: { tone: "error" | "success"; text: string } | null = null;
let virtualTable: VirtualTable | null = null;

void refresh();

async function refresh(): Promise<void> {
  busy = true;
  render();
  try {
    snapshot = await getAppSnapshot();
  } catch (error) {
    notice = { tone: "error", text: errorText(error) };
  } finally {
    busy = false;
    render();
  }
}

function render(): void {
  virtualTable?.dispose();
  virtualTable = null;
  appRoot.innerHTML = `
    <main class="app-shell">
      ${renderHeader()}
      ${renderNotice()}
      ${renderContent()}
    </main>
  `;
  bindActions();
  mountVirtualTable();
}

function renderHeader(): string {
  const status = busy
    ? "正在处理"
    : snapshot?.dataDirectory
      ? "数据目录已连接"
      : "等待首次设置";
  return `
    <header class="topbar">
      <div>
        <p class="eyebrow">SMART SPREADSHEET</p>
        <h1>智能表格</h1>
      </div>
      <div class="status-pill" aria-label="应用状态">
        <span class="status-dot ${busy ? "is-busy" : ""}"></span>
        ${status}
      </div>
    </header>
  `;
}

function renderNotice(): string {
  if (!notice) {
    return "";
  }
  return `<div class="notice notice-${notice.tone}" role="status">${escapeHtml(notice.text)}</div>`;
}

function renderContent(): string {
  if (!snapshot) {
    return `
      <section class="loading-state" aria-live="polite">
        <div class="loading-bar"></div>
        <p>正在读取应用状态…</p>
      </section>
    `;
  }
  if (snapshot.startupError) {
    return `
      <section class="fatal-state">
        <span class="step-label">启动错误</span>
        <h2>无法打开已配置的数据目录</h2>
        <p>${escapeHtml(snapshot.startupError)}</p>
        <p class="implementation-note">定位文件未被自动覆盖，避免误切换到另一份工作区。</p>
      </section>
    `;
  }
  if (!snapshot.dataDirectory) {
    return renderSetup();
  }
  return renderWorkspace(snapshot);
}

function renderSetup(): string {
  return `
    <section class="workspace" aria-labelledby="setup-title">
      <div class="workspace-copy">
        <span class="step-label">首次设置</span>
        <h2 id="setup-title">选择一个数据目录开始</h2>
        <p>
          数据库、工作簿副本和图片缓存会统一保存在这里。之后更改目录时，应用会完整迁移数据，而不是只更换路径。
        </p>
        <div class="actions">
          <button id="initialize-directory" class="primary-action" type="button" ${disabled()}>
            初始化数据目录
          </button>
          <button id="open-directory" class="secondary-action" type="button" ${disabled()}>
            打开已有目录
          </button>
        </div>
        <p class="implementation-note">初始化只接受空文件夹或已由智能表格管理的目录。</p>
      </div>
      ${renderPrinciples()}
    </section>
  `;
}

function renderWorkspace(state: AppSnapshot): string {
  const directory = escapeHtml(state.dataDirectory ?? "");
  const workbook = state.workbook;
  return `
    <section class="configured-workspace" aria-labelledby="workspace-title">
      <div class="workspace-heading">
        <div>
          <span class="step-label">工作区</span>
          <h2 id="workspace-title">${workbook ? "数据已准备好" : "导入第一份工作簿"}</h2>
          <p class="directory-path" title="${directory}">${directory}</p>
        </div>
        <button id="import-workbook" class="primary-action compact-action" type="button" ${disabled()}>
          ${workbook ? "替换工作簿" : "选择 Excel"}
        </button>
      </div>
      ${workbook ? renderWorkbookSummary(workbook) : renderEmptyWorkbook()}
      ${workbook ? renderTableSection(workbook.rowCount) : ""}
      <p class="implementation-note">数据目录已经锁定；更换位置必须通过后续的“迁移数据目录”功能。</p>
    </section>
  `;
}

function renderTableSection(rowCount: number): string {
  return `
    <section class="table-section" aria-labelledby="table-title">
      <div class="table-titlebar">
        <div>
          <span class="step-label">数据浏览</span>
          <h3 id="table-title">工作簿记录</h3>
        </div>
        <span>${rowCount.toLocaleString("zh-CN")} 行</span>
      </div>
      <div id="virtual-table-root" class="virtual-table-root"></div>
      <div id="row-detail-host"></div>
    </section>
  `;
}

function renderWorkbookSummary(workbook: NonNullable<AppSnapshot["workbook"]>): string {
  return `
    <div class="summary-grid">
      <article class="summary-card summary-card-wide">
        <span>工作簿</span>
        <strong>${escapeHtml(workbook.importedName)}</strong>
        <small>${escapeHtml(workbook.sheetName)}</small>
      </article>
      <article class="summary-card">
        <span>数据行</span>
        <strong>${workbook.rowCount.toLocaleString("zh-CN")}</strong>
        <small>按源 Excel 行号持久化</small>
      </article>
      <article class="summary-card">
        <span>导入时间</span>
        <strong class="summary-date">${formatDate(workbook.importedAt)}</strong>
        <small>Tag 尚未写回工作簿副本</small>
      </article>
    </div>
  `;
}

function renderEmptyWorkbook(): string {
  return `
    <div class="drop-panel">
      <div class="file-mark">XLSX</div>
      <div>
        <h3>固定 NovelAI Metadata 结构</h3>
        <p>应用会复制并校验工作簿，原文件不会被移动或修改。</p>
      </div>
    </div>
  `;
}

function renderPrinciples(): string {
  return `
    <aside class="principles" aria-label="数据规则">
      <h3>数据规则</h3>
      <dl>
        <div><dt>原 Excel</dt><dd>始终只读，不修改、不覆盖</dd></div>
        <div><dt>Tag</dt><dd>区分大小写，支持批量编辑</dd></div>
        <div><dt>导出</dt><dd>生成新文件，Tags 写入最后一列</dd></div>
      </dl>
    </aside>
  `;
}

function bindActions(): void {
  document
    .querySelector<HTMLButtonElement>("#initialize-directory")
    ?.addEventListener("click", () => void chooseDirectory("initialize"));
  document
    .querySelector<HTMLButtonElement>("#open-directory")
    ?.addEventListener("click", () => void chooseDirectory("open"));
  document
    .querySelector<HTMLButtonElement>("#import-workbook")
    ?.addEventListener("click", () => void chooseWorkbook());
}

function mountVirtualTable(): void {
  const root = document.querySelector<HTMLElement>("#virtual-table-root");
  const detailHost = document.querySelector<HTMLElement>("#row-detail-host");
  if (root && detailHost) {
    virtualTable = new VirtualTable(root, detailHost);
  }
}

async function chooseDirectory(mode: "initialize" | "open"): Promise<void> {
  const selection = await open({
    directory: true,
    multiple: false,
    title: mode === "initialize" ? "选择空的数据目录" : "打开智能表格数据目录",
  });
  if (typeof selection !== "string") {
    return;
  }
  await runAction(async () => {
    snapshot =
      mode === "initialize"
        ? await initializeDataDirectory(selection)
        : await openDataDirectory(selection);
    notice = { tone: "success", text: "数据目录已连接。" };
  });
}

async function chooseWorkbook(): Promise<void> {
  if (snapshot?.workbook) {
    const confirmed = window.confirm(
      "替换当前工作簿会清除现有行与 Tag 数据。原 Excel 不会被修改。是否继续？",
    );
    if (!confirmed) {
      return;
    }
  }
  const selection = await open({
    multiple: false,
    directory: false,
    title: "选择 NovelAI Metadata 工作簿",
    filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }],
  });
  if (typeof selection !== "string") {
    return;
  }
  await runAction(async () => {
    const result = await importWorkbook(selection);
    snapshot = result.snapshot;
    notice = {
      tone: "success",
      text: `已导入 ${result.importedRows.toLocaleString("zh-CN")} 行，识别 ${result.embeddedImages.toLocaleString("zh-CN")} 张嵌入图片。`,
    };
  });
}

async function runAction(action: () => Promise<void>): Promise<void> {
  busy = true;
  notice = null;
  render();
  try {
    await action();
  } catch (error) {
    notice = { tone: "error", text: errorText(error) };
  } finally {
    busy = false;
    render();
  }
}

function disabled(): string {
  return busy ? "disabled" : "";
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>'"]/g, (character) => {
    const entities: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      "'": "&#39;",
      '"': "&quot;",
    };
    return entities[character] ?? character;
  });
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? escapeHtml(value)
    : new Intl.DateTimeFormat("zh-CN", {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(date);
}
