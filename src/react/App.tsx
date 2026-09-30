import { useEffect, useState } from "react";
import { initializeLibrary, useLibrary, useRows } from "./state/library";
import { Tooltip } from "./ui/controls";
import { TopBar } from "./views/TopBar";
import { TagSidebar } from "./views/TagSidebar";
import { CanvasHeader } from "./views/CanvasHeader";
import { Gallery } from "./views/Gallery";
import { DetailPanel } from "./views/DetailPanel";
import { Notices } from "./ui/Notices";
import { Table } from "./views/Table";
import { useWorkspace } from "./state/workspace";
import { useWorkspaceLifecycle } from "./ui/use-workspace-lifecycle";
import { JsonExportDialog } from "./views/JsonExportDialog";
import { UpdateImportDialog } from "./views/UpdateImportDialog";
import { TaskProgress } from "./views/TaskProgress";
import { RowActionDialogs } from "./views/RowActionDialogs";
import { SelectionBar } from "./views/SelectionBar";
import { StartupScreen } from "./views/StartupScreen";
import { runStartupMaintenance } from "./state/library-session";
import { FilterPanel } from "./views/FilterPanel";
import { MaterialsView } from "./views/materials/MaterialsView";
import { PromptDocsView } from "./views/prompt-docs/PromptDocsView";
import { GroupBrowseView } from "./views/groups/GroupBrowseView";
import { DuplicateBrowseView } from "./views/duplicates/DuplicateBrowseView";
import { AlbumNavigation } from "./views/AlbumNavigation";
import { useGroups } from "./state/groups";
import { useDuplicates } from "./state/duplicates";

export function App() {
  useWorkspaceLifecycle();
  const loaded = useLibrary(state => state.loaded);
  const error = useLibrary(state => state.error ?? state.snapshot?.startupError);
  const configured = useLibrary(state => Boolean(state.snapshot?.dataDirectory));
  const refreshing = useRows(state => state.refreshing);
  const view = useWorkspace(state => state.viewMode);
  const [dialog, setDialog] = useState<string | null>(null);
  const [imageFilters, setImageFilters] = useState(false);
  const expandedGroups = useGroups(state => state.expanded);
  const duplicateDetails = useDuplicates(state => state.layout === "list" || state.expanded.length > 0);
  useEffect(() => { void initializeLibrary().then(runStartupMaintenance); }, []);
  return <Tooltip.Provider delayDuration={550} skipDelayDuration={150}>
    {!loaded || error || !configured ? <StartupScreen />
      : <div className="r-workspace"><TopBar />
        <div className="r-workspace-body"><AlbumNavigation onUpdateImport={() => setDialog("update")} /><MaterialsView active={view === "materials"} />{view === "promptDocs" ? <PromptDocsView /> : view !== "materials" && <><div className="r-library-filter-panel" data-open={imageFilters} inert={!imageFilters}><TagSidebar onFilter={() => setDialog("过滤")} /></div><main className="r-main-area">
          {refreshing && <div className="r-refresh-bar" role="status" aria-label="正在刷新" />}<CanvasHeader filtersOpen={imageFilters} onFilters={() => setImageFilters(value => !value)} />{view === "table" ? <Table /> : view === "group" ? <GroupBrowseView filtersOpen={imageFilters} onFilters={() => setImageFilters(value => !value)} /> : view === "duplicates" ? <DuplicateBrowseView filtersOpen={imageFilters} onFilters={() => setImageFilters(value => !value)} /> : <Gallery />}
          <SelectionBar />
        </main>{(view !== "group" || expandedGroups.length > 0) && (view !== "duplicates" || duplicateDetails) && <DetailPanel />}</>}</div>
      </div>}
    {dialog === "过滤" && <FilterPanel onClose={() => setDialog(null)} />}{dialog === "update" && <UpdateImportDialog onClose={() => setDialog(null)} />}
    <RowActionDialogs /><JsonExportDialog /><TaskProgress /><Notices />
  </Tooltip.Provider>;
}
