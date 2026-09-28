import { forwardRef, type ComponentProps, type ReactNode } from "react";
import { Bookmark, Copy, FileText, Folders, FolderInput, Images, Share, Table2, Wrench } from "lucide-react";
import { useWorkspace } from "../state/workspace";
import { useLibrary } from "../state/library";
import { useTasks } from "../state/tasks";
import { useSelection } from "../state/selection";
import { buildExportItems, exportScopeLabel } from "../state/export-actions";
import { chooseImageArchive, chooseImageFolder, updateAutoArtistPrefixOnImport } from "../state/import-actions";
import { openToolboxWindow } from "../../lib/windows/toolbox";
import { notify } from "../state/notices";
import { errorText } from "../../lib/utils/format";
import { Menu } from "../ui/controls";
import "../ui/album.css";

const entries = [{ mode: "gallery", label: "画廊", Icon: Images }, { mode: "group", label: "分组", Icon: Folders }, { mode: "materials", label: "素材", Icon: Bookmark }, { mode: "table", label: "表格", Icon: Table2 }, { mode: "promptDocs", label: "文档", Icon: FileText }, { mode: "duplicates", label: "重复项", Icon: Copy }] as const;

const RailButton = forwardRef<HTMLButtonElement, ComponentProps<"button"> & { icon: ReactNode; label: string }>(function RailButton({ icon, label, className, ...props }, ref) {
  return <button ref={ref} type="button" className={`r-rail-item${className ? ` ${className}` : ""}`} {...props}>{icon}{label}</button>;
});

/** Views on top; library-wide actions (import, export, toolbox) stay fixed at the bottom. */
export function AlbumNavigation({ onUpdateImport }: { onUpdateImport: () => void }) {
  const view = useWorkspace(state => state.viewMode), setView = useWorkspace(state => state.setView);
  const total = useLibrary(state => state.snapshot?.library?.rowCount ?? 0);
  const autoArtistPrefix = useLibrary(state => state.snapshot?.autoArtistPrefixOnImport ?? false);
  const busy = useTasks(state => state.busy);
  useSelection();
  return <nav className="r-album-rail" aria-label="视图切换">
    {entries.map(({ mode, label, Icon }) => <RailButton key={mode} aria-pressed={view === mode} onClick={() => setView(mode)} icon={<Icon size={20} />} label={label} />)}
    <div className="r-rail-actions">
      <Menu direction="right" disabled={busy} heading="导入图片到图库" trigger={<RailButton disabled={busy} icon={<FolderInput size={20} />} label="导入" />} items={[
        { label: "导入文件夹", action: () => void chooseImageFolder() },
        { label: "导入压缩包", hint: "zip / 7z / rar", action: () => void chooseImageArchive() },
        { label: "导入时自动补全画师前缀", hint: "仅使用库内明确 artist: 证据", checked: autoArtistPrefix, action: () => void updateAutoArtistPrefixOnImport(!autoArtistPrefix) },
        { label: "更新现有图片", hint: "只更新，不新增；保留 Tag / 分组", separator: true, action: onUpdateImport },
      ]} />
      <Menu direction="right" disabled={!total || busy} heading={`导出范围：${exportScopeLabel()}`} trigger={<RailButton disabled={!total || busy} icon={<Share size={20} />} label="导出" />} items={buildExportItems()} />
      <RailButton icon={<Wrench size={20} />} label="工具箱" onClick={() => void openToolboxWindow().catch(error => notify(`无法打开工具箱：${errorText(error)}`, "error"))} />
    </div>
  </nav>;
}
