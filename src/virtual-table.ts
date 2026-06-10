import { queryRows, type RowRecord } from "./api";

const PAGE_SIZE = 200;
const ROW_HEIGHT = 116;
const OVERSCAN = 5;

export class VirtualTable {
  readonly #root: HTMLElement;
  readonly #detailHost: HTMLElement;
  readonly #pages = new Map<number, RowRecord[]>();
  readonly #pendingPages = new Set<number>();
  #viewport: HTMLElement;
  #spacer: HTMLElement;
  #layer: HTMLElement;
  #totalCount = 0;
  #disposed = false;
  #renderFrame = 0;

  constructor(root: HTMLElement, detailHost: HTMLElement) {
    this.#root = root;
    this.#detailHost = detailHost;
    this.#root.innerHTML = `
      <div class="sheet-header" aria-hidden="true">
        <span>行</span><span>图片</span><span>时间</span><span>正向提示词</span>
        <span>负向提示词</span><span>画师</span><span>Tags</span><span></span>
      </div>
      <div class="sheet-viewport" role="table" aria-label="工作簿数据">
        <div class="sheet-spacer"><div class="sheet-layer"></div></div>
        <div class="table-loading">正在加载表格…</div>
      </div>
    `;
    this.#viewport = requiredElement(this.#root, ".sheet-viewport");
    this.#spacer = requiredElement(this.#root, ".sheet-spacer");
    this.#layer = requiredElement(this.#root, ".sheet-layer");
    this.#viewport.addEventListener("scroll", this.#scheduleRender, { passive: true });
    window.addEventListener("resize", this.#scheduleRender);
    void this.#loadPage(0);
  }

  dispose(): void {
    this.#disposed = true;
    this.#viewport.removeEventListener("scroll", this.#scheduleRender);
    window.removeEventListener("resize", this.#scheduleRender);
    window.cancelAnimationFrame(this.#renderFrame);
    this.#detailHost.replaceChildren();
  }

  readonly #scheduleRender = (): void => {
    window.cancelAnimationFrame(this.#renderFrame);
    this.#renderFrame = window.requestAnimationFrame(() => this.#renderVisibleRows());
  };

  async #loadPage(pageIndex: number): Promise<void> {
    if (this.#disposed || this.#pages.has(pageIndex) || this.#pendingPages.has(pageIndex)) {
      return;
    }
    this.#pendingPages.add(pageIndex);
    try {
      const page = await queryRows({
        offset: pageIndex * PAGE_SIZE,
        limit: PAGE_SIZE,
        tags: [],
        tagMode: "and",
      });
      if (this.#disposed) {
        return;
      }
      this.#pages.set(pageIndex, page.rows);
      this.#totalCount = page.totalCount;
      this.#spacer.style.height = `${this.#totalCount * ROW_HEIGHT}px`;
      this.#root.querySelector(".table-loading")?.remove();
      this.#renderVisibleRows();
    } catch (error) {
      if (!this.#disposed) {
        const loading = this.#root.querySelector<HTMLElement>(".table-loading");
        if (loading) {
          loading.textContent = `加载失败：${errorText(error)}`;
          loading.classList.add("is-error");
        }
      }
    } finally {
      this.#pendingPages.delete(pageIndex);
    }
  }

  #renderVisibleRows(): void {
    if (this.#disposed || this.#totalCount === 0) {
      return;
    }
    const visibleCount = Math.ceil(this.#viewport.clientHeight / ROW_HEIGHT);
    const first = Math.max(0, Math.floor(this.#viewport.scrollTop / ROW_HEIGHT) - OVERSCAN);
    const last = Math.min(this.#totalCount, first + visibleCount + OVERSCAN * 2);
    const fragment = document.createDocumentFragment();

    for (let index = first; index < last; index += 1) {
      const pageIndex = Math.floor(index / PAGE_SIZE);
      const row = this.#pages.get(pageIndex)?.[index % PAGE_SIZE];
      fragment.append(row ? this.#createRow(row, index) : this.#createSkeleton(index));
      if (!row) {
        void this.#loadPage(pageIndex);
      }
    }
    this.#layer.replaceChildren(fragment);

    const nextPage = Math.floor(last / PAGE_SIZE);
    if (nextPage * PAGE_SIZE < this.#totalCount) {
      void this.#loadPage(nextPage);
    }
  }

  #createRow(row: RowRecord, index: number): HTMLElement {
    const element = document.createElement("div");
    element.className = "sheet-row";
    element.style.transform = `translateY(${index * ROW_HEIGHT}px)`;
    element.setAttribute("role", "row");

    element.append(
      cell(String(row.sourceRow), "row-number"),
      imageCell(row),
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
    for (let column = 0; column < 8; column += 1) {
      const placeholder = document.createElement("span");
      placeholder.className = "skeleton-line";
      element.append(placeholder);
    }
    return element;
  }

  #showDetails(row: RowRecord): void {
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
    close.addEventListener("click", () => this.#detailHost.replaceChildren());
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
    backdrop.addEventListener("click", (event) => {
      if (event.target === backdrop) {
        this.#detailHost.replaceChildren();
      }
    });
    this.#detailHost.replaceChildren(backdrop);
    close.focus();
  }
}

function cell(value: string, className: string): HTMLElement {
  const element = document.createElement("div");
  element.className = `sheet-cell ${className}`;
  element.textContent = value;
  element.title = value;
  return element;
}

function imageCell(row: RowRecord): HTMLElement {
  const element = document.createElement("div");
  element.className = "sheet-cell image-cell";
  const placeholder = document.createElement("span");
  placeholder.textContent = row.embeddedImageRef ? "内嵌" : row.imagePath ? "路径" : "缺失";
  element.append(placeholder);
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
