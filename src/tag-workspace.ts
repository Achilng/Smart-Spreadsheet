import {
  addTagsToSelection,
  countSelectedRows,
  listUsedTags,
  removeTagsFromSelection,
  type RowSelection,
  type TagMatchMode,
  type TagSummary,
} from "./api";
import { VirtualTable, type VirtualTablePageState } from "./virtual-table";

type SelectionState =
  | { kind: "explicit"; rowIds: Set<number> }
  | {
      kind: "filtered";
      tags: string[];
      tagMode: TagMatchMode;
      excludedRowIds: Set<number>;
      totalCount: number;
    };

type Mutation = "add" | "remove";

export class TagWorkspace {
  readonly #filterRoot: HTMLElement;
  readonly #selectionRoot: HTMLElement;
  readonly #table: VirtualTable;
  #usedTags: TagSummary[] = [];
  #activeTags: string[] = [];
  #tagMode: TagMatchMode = "and";
  #selection: SelectionState = explicitSelection();
  #pageState: VirtualTablePageState = {
    pageIndex: 0,
    start: 0,
    end: 0,
    totalCount: 0,
    rows: [],
  };
  #busy = false;
  #disposed = false;

  constructor(
    filterRoot: HTMLElement,
    selectionRoot: HTMLElement,
    tableRoot: HTMLElement,
    detailHost: HTMLElement,
  ) {
    this.#filterRoot = filterRoot;
    this.#selectionRoot = selectionRoot;
    this.#renderShells();
    this.#bindActions();
    this.#table = new VirtualTable(tableRoot, detailHost, {
      query: { tags: this.#activeTags, tagMode: this.#tagMode },
      isRowSelected: rowId => this.#isRowSelected(rowId),
      onRowSelectionChange: (rowId, selected) => this.#toggleRow(rowId, selected),
      onPageStateChange: state => this.#handlePageState(state),
    });
    this.#refreshFilterUi();
    this.#refreshSelectionUi();
    void this.#loadUsedTags();
  }

  dispose(): void {
    this.#disposed = true;
    this.#table.dispose();
  }

  #renderShells(): void {
    this.#filterRoot.innerHTML = `
      <section class="tag-filter-panel" aria-labelledby="tag-filter-title">
        <div class="filter-heading">
          <div>
            <span class="step-label">Tag 筛选</span>
            <h4 id="tag-filter-title">按精确大小写组合筛选</h4>
          </div>
          <div class="mode-switch" aria-label="Tag 筛选模式">
            <button type="button" data-tag-mode="and">AND</button>
            <button type="button" data-tag-mode="or">OR</button>
          </div>
        </div>
        <div class="tag-filter-list" aria-live="polite"></div>
        <div class="filter-footer">
          <span class="filter-match-count">正在统计匹配记录…</span>
          <button type="button" class="text-action clear-filter">清除筛选</button>
        </div>
        <p class="tag-filter-status" role="status"></p>
      </section>
    `;
    this.#selectionRoot.innerHTML = `
      <section class="selection-panel" aria-labelledby="selection-title">
        <div class="selection-actions">
          <div>
            <span class="step-label">批量选择</span>
            <h4 id="selection-title">选择当前页或全部筛选结果</h4>
          </div>
          <div class="selection-buttons">
            <button type="button" class="secondary-action select-page">选择当前页</button>
            <button type="button" class="secondary-action select-filtered">全选筛选结果</button>
            <button type="button" class="text-action clear-selection">清除选择</button>
          </div>
        </div>
        <div class="batch-editor">
          <label>
            <span>待操作 Tag（每行一个）</span>
            <textarea class="batch-tags" rows="2" placeholder="Landscape&#10;favorite"></textarea>
          </label>
          <div class="batch-buttons">
            <button type="button" class="primary-action add-tags">批量添加</button>
            <button type="button" class="danger-action remove-tags">批量删除</button>
          </div>
          <div class="selection-summary">
            <strong>已选择 0 行</strong>
            <span>尚未选择记录</span>
          </div>
        </div>
        <p class="batch-status" role="status"></p>
      </section>
    `;
  }

  #bindActions(): void {
    for (const button of this.#filterRoot.querySelectorAll<HTMLButtonElement>("[data-tag-mode]")) {
      button.addEventListener("click", () => {
        if (this.#busy) {
          return;
        }
        const mode = button.dataset.tagMode;
        if ((mode === "and" || mode === "or") && mode !== this.#tagMode) {
          this.#tagMode = mode;
          this.#applyQueryChange();
        }
      });
    }
    requiredElement<HTMLButtonElement>(this.#filterRoot, ".clear-filter").addEventListener(
      "click",
      () => {
        if (!this.#busy && this.#activeTags.length > 0) {
          this.#activeTags = [];
          this.#applyQueryChange();
        }
      },
    );
    requiredElement<HTMLButtonElement>(this.#selectionRoot, ".select-page").addEventListener(
      "click",
      () => this.#toggleCurrentPage(),
    );
    requiredElement<HTMLButtonElement>(this.#selectionRoot, ".select-filtered").addEventListener(
      "click",
      () => void this.#selectAllFiltered(),
    );
    requiredElement<HTMLButtonElement>(this.#selectionRoot, ".clear-selection").addEventListener(
      "click",
      () => this.#clearSelection(),
    );
    requiredElement<HTMLButtonElement>(this.#selectionRoot, ".add-tags").addEventListener(
      "click",
      () => void this.#mutateTags("add"),
    );
    requiredElement<HTMLButtonElement>(this.#selectionRoot, ".remove-tags").addEventListener(
      "click",
      () => void this.#mutateTags("remove"),
    );
  }

  async #loadUsedTags(): Promise<void> {
    try {
      const tags = await listUsedTags();
      if (this.#disposed) {
        return;
      }
      this.#usedTags = tags;
      this.#setFilterStatus("");
      this.#refreshFilterUi();
    } catch (error) {
      if (!this.#disposed) {
        this.#setFilterStatus(`Tag 列表加载失败：${errorText(error)}`, true);
      }
    }
  }

  #applyQueryChange(): void {
    this.#selection = explicitSelection();
    this.#setBatchStatus("");
    this.#table.setQuery({ tags: this.#activeTags, tagMode: this.#tagMode });
    this.#refreshFilterUi();
    this.#refreshSelectionUi();
  }

  #toggleTag(tag: string): void {
    if (this.#activeTags.includes(tag)) {
      this.#activeTags = this.#activeTags.filter(active => active !== tag);
    } else {
      this.#activeTags = [...this.#activeTags, tag];
    }
    this.#applyQueryChange();
  }

  #handlePageState(state: VirtualTablePageState): void {
    this.#pageState = state;
    this.#refreshFilterUi();
    this.#refreshSelectionUi();
  }

  #isRowSelected(rowId: number): boolean {
    if (this.#selection.kind === "explicit") {
      return this.#selection.rowIds.has(rowId);
    }
    return !this.#selection.excludedRowIds.has(rowId);
  }

  #toggleRow(rowId: number, selected: boolean): void {
    if (this.#selection.kind === "explicit") {
      if (selected) {
        this.#selection.rowIds.add(rowId);
      } else {
        this.#selection.rowIds.delete(rowId);
      }
    } else if (selected) {
      this.#selection.excludedRowIds.delete(rowId);
    } else {
      this.#selection.excludedRowIds.add(rowId);
    }
    this.#setBatchStatus("");
    this.#refreshSelectionUi();
  }

  #toggleCurrentPage(): void {
    if (this.#busy) {
      return;
    }
    const rows = this.#table.getCurrentPageRows();
    if (rows.length === 0) {
      return;
    }
    const allSelected = rows.every(row => this.#isRowSelected(row.id));
    if (this.#selection.kind === "explicit") {
      for (const row of rows) {
        if (allSelected) {
          this.#selection.rowIds.delete(row.id);
        } else {
          this.#selection.rowIds.add(row.id);
        }
      }
    } else {
      for (const row of rows) {
        if (allSelected) {
          this.#selection.excludedRowIds.add(row.id);
        } else {
          this.#selection.excludedRowIds.delete(row.id);
        }
      }
    }
    this.#setBatchStatus("");
    this.#table.refreshSelection();
    this.#refreshSelectionUi();
  }

  async #selectAllFiltered(): Promise<void> {
    if (this.#busy) {
      return;
    }
    const selection: RowSelection = {
      kind: "filtered",
      tags: [...this.#activeTags],
      tagMode: this.#tagMode,
      excludedRowIds: [],
    };
    this.#setBusy(true);
    try {
      const totalCount = await countSelectedRows(selection);
      if (this.#disposed) {
        return;
      }
      this.#selection = {
        ...selection,
        excludedRowIds: new Set<number>(),
        totalCount,
      };
      this.#setBatchStatus(
        totalCount === 0 ? "当前筛选没有可选择的记录。" : `已全选 ${formatCount(totalCount)} 条筛选结果。`,
        totalCount === 0,
      );
      this.#table.refreshSelection();
    } catch (error) {
      if (!this.#disposed) {
        this.#setBatchStatus(`全选失败：${errorText(error)}`, true);
      }
    } finally {
      if (!this.#disposed) {
        this.#setBusy(false);
      }
    }
  }

  #clearSelection(): void {
    if (this.#busy || this.#selectionCount() === 0) {
      return;
    }
    this.#selection = explicitSelection();
    this.#setBatchStatus("");
    this.#table.refreshSelection();
    this.#refreshSelectionUi();
  }

  async #mutateTags(mutation: Mutation): Promise<void> {
    if (this.#busy) {
      return;
    }
    const selectedCount = this.#selectionCount();
    const textarea = requiredElement<HTMLTextAreaElement>(this.#selectionRoot, ".batch-tags");
    const tags = parseTags(textarea.value);
    if (selectedCount === 0) {
      this.#setBatchStatus("请先选择至少一行。", true);
      return;
    }
    if (tags.length === 0) {
      this.#setBatchStatus("请输入至少一个非空 Tag，每行一个。", true);
      return;
    }
    if (
      mutation === "remove" &&
      !window.confirm(`将从 ${formatCount(selectedCount)} 行删除 ${tags.length} 个 Tag。是否继续？`)
    ) {
      return;
    }

    this.#setBusy(true);
    this.#setBatchStatus("");
    try {
      const selection = this.#selectionDto();
      const result =
        mutation === "add"
          ? await addTagsToSelection(selection, tags)
          : await removeTagsFromSelection(selection, tags);
      if (this.#disposed) {
        return;
      }
      this.#selection = explicitSelection();
      textarea.value = "";
      this.#setBatchStatus(
        `已处理 ${formatCount(result.affectedRows)} 行，实际变更 ${formatCount(result.associationsChanged)} 个 Tag 关联。`,
      );
      this.#table.reload();
      this.#table.refreshSelection();
      await this.#loadUsedTags();
    } catch (error) {
      if (!this.#disposed) {
        this.#setBatchStatus(`批量操作失败：${errorText(error)}`, true);
      }
    } finally {
      if (!this.#disposed) {
        this.#setBusy(false);
      }
    }
  }

  #selectionDto(): RowSelection {
    if (this.#selection.kind === "explicit") {
      return {
        kind: "explicit",
        rowIds: [...this.#selection.rowIds].sort((left, right) => left - right),
      };
    }
    return {
      kind: "filtered",
      tags: [...this.#selection.tags],
      tagMode: this.#selection.tagMode,
      excludedRowIds: [...this.#selection.excludedRowIds].sort((left, right) => left - right),
    };
  }

  #selectionCount(): number {
    if (this.#selection.kind === "explicit") {
      return this.#selection.rowIds.size;
    }
    return Math.max(0, this.#selection.totalCount - this.#selection.excludedRowIds.size);
  }

  #refreshFilterUi(): void {
    const list = requiredElement<HTMLElement>(this.#filterRoot, ".tag-filter-list");
    const countByName = new Map(this.#usedTags.map(tag => [tag.name, tag.rowCount]));
    const names = [...this.#usedTags.map(tag => tag.name)];
    for (const active of this.#activeTags) {
      if (!countByName.has(active)) {
        names.push(active);
      }
    }
    const fragment = document.createDocumentFragment();
    if (names.length === 0) {
      const empty = document.createElement("span");
      empty.className = "empty-tags";
      empty.textContent = "还没有 Tag。选择记录后可在下方批量添加。";
      fragment.append(empty);
    } else {
      for (const name of names) {
        const button = document.createElement("button");
        button.type = "button";
        button.className = `tag-filter-chip${this.#activeTags.includes(name) ? " is-active" : ""}`;
        button.disabled = this.#busy;
        button.setAttribute("aria-pressed", String(this.#activeTags.includes(name)));
        const label = document.createElement("span");
        label.textContent = name;
        const count = document.createElement("small");
        count.textContent = formatCount(countByName.get(name) ?? 0);
        button.append(label, count);
        button.addEventListener("click", () => this.#toggleTag(name));
        fragment.append(button);
      }
    }
    list.replaceChildren(fragment);

    for (const button of this.#filterRoot.querySelectorAll<HTMLButtonElement>("[data-tag-mode]")) {
      const active = button.dataset.tagMode === this.#tagMode;
      button.classList.toggle("is-active", active);
      button.setAttribute("aria-pressed", String(active));
      button.disabled = this.#busy;
    }
    const clear = requiredElement<HTMLButtonElement>(this.#filterRoot, ".clear-filter");
    clear.disabled = this.#busy || this.#activeTags.length === 0;
    requiredElement<HTMLElement>(this.#filterRoot, ".filter-match-count").textContent =
      `${formatCount(this.#pageState.totalCount)} 条匹配 · ${this.#tagMode.toUpperCase()} · 区分大小写`;
  }

  #refreshSelectionUi(): void {
    const rows = this.#pageState.rows;
    const allPageSelected = rows.length > 0 && rows.every(row => this.#isRowSelected(row.id));
    const pageButton = requiredElement<HTMLButtonElement>(this.#selectionRoot, ".select-page");
    const range =
      this.#pageState.totalCount === 0
        ? ""
        : ` ${formatCount(this.#pageState.start + 1)}–${formatCount(this.#pageState.end)}`;
    pageButton.textContent = `${allPageSelected ? "取消" : "选择"}当前页${range}`;
    pageButton.disabled = this.#busy || rows.length === 0;

    const filteredButton = requiredElement<HTMLButtonElement>(
      this.#selectionRoot,
      ".select-filtered",
    );
    filteredButton.textContent = `全选筛选结果（${formatCount(this.#pageState.totalCount)}）`;
    filteredButton.disabled = this.#busy || this.#pageState.totalCount === 0;
    requiredElement<HTMLButtonElement>(this.#selectionRoot, ".clear-selection").disabled =
      this.#busy || this.#selectionCount() === 0;

    const summary = requiredElement<HTMLElement>(this.#selectionRoot, ".selection-summary");
    requiredElement<HTMLElement>(summary, "strong").textContent =
      `已选择 ${formatCount(this.#selectionCount())} 行`;
    requiredElement<HTMLElement>(summary, "span").textContent =
      this.#selection.kind === "filtered"
        ? `筛选结果全选，已排除 ${formatCount(this.#selection.excludedRowIds.size)} 行`
        : this.#selectionCount() === 0
          ? "尚未选择记录"
          : "显式选择，可跨页继续勾选";

    const hasSelection = this.#selectionCount() > 0;
    requiredElement<HTMLButtonElement>(this.#selectionRoot, ".add-tags").disabled =
      this.#busy || !hasSelection;
    requiredElement<HTMLButtonElement>(this.#selectionRoot, ".remove-tags").disabled =
      this.#busy || !hasSelection;
    requiredElement<HTMLTextAreaElement>(this.#selectionRoot, ".batch-tags").disabled = this.#busy;
  }

  #setBusy(busy: boolean): void {
    this.#busy = busy;
    this.#refreshFilterUi();
    this.#refreshSelectionUi();
  }

  #setFilterStatus(message: string, error = false): void {
    const status = requiredElement<HTMLElement>(this.#filterRoot, ".tag-filter-status");
    status.textContent = message;
    status.classList.toggle("is-error", error);
  }

  #setBatchStatus(message: string, error = false): void {
    const status = requiredElement<HTMLElement>(this.#selectionRoot, ".batch-status");
    status.textContent = message;
    status.classList.toggle("is-error", error);
  }
}

function explicitSelection(): SelectionState {
  return { kind: "explicit", rowIds: new Set<number>() };
}

function parseTags(value: string): string[] {
  const seen = new Set<string>();
  const tags: string[] = [];
  for (const line of value.split(/\r?\n/)) {
    const tag = line.trim();
    if (tag && !seen.has(tag)) {
      seen.add(tag);
      tags.push(tag);
    }
  }
  return tags;
}

function formatCount(value: number): string {
  return value.toLocaleString("zh-CN");
}

function requiredElement<T extends Element>(root: Element, selector: string): T {
  const element = root.querySelector<T>(selector);
  if (!element) {
    throw new Error(`Missing Tag workspace element: ${selector}`);
  }
  return element;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
