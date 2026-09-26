import { Bookmark, Copy, FileText, Folders, Images, Table2, Wrench } from "lucide-react";
import { useWorkspace } from "../state/workspace";
import { openToolboxWindow } from "../../lib/windows/toolbox";
import { notify } from "../state/notices";
import { errorText } from "../../lib/utils/format";
import "../ui/album.css";

const entries = [{ mode: "gallery", label: "画廊", Icon: Images }, { mode: "group", label: "分组", Icon: Folders }, { mode: "materials", label: "素材", Icon: Bookmark }, { mode: "table", label: "表格", Icon: Table2 }, { mode: "promptDocs", label: "文档", Icon: FileText }, { mode: "duplicates", label: "重复项", Icon: Copy }] as const;
export function AlbumNavigation() {
  const view = useWorkspace(state => state.viewMode), setView = useWorkspace(state => state.setView);
  return <nav className="r-album-rail" aria-label="视图切换">{entries.map(({ mode, label, Icon }) => <button type="button" key={mode} aria-pressed={view === mode} onClick={() => setView(mode)}><Icon size={20} />{label}</button>)}<button type="button" className="r-album-tools" onClick={() => void openToolboxWindow().catch(error => notify(errorText(error), "error"))}><Wrench size={20} />工具箱</button></nav>;
}
