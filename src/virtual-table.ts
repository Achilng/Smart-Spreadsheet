import {
  getRowPreview,
  queryRows,
  type RowQuery,
  type RowRecord,
  type TagMatchMode,
} from "./api";
import { ThumbnailLoader, binaryBuffer } from "./image-loader";

const PAGE_SIZE = 200;
const ROW_HEIGHT = 116;
const OVERSCAN = 5;

type TableQuery = Pick<RowQuery, "tags" | "tagMode">;

export interface VirtualTablePageState {
  pageIndex: number;
  start: number;
  end: number;
  totalCount: number;
  rows: RowRecord[];
}

export interface VirtualTableOptions {
  query: TableQuery;
  isRowSelected: (rowId: number) => boolean;
  onRowSelectionChange: (rowId: number, selected: boolean) => void;
  onPageStateChange: (state: VirtualTablePageState) => void;
}

export class VirtualTable {
  readonly #root: HTMLElement;
  readonly #detailHost: HTMLElement;
  readonly #options: VirtualTableOptions;
  readonly #thumbnailLoader = new ThumbnailLoader();
  readonly #pages = new Map<number, RowRecord[]>();
  readonly #pendingPages = new Map<number, number>();
  #viewport: HTMLElement;
  #spacer: HTMLElement;
  #layer: HTMLElement;
  #query: TableQuery;
  #totalCount = 0;
  #generation = 0;
  #disposed = false;
  #renderFrame = 0;
  #lastPageSignature = "";
  #closeDetails: (() => void) | null = null;

  constructor(root: HTMLElement, detailHost: HTMLElement, options: VirtualTableOptions) {
    this.#root = root;
    this.#detailHost = detailHost;
    this.#options = options;
    this.#query = cloneQuery(options.query);
    this.#root.innerHTML = `
      <div class="sheet-header" aria-hidden="true">
        <span>选</span><span>行</span><span>图片</span><span>时间</span><span>正向提示词</span>
        <span>负向提示词</span><span>画师</span><span>Tags</span><span></span>
      </div>
      <div class="sheet-viewport" role="table" aria-label="工作簿数据">
        <div class="sheet-spacer"><div class="sheet-layer"></div></div>
      </div>
    `;
    this.#viewport = requiredElement(this.#root, ".sheet-viewport");
    this.#spacer = requiredElement(this.#root, ".sheet-spacer");
    this.#layer = requiredElement(this.#root, ".sheet-layer");
    this.#viewport.addEventListener("scroll", this.#scheduleRender, { passive: true });
    window.addEventListener("resize", this.#scheduleRender);
    this.setQuery(this.#query);
  }

  dispose(): void {
    this.#disposed = true;
    this.#generation += 1;
    this.#viewport.removeEventListener("scroll", this.#scheduleRender);
    window.removeEventListener("resize", this.#scheduleRender);
    window.cancelAnimationFrame(this.#renderFrame);
    this.#closeDetails?.();
    this.#thumbnailLoader.dispose();
    this.#detailHost.replaceChildren();
  }

  setQuery(query: TableQuery): void {
    this.#query = cloneQuery(query);
    this.#generation += 1;
    this.#pages.clear();
    this.#pendingPages.clear();
    this.#totalCount = 0;
    this.#lastPageSignature = "";
    this.#viewport.scrollTop = 0;
    this.#spacer.style.height = "0px";
    this.#layer.replaceChildren();
    this.#thumbnailLoader.retain(new Set());
    this.#showStatus("正在加载表格…");
    this.#emitPageState();
    void this.#loadPage(0, this.#generation);
  }

  reload(): void {
    this.setQuery(this.#query);
  }

  refreshSelection(): void {
    this.#renderVisibleRows();
  }

  getCurrentPageRows(): RowRecord[] {
    return [...(this.#pages.get(this.#currentPageIndex()) ?? [])];
  }

  getTotalCount(): number {
    return this.#totalCount;
  }

  readonly #scheduleRender = (): void => {
    window.cancelAnimationFrame(this.#renderFrame);
    this.#renderFrame = window.requestAnimationFrame(() => this.#renderVisibleRows());
  };

  async #loadPage(pageIndex: number, generation: number): Promise<void> {
    if (
      this.#disposed ||
      generation !== this.#generation ||
      this.#pages.has(pageIndex) ||
      this.#pendingPages.get(pageIndex) === generation
    ) {
      return;
    }
    this.#pendingPages.set(pageIndex, generation);
    try {
      const page = await queryRows({
        offset: pageIndex * PAGE_SIZE,
        limit: PAGE_SIZE,
        tags: [...this.#query.tags],
        tagMode: this.#query.tagMode,
      });
      if (this.#disposed || generation !== this.#generation) {
        return;
      }
      this.#pages.set(pageIndex, page.rows);
      this.#totalCount = page.totalCount;
      this.#spacer.style.height = `${this.#totalCount * ROW_HEIGHT}px`;
      if (this.#totalCount === 0) {
        this.#layer.replaceChildren();
        this.#showStatus("当前筛选没有匹配记录。", "is-empty");
      } else {
        this.#hideStatus();
        this.#renderVisibleRows();
      }
      this.#emitPageState();
    } catch (error) {
      if (!this.#disposed && generation === this.#generation) {
        this.#showStatus(`加载失败：${errorText(error)}`, "is-error");
      }
    } finally {
      if (this.#pendingPages.get(pageIndex) === generation) {
        this.#pendingPages.delete(pageIndex);
      }
    }
  }

  #renderVisibleRows(): void {
    if (this.#disposed || this.#totalCount === 0) {
      this.#emitPageState();
      return;
    }
    const visibleCount = Math.max(1, Math.ceil(this.#viewport.clientHeight / ROW_HEIGHT));
    const first = Math.max(0, Math.floor(this.#viewport.scrollTop / ROW_HEIGHT) - OVERSCAN);
    const last = Math.min(this.#totalCount, first + visibleCount + OVERSCAN * 2);
    const fragment = document.createDocumentFragment();
    const visibleRowIds = new Set<number>();

    for (let index = first; index < last; index += 1) {
      const pageIndex = Math.floor(index / PAGE_SIZE);
      const row = this.#pages.get(pageIndex)?.[index % PAGE_SIZE];
      if (row) {
        visibleRowIds.add(row.id);
      }
      fragment.append(row ? this.#createRow(row, index) : this.#createSkeleton(index));
      if (!row) {
        void this.#loadPage(pageIndex, this.#generation);
      }
    }
    this.#thumbnailLoader.retain(visibleRowIds);
    this.#layer.replaceChildren(fragment);

    const nextPage = Math.floor(last / PAGE_SIZE);
    if (nextPage * PAGE_SIZE < this.#totalCount) {
      void this.#loadPage(nextPage, this.#generation);
    }
    this.#emitPageState();
  }

  #createRow(row: RowRecord, index: number): HTMLElement {
    const element = document.createElement("div");
    const selected = this.#options.isRowSelected(row.id);
    element.className = `sheet-row${selected ? " is-selected" : ""}`;
    element.style.transform = `translateY(${index * ROW_HEIGHT}px)`;
    element.setAttribute("role", "row");

    const selection = document.createElement("div");
    selection.className = "sheet-cell selection-cell";
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = selected;
    checkbox.setAttribute("aria-label", `选择 Excel 第 ${row.sourceRow} 行`);
    checkbox.addEventListener("change", () => {
      this.#options.onRowSelectionChange(row.id, checkbox.checked);
      this.#renderVisibleRows();
    });
    selection.append(checkbox);

    element.append(
      selection,
      cell(String(row.sourceRow), "row-number"),
      this.#createImageCell(row),
      cell(row.time ?? "—", "time-cell"),
      cell(row.positivePrompt ?? "—", "prompt-cell"),
      cell(row.negativePrompt ?? "—", "prompt-cell muted-cell"),
      cell(row.artists ?? "—", "artists-cell"),
      tagsCell(row.tags),
    );

    const actions = document.createElement("div");
    actions.className = "sheet-cell row-actions";
    const details = document.createElement("button");
    details.type = "button";
    details.className = "text-action";
    details.textContent = "展开";
    details.addEventListener("click", () => this.#showDetails(row));
    actions.append(details);
    element.append(actions);
    return element;
  }

  #createSkeleton(index: number): HTMLElement {
    const element = document.createElement("div");
    element.className = "sheet-row sheet-row-skeleton";
    element.style.transform = `translateY(${index * ROW_HEIGHT}px)`;
    for (let column = 0; column < 9; column += 1) {
      const placeholder = document.createElement("span");
      placeholder.className = "skeleton-line";
      element.append(placeholder);
    }
    return element;
  }

  #createImageCell(row: RowRecord): HTMLElement {
    const element = document.createElement("div");
    element.className = "sheet-cell image-cell";
    const hasSource = Boolean(row.imagePath?.trim() || row.embeddedImageRef?.trim());
    const button = document.createElement("button");
    button.type = "button";
    button.className = "thumbnail-button";
    button.disabled = !hasSource;
    button.setAttribute("aria-label", `预览 Excel 第 ${row.sourceRow} 行图片`);
    const placeholder = document.createElement("span");
    placeholder.textContent = hasSource ? "加载中" : "缺失";
    button.append(placeholder);
    if (hasSource) {
      button.addEventListener("click", () => this.#showImagePreview(row));
      void this.#thumbnailLoader.load(row.id).then(
        url => {
          if (!button.isConnected) {
            return;
          }
          const image = document.createElement("img");
          image.src = url;
          image.alt = `Excel 第 ${row.sourceRow} 行缩略图`;
          button.replaceChildren(image);
          button.classList.add("is-loaded");
        },
        error => {
          if (!button.isConnected) {
            return;
          }
          placeholder.textContent = "不可用";
          button.title = errorText(error);
          button.classList.add("is-error");
        },
      );
    }
    element.append(button);
    return element;
  }

  #currentPageIndex(): number {
    if (this.#totalCount === 0) {
      return 0;
    }
    const maximum = Math.max(0, Math.ceil(this.#totalCount / PAGE_SIZE) - 1);
    return Math.min(maximum, Math.floor(this.#viewport.scrollTop / (PAGE_SIZE * ROW_HEIGHT)));
  }

  #emitPageState(): void {
    const pageIndex = this.#currentPageIndex();
    const rows = this.#pages.get(pageIndex) ?? [];
    const start = pageIndex * PAGE_SIZE;
    const end = Math.min(this.#totalCount, start + (rows.length || PAGE_SIZE));
    const signature = `${this.#generation}:${pageIndex}:${this.#totalCount}:${rows.length}`;
    if (signature === this.#lastPageSignature) {
      return;
    }
    this.#lastPageSignature = signature;
    this.#options.onPageStateChange({
      pageIndex,
      start,
      end,
      totalCount: this.#totalCount,
      rows: [...rows],
    });
  }

  #showStatus(message: string, modifier?: "is-empty" | "is-error"): void {
    let status = this.#viewport.querySelector<HTMLElement>(".table-loading");
    if (!status) {
      status = document.createElement("div");
      status.className = "table-loading";
      this.#viewport.append(status);
    }
    status.className = `table-loading${modifier ? ` ${modifier}` : ""}`;
    status.textContent = message;
  }

  #hideStatus(): void {
    this.#viewport.querySelector(".table-loading")?.remove();
  }

  #showDetails(row: RowRecord): void {
    this.#closeDetails?.();
    const backdrop = document.createElement("div");
    backdrop.className = "detail-backdrop";
    const panel = document.createElement("article");
    panel.className = "detail-panel";
    panel.setAttribute("role", "dialog");
    panel.setAttribute("aria-modal", "true");

    const header = document.createElement("header");
    const title = document.createElement("div");
    const eyebrow = document.createElement("span");
    eyebrow.textContent = `Excel 第 ${row.sourceRow} 行`;
    const heading = document.createElement("h3");
    heading.textContent = row.time ?? "未记录时间";
    title.append(eyebrow, heading);
    const close = document.createElement("button");
    close.type = "button";
    close.className = "detail-close";
    close.textContent = "关闭";
    header.append(title, close);
    panel.append(header);

    const content = document.createElement("div");
    content.className = "detail-content";
    content.append(
      detailSection("正向提示词", row.positivePrompt),
      detailSection("负向提示词", row.negativePrompt),
      detailSection("画师串", row.artists),
      detailSection("图片路径", row.imagePath),
    );
    panel.append(content);
    backdrop.append(panel);

    let closed = false;
    const onKeydown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        closeDetails();
      }
    };
    const closeDetails = (): void => {
      if (closed) {
        return;
      }
      closed = true;
      window.removeEventListener("keydown", onKeydown);
      this.#detailHost.replaceChildren();
      if (this.#closeDetails === closeDetails) {
        this.#closeDetails = null;
      }
    };
    close.addEventListener("click", closeDetails);
    backdrop.addEventListener("click", (event) => {
      if (event.target === backdrop) {
        closeDetails();
      }
    });
    window.addEventListener("keydown", onKeydown);
    this.#closeDetails = closeDetails;
    this.#detailHost.replaceChildren(backdrop);
    close.focus();
  }

  #showImagePreview(row: RowRecord): void {
    this.#closeDetails?.();
    const backdrop = document.createElement("div");
    backdrop.className = "detail-backdrop image-preview-backdrop";
    const panel = document.createElement("article");
    panel.className = "image-preview-panel";
    panel.setAttribute("role", "dialog");
    panel.setAttribute("aria-modal", "true");
    panel.setAttribute("aria-label", `Excel 第 ${row.sourceRow} 行图片预览`);

    const header = document.createElement("header");
    const title = document.createElement("div");
    const eyebrow = document.createElement("span");
    eyebrow.textContent = `Excel 第 ${row.sourceRow} 行`;
    const heading = document.createElement("h3");
    heading.textContent = row.time ?? "图片预览";
    title.append(eyebrow, heading);
    const close = document.createElement("button");
    close.type = "button";
    close.className = "detail-close";
    close.textContent = "关闭";
    header.append(title, close);

    const stage = document.createElement("div");
    stage.className = "image-preview-stage";
    const loading = document.createElement("span");
    loading.textContent = "正在加载预览…";
    stage.append(loading);
    panel.append(header, stage);
    backdrop.append(panel);

    let closed = false;
    let previewUrl: string | null = null;
    const onKeydown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        closePreview();
      }
    };
    const closePreview = (): void => {
      if (closed) {
        return;
      }
      closed = true;
      window.removeEventListener("keydown", onKeydown);
      if (previewUrl) {
        URL.revokeObjectURL(previewUrl);
      }
      this.#detailHost.replaceChildren();
      if (this.#closeDetails === closePreview) {
        this.#closeDetails = null;
      }
    };
    close.addEventListener("click", closePreview);
    backdrop.addEventListener("click", event => {
      if (event.target === backdrop) {
        closePreview();
      }
    });
    window.addEventListener("keydown", onKeydown);
    this.#closeDetails = closePreview;
    this.#detailHost.replaceChildren(backdrop);
    close.focus();

    void getRowPreview(row.id).then(
      response => {
        const buffer = binaryBuffer(response);
        const url = URL.createObjectURL(new Blob([buffer], { type: "image/png" }));
        if (closed) {
          URL.revokeObjectURL(url);
          return;
        }
        previewUrl = url;
        const image = document.createElement("img");
        image.src = url;
        image.alt = `Excel 第 ${row.sourceRow} 行图片预览`;
        stage.replaceChildren(image);
      },
      error => {
        if (!closed) {
          loading.textContent = `预览加载失败：${errorText(error)}`;
          loading.classList.add("is-error");
        }
      },
    );
  }
}

function cloneQuery(query: TableQuery): TableQuery {
  return { tags: [...query.tags], tagMode: query.tagMode as TagMatchMode };
}

function cell(value: string, className: string): HTMLElement {
  const element = document.createElement("div");
  element.className = `sheet-cell ${className}`;
  element.textContent = value;
  element.title = value;
  return element;
}

function tagsCell(tags: string[]): HTMLElement {
  const element = document.createElement("div");
  element.className = "sheet-cell tags-cell";
  if (tags.length === 0) {
    element.textContent = "—";
    return element;
  }
  for (const tag of tags.slice(0, 3)) {
    const chip = document.createElement("span");
    chip.textContent = tag;
    element.append(chip);
  }
  if (tags.length > 3) {
    const more = document.createElement("span");
    more.textContent = `+${tags.length - 3}`;
    element.append(more);
  }
  return element;
}

function detailSection(label: string, value: string | null): HTMLElement {
  const section = document.createElement("section");
  const header = document.createElement("div");
  const heading = document.createElement("h4");
  heading.textContent = label;
  const copy = document.createElement("button");
  copy.type = "button";
  copy.className = "text-action";
  copy.textContent = "复制";
  copy.disabled = !value;
  copy.addEventListener("click", () => void copyText(value ?? "", copy));
  header.append(heading, copy);
  const text = document.createElement("pre");
  text.textContent = value ?? "—";
  section.append(header, text);
  return section;
}

async function copyText(value: string, button: HTMLButtonElement): Promise<void> {
  try {
    await navigator.clipboard.writeText(value);
    button.textContent = "已复制";
  } catch {
    button.textContent = "复制失败";
  }
  window.setTimeout(() => {
    button.textContent = "复制";
  }, 1200);
}

function requiredElement<T extends Element>(root: Element, selector: string): T {
  const element = root.querySelector<T>(selector);
  if (!element) {
    throw new Error(`Missing virtual table element: ${selector}`);
  }
  return element;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
