import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { defaultRangeExtractor, useVirtualizer } from "@tanstack/react-virtual";
import { useShallow } from "zustand/react/shallow";
import { Plus, RotateCw } from "lucide-react";
import { useLibrary } from "../../state/library";
import { useTasks } from "../../state/tasks";
import { initializePromptDocs, switchPromptDoc, usePromptDocs } from "../../state/prompt-docs";
import { Button, SearchField } from "../../ui/controls";

export function PromptDocList({ confirmDiscard }: { confirmDiscard: () => Promise<boolean> }) {
  const state = usePromptDocs(useShallow(({ docs, search, active, loading, error, directory }) => ({
    docs, search, activeId: active?.id, loading, error, directory,
  })));
  const directory = useLibrary(value => value.snapshot?.dataDirectory);
  const busy = useTasks(value => value.busy);
  const locked = state.loading || busy || state.directory !== (directory ?? null);
  const keyword = state.search.trim().toLocaleLowerCase();
  const docs = useMemo(() => state.docs.filter(doc => !keyword || `${doc.title}\n${doc.plainText}`.toLocaleLowerCase().includes(keyword)), [state.docs, keyword]);
  const viewport = useRef<HTMLDivElement>(null);
  const [focusedId, setFocusedId] = useState<string | null>(null);
  const pendingFocus = useRef<string | null>(null);
  const activeIndex = docs.findIndex(doc => doc.id === state.activeId);
  const focusedIndex = docs.findIndex(doc => doc.id === focusedId);
  const tabIndex = focusedIndex >= 0 ? focusedIndex : Math.max(0, activeIndex);
  const virtual = useVirtualizer({
    count: docs.length, getScrollElement: () => viewport.current, overscan: 3,
    getItemKey: useCallback((index: number) => docs[index].id, [docs]),
    estimateSize: useCallback((index: number) => docs[index].plainText.trim() ? 108 : 66, [docs]),
    rangeExtractor: range => {
      const visible = defaultRangeExtractor(range);
      // Scrolling must not unmount the button that owns keyboard focus.
      return focusedIndex < 0 ? visible : [...new Set([...visible, focusedIndex])].sort((a, b) => a - b);
    },
  });
  const items = virtual.getVirtualItems();
  useLayoutEffect(() => {
    if (!pendingFocus.current) return;
    const index = docs.findIndex(doc => doc.id === pendingFocus.current);
    const button = viewport.current?.querySelector<HTMLButtonElement>(`[data-doc-index="${index}"]`);
    if (button) { button.focus({ preventScroll: true }); pendingFocus.current = null; }
  }, [docs, focusedId, items]);
  useEffect(() => { virtual.scrollToOffset(0); }, [keyword, virtual]);
  useEffect(() => { if (activeIndex >= 0) virtual.scrollToIndex(activeIndex, { align: "auto" }); }, [state.activeId, activeIndex, virtual]);
  return <aside className="r-pd-list">
    <header><h2>提示词文档</h2><Button variant="primary" disabled={locked} onClick={() => void switchPromptDoc(null, confirmDiscard)}><Plus size={14} />新建</Button></header>
    <SearchField aria-label="搜索提示词文档" placeholder="搜索标题和正文…" value={state.search} onChange={event => usePromptDocs.setState({ search: event.target.value })} onClear={() => usePromptDocs.setState({ search: "" })} clearLabel="清除文档搜索" />
    {state.error && <div className="r-pd-error" role="alert"><p>{state.error}</p><Button disabled={locked} onClick={() => void initializePromptDocs()}><RotateCw size={14} />重新加载</Button></div>}
    <div ref={viewport} className="r-pd-items" role="list" aria-label="文档列表" onKeyDown={event => {
      if (locked || !docs.length || !["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      const index = event.key === "Home" ? 0 : event.key === "End" ? docs.length - 1 : Math.max(0, Math.min(docs.length - 1, tabIndex + (event.key === "ArrowDown" ? 1 : -1)));
      pendingFocus.current = docs[index].id; setFocusedId(docs[index].id);
      virtual.scrollToIndex(index, { align: "auto" });
    }}>
      {!state.error && !docs.length && <p className="r-pd-list-empty">{state.loading ? "正在加载文档…" : keyword ? "没有匹配的文档" : "还没有提示词文档"}</p>}
      <div className="r-pd-virtual" style={{ height: virtual.getTotalSize() }}>
        {items.map(item => { const doc = docs[item.index]; return <div key={item.key} data-index={item.index} ref={virtual.measureElement} role="listitem" aria-posinset={item.index + 1} aria-setsize={docs.length} className="r-pd-virtual-row" style={{ transform: `translateY(${item.start}px)` }}>
          <button data-doc-index={item.index} type="button" className={`r-pd-item ${state.activeId === doc.id ? "is-active" : ""}`} aria-current={state.activeId === doc.id ? "page" : undefined} disabled={locked} tabIndex={item.index === tabIndex ? 0 : -1} onFocus={() => setFocusedId(doc.id)} onClick={() => void switchPromptDoc(doc.id, confirmDiscard)}>
            <span className="r-pd-name" title={doc.title}>{doc.title}</span><time>{doc.updatedAt}</time>{doc.plainText.trim() && <span className="r-pd-snippet">{doc.plainText.trim().slice(0, 240)}</span>}
          </button>
        </div>; })}
      </div>
    </div>
  </aside>;
}
