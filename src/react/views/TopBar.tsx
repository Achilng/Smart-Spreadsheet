import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, X } from "lucide-react";
import { useWorkspace } from "../state/workspace";
import { setQuery, useLibrary, useRows } from "../state/library";
import { VIEW_MODES } from "../../lib/utils/view-modes";
import { formatCount } from "../../lib/utils/format";
import { Button, Hint, Menu } from "../ui/controls";
import { WindowControls } from "../ui/WindowControls";
import { navigateHistory, useNavigation } from "../state/navigation";
import { buildExportItems } from "../state/export-actions";
import { chooseImageArchive, chooseImageFolder, updateAutoArtistPrefixOnImport } from "../state/import-actions";
import { useTasks } from "../state/tasks";
import { useSelection } from "../state/selection";
import { setMaterialSearch, useMaterials } from "../state/materials";

export function TopBar({ onUpdateImport, onToolbox }: { onUpdateImport: () => void; onToolbox: () => void }) {
  const view = useWorkspace(state => state.viewMode);
  const setView = useWorkspace(state => state.setView);
  const total = useLibrary(state => state.snapshot?.library?.rowCount ?? 0);
  const autoArtistPrefix = useLibrary(state => state.snapshot?.autoArtistPrefixOnImport ?? false);
  const busy = useTasks(state => state.busy);
  useSelection();
  const rowSearch = useRows(state => state.query.search);
  const materialSearch = useMaterials(state => state.search);
  const materialMode = view === "materials";
  const search = materialMode ? materialSearch : rowSearch;
  const navigation = useNavigation();
  const searchSession = useRef(0);
  const lastInput = useRef(0);
  const [input, setInput] = useState(search);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const composing = useRef(false);
  const tabs = useRef<HTMLElement>(null);
  const [indicator, setIndicator] = useState<{ left: number; width: number } | null>(null);
  useEffect(() => { clearTimeout(searchTimer.current); setInput(search); }, [search, navigation.token, materialMode]);
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
  const applySearch = (value: string) => { if (materialMode) setMaterialSearch(value); else setQuery({ search: value }, searchSession.current); };
  const commit = (value: string) => { clearTimeout(searchTimer.current); applySearch(value); searchSession.current++; };
  const schedule = (value: string) => {
    if (Date.now() - lastInput.current > 1500) searchSession.current++;
    lastInput.current = Date.now();
    clearTimeout(searchTimer.current); searchTimer.current = setTimeout(() => applySearch(value), 300);
  };
  return <header className="r-topbar" data-tauri-drag-region>
    <div className="r-brand" data-tauri-drag-region><span data-tauri-drag-region>智能表格</span><small>{formatCount(total)} 张图片</small></div>
    <div className="r-history" role="group" aria-label="浏览历史">
      <Hint text={navigation.back ? `后退到 ${navigation.back}` : "没有可后退的浏览记录"}><Button variant="ghost" size="icon" disabled={!navigation.back || navigation.restoring} aria-label="后退" onClick={() => void navigateHistory(-1)}><ArrowLeft size={15} /></Button></Hint>
      <Hint text={navigation.forward ? `前进到 ${navigation.forward}` : "没有可前进的浏览记录"}><Button variant="ghost" size="icon" disabled={!navigation.forward || navigation.restoring} aria-label="前进" onClick={() => void navigateHistory(1)}><ArrowRight size={15} /></Button></Hint>
    </div>
    <nav className="r-segmented" aria-label="视图切换" ref={tabs}>
      {indicator && <span className="r-segment-indicator" style={{ transform: `translateX(${indicator.left}px)`, width: indicator.width }} />}
      {VIEW_MODES.map(item => <button key={item.mode} type="button" aria-pressed={view === item.mode} onClick={() => setView(item.mode)}>{item.label}</button>)}
    </nav>
    <div className="r-title-spacer" data-tauri-drag-region />
    {view !== "promptDocs" && <div className="r-search"><input type="text" placeholder={materialMode ? "搜索素材名称 / 文本内容…" : "搜索文件名 / 提示词 / 画师…"} aria-label={materialMode ? "搜索素材名称和文本" : "搜索文件名、提示词和画师"} value={input}
      onChange={event => { setInput(event.target.value); if (!composing.current) schedule(event.target.value); }}
      onCompositionStart={() => { composing.current = true; clearTimeout(searchTimer.current); }}
      onCompositionEnd={event => { composing.current = false; schedule(event.currentTarget.value); }}
      onBlur={() => { if (!composing.current) commit(input); }}
      onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) commit(input); }} />
      {input && <Button variant="ghost" size="icon" className="r-search-clear" aria-label="清除搜索" onClick={() => { setInput(""); commit(""); }}><X size={13} /></Button>}
    </div>}
    <div className="r-top-actions"><Button variant="ghost" disabled={busy} onClick={onToolbox}>工具箱</Button>
      {!materialMode && <><Menu label="导入" disabled={busy} items={[{ label: "导入文件夹", action: () => void chooseImageFolder() }, { label: "导入压缩包", hint: "zip / 7z / rar", action: () => void chooseImageArchive() },
        { label: "导入时自动补全画师前缀", hint: "仅使用库内明确 artist: 证据", checked: autoArtistPrefix, action: () => void updateAutoArtistPrefixOnImport(!autoArtistPrefix) },
        { label: "更新现有图片", hint: "只更新，不新增；保留 Tag / 分组", separator: true, action: onUpdateImport }]} />
      <Menu label="导出" variant="primary" disabled={!total || busy} items={buildExportItems()} /></>}
    </div><WindowControls />
  </header>;
}
