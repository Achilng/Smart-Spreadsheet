import { useEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowRight } from "lucide-react";
import { useWorkspace } from "../state/workspace";
import { setQuery, useLibrary, useRows } from "../state/library";
import { formatCount } from "../../lib/utils/format";
import { Button, Hint, SearchField } from "../ui/controls";
import { WindowControls } from "../ui/WindowControls";
import { navigateHistory, useNavigation } from "../state/navigation";
import { useSelection } from "../state/selection";
import { setMaterialSearch, useMaterials } from "../state/materials";

export function TopBar() {
  const view = useWorkspace(state => state.viewMode);
  const total = useLibrary(state => state.snapshot?.library?.rowCount ?? 0);
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
  useEffect(() => { clearTimeout(searchTimer.current); setInput(search); }, [search, navigation.token, materialMode]);
  useEffect(() => () => clearTimeout(searchTimer.current), []);
  const applySearch = (value: string) => { if (materialMode) setMaterialSearch(value); else setQuery({ search: value }, searchSession.current); };
  const commit = (value: string) => { clearTimeout(searchTimer.current); applySearch(value); searchSession.current++; };
  const schedule = (value: string) => {
    if (Date.now() - lastInput.current > 1500) searchSession.current++;
    lastInput.current = Date.now();
    clearTimeout(searchTimer.current); searchTimer.current = setTimeout(() => applySearch(value), 300);
  };
  return <header className="r-topbar" data-tauri-drag-region>
    <div className="r-topbar-start" data-tauri-drag-region>
      <div className="r-brand" data-tauri-drag-region><span data-tauri-drag-region>智能表格</span><small data-tauri-drag-region>{formatCount(total)} 张图片</small></div>
      <div className="r-history" role="group" aria-label="浏览历史">
        <Hint text={navigation.back ? `后退到 ${navigation.back}` : "没有可后退的浏览记录"}><Button variant="ghost" size="icon" disabled={!navigation.back || navigation.restoring} aria-label="后退" onClick={() => void navigateHistory(-1)}><ArrowLeft size={16} /></Button></Hint>
        <Hint text={navigation.forward ? `前进到 ${navigation.forward}` : "没有可前进的浏览记录"}><Button variant="ghost" size="icon" disabled={!navigation.forward || navigation.restoring} aria-label="前进" onClick={() => void navigateHistory(1)}><ArrowRight size={16} /></Button></Hint>
      </div>
    </div>
    {view === "promptDocs" ? <div data-tauri-drag-region /> : <SearchField placeholder={materialMode ? "搜索素材名称 / 文本内容…" : "搜索文件名 / 提示词 / 画师…"} aria-label={materialMode ? "搜索素材名称和文本" : "搜索文件名、提示词和画师"} value={input}
      onChange={event => { setInput(event.target.value); if (!composing.current) schedule(event.target.value); }}
      onCompositionStart={() => { composing.current = true; clearTimeout(searchTimer.current); }}
      onCompositionEnd={event => { composing.current = false; schedule(event.currentTarget.value); }}
      onBlur={() => { if (!composing.current) commit(input); }}
      onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) commit(input); }}
      onClear={() => { setInput(""); commit(""); }} />}
    <div className="r-topbar-end" data-tauri-drag-region><WindowControls /></div>
  </header>;
}
