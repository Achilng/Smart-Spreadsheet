import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, X } from "lucide-react";
import { useWorkspace } from "../state/workspace";
import { setQuery, useLibrary, useRows } from "../state/library";
import { VIEW_MODES } from "../../lib/utils/view-modes";
import { formatCount } from "../../lib/utils/format";
import { Button, Hint, Menu } from "../ui/controls";
import { WindowControls } from "../ui/WindowControls";

export function TopBar({ onImport, onExport, onToolbox }: { onImport: (archive: boolean) => void; onExport: () => void; onToolbox: () => void }) {
  const view = useWorkspace(state => state.viewMode);
  const setView = useWorkspace(state => state.setView);
  const total = useLibrary(state => state.snapshot?.library?.rowCount ?? 0);
  const search = useRows(state => state.query.search);
  const [input, setInput] = useState(search);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const composing = useRef(false);
  const tabs = useRef<HTMLElement>(null);
  const [indicator, setIndicator] = useState<{ left: number; width: number } | null>(null);
  useEffect(() => { clearTimeout(searchTimer.current); setInput(search); }, [search]);
  useEffect(() => () => clearTimeout(searchTimer.current), []);
  useLayoutEffect(() => {
    const root = tabs.current;
    if (!root) return;
    const update = () => {
      const active = root.querySelector<HTMLButtonElement>('[aria-pressed="true"]');
      if (active) setIndicator({ left: active.offsetLeft, width: active.offsetWidth });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(root);
    return () => observer.disconnect();
  }, [view]);
  const commit = (value: string) => { clearTimeout(searchTimer.current); setQuery({ search: value }); };
  const schedule = (value: string) => { clearTimeout(searchTimer.current); searchTimer.current = setTimeout(() => setQuery({ search: value }), 300); };
  return <header className="r-topbar" data-tauri-drag-region>
    <div className="r-brand" data-tauri-drag-region><span data-tauri-drag-region>智能表格</span><small>{formatCount(total)} 张图片</small></div>
    <div className="r-history" role="group" aria-label="浏览历史">
      <Hint text="没有可后退的浏览记录"><Button variant="ghost" size="icon" disabled aria-label="后退"><ArrowLeft size={15} /></Button></Hint>
      <Hint text="没有可前进的浏览记录"><Button variant="ghost" size="icon" disabled aria-label="前进"><ArrowRight size={15} /></Button></Hint>
    </div>
    <nav className="r-segmented" aria-label="视图切换" ref={tabs}>
      {indicator && <span className="r-segment-indicator" style={{ transform: `translateX(${indicator.left}px)`, width: indicator.width }} />}
      {VIEW_MODES.map(item => <button key={item.mode} type="button" aria-pressed={view === item.mode} onClick={() => setView(item.mode)}>{item.label}</button>)}
    </nav>
    <div className="r-title-spacer" data-tauri-drag-region />
    {view !== "promptDocs" && <div className="r-search"><input type="text" placeholder="搜索文件名 / 提示词 / 画师…" aria-label="搜索文件名、提示词和画师" value={input}
      onChange={event => { setInput(event.target.value); if (!composing.current) schedule(event.target.value); }}
      onCompositionStart={() => { composing.current = true; clearTimeout(searchTimer.current); }}
      onCompositionEnd={event => { composing.current = false; schedule(event.currentTarget.value); }}
      onBlur={() => { if (!composing.current) commit(input); }}
      onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) commit(input); }} />
      {input && <Button variant="ghost" size="icon" className="r-search-clear" aria-label="清除搜索" onClick={() => { setInput(""); commit(""); }}><X size={13} /></Button>}
    </div>}
    <div className="r-top-actions"><Button variant="ghost" onClick={onToolbox}>工具箱</Button>
      <Menu label="导入" items={[{ label: "导入文件夹", action: () => onImport(false) }, { label: "导入压缩包", hint: "zip / 7z / rar", action: () => onImport(true) }]} />
      <Menu label="导出" variant="primary" disabled={!total} items={[{ label: "导出选项", action: onExport }]} />
    </div><WindowControls />
  </header>;
}
