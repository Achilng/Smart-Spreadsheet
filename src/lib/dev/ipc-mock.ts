/**
 * 开发专用的 Tauri IPC 模拟（`?mock=1` 查询参数激活，仅 DEV 构建引用）。
 * 用于 Playwright/浏览器冒烟：不启动 Tauri 也能渲染对比窗口并核对
 * 分区、空态、分页与 set-sample 事件切换。生产构建会把它摇树剔除。
 */

interface MockRow {
  id: number;
  artists?: string;
  positivePrompt?: string | null;
  characterPrompt?: string | null;
  generationModel?: string | null;
  imagePath?: string;
  imageWidth?: number;
  imageHeight?: number;
  vibeReferenceCount?: number;
  time?: string;
  tags?: string[];
}

function rowDto(row: MockRow) {
  return {
    id: row.id,
    batchId: 1,
    sourceOrdinal: row.id,
    time: row.time ?? "2026-08-01 12:00",
    positivePrompt: row.positivePrompt ?? null,
    characterPrompt: row.characterPrompt ?? null,
    negativePrompt: "lowres, worst quality",
    note: null,
    artists: row.artists ?? null,
    imageFolder: null,
    imagePath: row.imagePath ?? `D:\\mock\\image${row.id}.png`,
    storedImagePath: null,
    imageWidth: row.imageWidth ?? 832,
    imageHeight: row.imageHeight ?? 1216,
    generationModel: row.generationModel ?? null,
    generationSampler: "k_euler_ancestral",
    generationSteps: 28,
    generationSeed: String(1000 + row.id),
    generationScale: "5",
    generationCfgRescale: "0.18",
    generationNoiseSchedule: "karras",
    metadataFailed: false,
    vibeReferenceCount: row.vibeReferenceCount ?? 0,
    groupId: null,
    groupName: null,
    tags: row.tags ?? [],
  };
}

const ARTIST_ROWS: MockRow[] = Array.from({ length: 30 }, (_, index) => ({
  id: 101 + index,
  artists: "artist:alpha",
  positivePrompt: `artist:alpha, hair style ${index}`,
}));

const VIBE_ROWS: MockRow[] = [
  {
    id: 201,
    positivePrompt: "artist:beta, night city",
    characterPrompt: "1girl, silver hair\ngreen eyes",
    vibeReferenceCount: 3,
  },
  { id: 202, positivePrompt: "artist:gamma, sunset beach", vibeReferenceCount: 3 },
];

const STYLE_ROWS: MockRow[] = [
  { id: 301, positivePrompt: "artist:delta, blue hair, school uniform" },
  { id: 302, positivePrompt: "artist:alpha, blue hair, school uniform" },
];

const MODEL_ROWS: MockRow[] = [
  { id: 401, generationModel: "NovelAI Diffusion V4 Full", positivePrompt: "same prompt" },
  { id: 402, generationModel: "NovelAI Diffusion V4.5 Curated", positivePrompt: "same prompt" },
  { id: 403, generationModel: "NovelAI Diffusion V3", positivePrompt: "same prompt" },
  { id: 404, generationModel: null, positivePrompt: "same prompt" },
  // 与样本同档位：后端会返回，但模型分区必须过滤且不得计入标题数量。
  { id: 405, generationModel: "NovelAI Diffusion V4.5 Full", positivePrompt: "same prompt" },
];

const SAME_MODEL_ROWS: MockRow[] = Array.from({ length: 7 }, (_, index) => ({
  id: 501 + index,
  generationModel: "NovelAI Diffusion V4.5 Full",
  positivePrompt: "same prompt",
}));

type SectionRow = MockRow[];

const SECTIONS: Record<string, SectionRow> = {
  sameArtists: ARTIST_ROWS,
  vibeDiffStyle: VIBE_ROWS,
  styleDiffVibe: STYLE_ROWS,
};

const PAGE_SIZE_DEFAULT = 24;

function sectionPage(rows: SectionRow, offset: number, limit: number) {
  const page = rows.slice(offset, offset + limit);
  return {
    rows: page.map(rowDto),
    totalCount: rows.length,
    offset,
    limit,
  };
}

/** 1×1 透明 PNG，让缩略图/大图管线在浏览器里有真实字节可解码。 */
function tinyPng(): ArrayBuffer {
  const bytes = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
    0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00,
    0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
  ];
  return new Uint8Array(bytes).buffer;
}

/** Browser-only generated scenery, so visual QA can inspect actual image framing. */
async function previewPng(rowId: number): Promise<ArrayBuffer> {
  const canvas = document.createElement("canvas");
  canvas.width = 320;
  canvas.height = 400;
  const context = canvas.getContext("2d");
  if (!context) return tinyPng();
  const palettes = [["#dedfcf", "#81969e", "#4d6d78"], ["#f5dcbf", "#c19487", "#816f80"], ["#d6e8df", "#83a5a1", "#496f78"]];
  const colors = palettes[rowId % palettes.length];
  const gradient = context.createLinearGradient(0, 0, 0, 400);
  gradient.addColorStop(0, colors[0]); gradient.addColorStop(1, colors[1]);
  context.fillStyle = gradient; context.fillRect(0, 0, 320, 400);
  context.fillStyle = "#fff9e5"; context.beginPath(); context.arc(220, 93, 30, 0, Math.PI * 2); context.fill();
  for (let layer = 0; layer < 3; layer++) {
    context.fillStyle = colors[2]; context.globalAlpha = .2 + layer * .2;
    context.beginPath(); context.moveTo(0, 215 + layer * 50);
    context.bezierCurveTo(80, 110 + layer * 60, 160, 320 + layer * 10, 320, 180 + layer * 50);
    context.lineTo(320, 400); context.lineTo(0, 400); context.closePath(); context.fill();
  }
  context.globalAlpha = .7; context.fillStyle = "white"; context.font = "11px sans-serif"; context.fillText("PREVIEW  /  " + rowId, 20, 375);
  const blob = await new Promise<Blob | null>(resolve => canvas.toBlob(resolve, "image/png"));
  return blob ? blob.arrayBuffer() : tinyPng();
}

const eventListeners = new Map<number, { event: string; handler: (payload: unknown) => void }>();
let callbackCounter = 0;

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    /** 冒烟测试辅助：模拟后端向本窗口推送事件。 */
    __mockEmit?: (event: string, payload: unknown) => void;
    __mockCalls?: { command: string; payload: Record<string, unknown> }[];
    __mockDelayMs?: number;
    __mockFailNext?: string;
  }
}

export function installIpcMock(): void {
  eventListeners.clear();
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_event: string, id: number) => { eventListeners.delete(id); } };
  const params = new URLSearchParams(location.search);
  const windowLabel = params.get("window") ?? "main";
  const libraryRows = Array.from({ length: params.has("large") ? 60_000 : 64 }, (_, index) => rowDto({
    id: index + 1,
    imagePath: `D:\\mock\\${index % 7 === 0 ? "清晨的山谷与远处的群山_长文件名显示检查_" : "风景_"}${String(index + 1).padStart(3, "0")}.png`,
    artists: index % 2 === 0 ? "artist:alpha" : "artist:beta",
    positivePrompt: `masterpiece, scenery, mountains, soft light, ${index % 2 ? "sunset" : "morning"}, artist:${index % 2 ? "beta" : "alpha"}`,
    generationModel: "NovelAI Diffusion V4.5 Full",
    imageWidth: index % 3 === 0 ? 1216 : 832,
    imageHeight: index % 3 === 0 ? 832 : 1216,
    tags: index % 4 === 0 ? ["收藏", "风景", "柔和光线"] : index % 3 === 0 ? [] : ["风景"],
    vibeReferenceCount: index % 4 === 0 ? 2 : 0,
  }));
  const snapshot = {
    dataDirectory: params.has("setup") ? null : "D:\\mock",
    rejectedImagesDirectory: "D:\\mock\\rejected",
    library: { rowCount: libraryRows.length, batchCount: 1, lastBatch: null },
    autoArtistPrefixOnImport: false,
    startupError: null,
  };
  window.__mockCalls = [];
  window.__mockDelayMs = Number(new URLSearchParams(location.search).get("mockDelay") ?? 0);
  const internals = {
    metadata: { currentWindow: { label: windowLabel }, currentWebview: { label: windowLabel } },
    transformCallback(callback: (payload: unknown) => void): number {
      callbackCounter += 1;
      const id = callbackCounter;
      eventListeners.set(id, { event: "", handler: callback });
      // listen 命令随后会携带 event 名重新登记；这里先占位。
      void callback;
      return id;
    },
    async invoke(command: string, args: Record<string, unknown> | undefined): Promise<unknown> {
      const payload = args ?? {};
      window.__mockCalls?.push({ command, payload });
      if (!command.startsWith("plugin:")) {
        if (window.__mockDelayMs) await new Promise(resolve => setTimeout(resolve, window.__mockDelayMs));
        if (window.__mockFailNext === command) {
          window.__mockFailNext = undefined;
          throw new Error("模拟查询失败，请重试");
        }
      }
      switch (command) {
        case "get_app_snapshot": return snapshot;
        case "set_auto_artist_prefix_on_import":
          snapshot.autoArtistPrefixOnImport = Boolean(payload.enabled);
          return { ...snapshot };
        case "query_rows": {
          const query = payload.query as { offset: number; limit: number; search?: string; tags?: string[]; tagMode?: string; untaggedOnly?: boolean };
          const search = (query.search ?? "").toLowerCase();
          let rows = libraryRows.filter(row => (!search || `${row.imagePath} ${row.positivePrompt} ${row.artists}`.toLowerCase().includes(search))
            && (!query.untaggedOnly || row.tags.length === 0)
            && (!query.tags?.length || (query.tagMode === "or" ? query.tags.some(tag => row.tags.includes(tag)) : query.tags.every(tag => row.tags.includes(tag)))));
          if (payload.sort === "timeDesc") rows = [...rows].reverse();
          return { rows: rows.slice(query.offset, query.offset + query.limit), totalCount: rows.length, offset: query.offset, limit: query.limit, hasMore: query.offset + query.limit < rows.length };
        }
        case "get_rows_by_ids": return libraryRows.filter(row => (payload.rowIds as number[]).includes(row.id));
        case "get_row_index": return libraryRows.findIndex(row => row.id === payload.rowId);
        case "list_tags": return ["收藏", "风景", "柔和光线"].map(name => ({ name, rowCount: libraryRows.filter(row => row.tags.includes(name)).length }));
        case "list_groups": return [];
        case "backfill_vibe_statuses":
        case "backfill_style_signatures": return { total: 0, processed: 0, updated: 0, unreadable: 0 };
        case "plugin:window|is_maximized": return false;
        case "plugin:window|is_focused": return true;
        case "plugin:event|listen": {
          const id = Number(payload.handler);
          const entry = eventListeners.get(id);
          if (entry) {
            entry.event = String(payload.event);
          }
          return id;
        }
        case "plugin:event|unlisten": {
          eventListeners.delete(Number(payload.eventId ?? -1));
          return null;
        }
        case "get_compare_sample": {
          // 按请求的 rowId 选样本：set-sample 事件切换后界面可见变化。
          const requested = Number(payload.rowId);
          const sample: MockRow = requested === 2
            ? { id: requested, positivePrompt: null, generationModel: null }
            : {
                id: requested,
                artists: "artist:alpha",
                positivePrompt: "artist:alpha, blue hair, school uniform, masterpiece",
                characterPrompt: "1girl, silver hair\nblue eyes",
                generationModel: "NovelAI Diffusion V4.5 Full",
                vibeReferenceCount: 3,
                tags: ["样本"],
              };
          const flags = requested === 2
            ? { hasStyleSignature: false, hasVibeSignature: false, vibeSignatureUnreadable: false }
            : { hasStyleSignature: true, hasVibeSignature: true, vibeSignatureUnreadable: false };
          return { row: rowDto(sample), ...flags };
        }
        case "query_compare_same_artists":
        case "query_compare_same_vibe_diff_style":
        case "query_compare_same_style_diff_vibe": {
          // 样本 2（空态样本）下分区如实返回空。
          if (Number(payload.rowId) === 2) {
            return sectionPage([], 0, Number(payload.limit));
          }
          if (new URLSearchParams(location.search).has("large")) {
            const offset = Number(payload.offset);
            const limit = Number(payload.limit);
            return { rows: Array.from({ length: Math.min(limit, 60_000 - offset) }, (_, index) => rowDto({ id: 101 + offset + index, artists: "artist:alpha", positivePrompt: "artist:alpha, blue hair, sunlight", generationModel: "NovelAI Diffusion V4.5 Full" })), totalCount: 60_000, offset, limit };
          }
          const rows = command === "query_compare_same_artists"
            ? SECTIONS.sameArtists
            : command === "query_compare_same_vibe_diff_style"
              ? SECTIONS.vibeDiffStyle
              : SECTIONS.styleDiffVibe;
          return sectionPage(rows, Number(payload.offset), Number(payload.limit));
        }
        case "query_compare_same_style_all_models":
          if (Number(payload.rowId) === 2) {
            return { rows: [], totalCount: 0, truncated: false };
          }
          if (Number(payload.rowId) === 3) {
            return {
              rows: SAME_MODEL_ROWS.map(rowDto),
              totalCount: SAME_MODEL_ROWS.length,
              truncated: false,
            };
          }
          if (new URLSearchParams(location.search).has("large")) {
            return { rows: Array.from({ length: 500 }, (_, index) => rowDto({ id: 1001 + index, generationModel: index % 2 ? "NovelAI Diffusion V4 Full" : "NovelAI Diffusion V3" })), totalCount: 60_000, truncated: true };
          }
          return {
            rows: MODEL_ROWS.map(rowDto),
            totalCount: MODEL_ROWS.length,
            truncated: false,
          };
        case "get_row_thumbnail":
        case "get_row_gallery_preview":
        case "get_row_preview":
        case "get_row_original":
          return previewPng(Number(payload.rowId));
        case "get_row_vibe_status":
          return 2;
        case "plugin:window|destroy":
          window.close();
          return null;
        default:
          // 未显式模拟的命令一律回空对象，避免冒烟时无关路径报错。
          return {};
      }
    },
  };
  window.__TAURI_INTERNALS__ = internals;
  window.__mockEmit = (event: string, payload: unknown) => {
    // 真实后端调用回调时传完整事件信封，@tauri-apps/api 再从中取 payload。
    for (const [id, entry] of eventListeners) {
      if (entry.event === event) {
        entry.handler({ event, id, payload });
      }
    }
  };
  // ?switchTo=<id>：装载 800ms 后模拟后端推送 set-sample（复用窗口切换样本路径）。
  const switchTo = new URLSearchParams(window.location.search).get("switchTo");
  if (switchTo) {
    window.setTimeout(() => {
      window.__mockEmit?.("compare://set-sample", Number(switchTo));
    }, 800);
  }
  void PAGE_SIZE_DEFAULT;
}
