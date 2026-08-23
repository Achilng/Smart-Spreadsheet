/**
 * 开发专用的 Tauri IPC 模拟（`?mock=1` 查询参数激活，仅 DEV 构建引用）。
 * 用于 Playwright/浏览器冒烟：不启动 Tauri 也能渲染对比窗口并核对
 * 分区、空态、分页与 set-sample 事件切换。生产构建会把它摇树剔除。
 */

interface MockRow {
  id: number;
  artists?: string;
  positivePrompt?: string | null;
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
    characterPrompt: null,
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
  { id: 201, positivePrompt: "artist:beta, night city", vibeReferenceCount: 3 },
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

const eventListeners = new Map<number, { event: string; handler: (payload: unknown) => void }>();
let callbackCounter = 0;

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    /** 冒烟测试辅助：模拟后端向本窗口推送事件。 */
    __mockEmit?: (event: string, payload: unknown) => void;
  }
}

export function installIpcMock(): void {
  const internals = {
    metadata: { currentWindow: { label: "compare" }, currentWebview: { label: "compare" } },
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
      switch (command) {
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
          return {
            rows: MODEL_ROWS.map(rowDto),
            totalCount: MODEL_ROWS.length,
            truncated: false,
          };
        case "get_row_thumbnail":
        case "get_row_gallery_preview":
        case "get_row_preview":
        case "get_row_original":
          return tinyPng();
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
