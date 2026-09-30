import { useMaterials, finishMaterialGalleryPick } from "../../state/materials";
import { useRows } from "../../state/library";
import { useTasks } from "../../state/tasks";
import { rowFileName } from "../../../lib/utils/row-display";
import { Button } from "../../ui/controls";

// This is a toolbar inside the real gallery, not a separate image browser.
export function MaterialGalleryPicker() {
  const pick = useMaterials(state => state.galleryPick);
  const row = useRows(state => state.activeRow);
  const loading = useRows(state => state.loading || state.refreshing);
  const busy = useTasks(state => state.busy);
  if (!pick) return null;
  const canChoose = row && (row.imagePath || row.storedImagePath) && !loading && !busy;
  return <section className="rm-gallery-pick" aria-label="素材选图">
    <div><strong>为{pick.label}选择图片</strong><span>{row ? `已选：${rowFileName(row) ?? `图片 ${row.id}`}` : "点击画廊图片预览，选好后返回素材编辑"}</span></div>
    <div className="rm-actions"><Button onClick={() => finishMaterialGalleryPick()}>取消选图，返回编辑</Button><Button variant="primary" disabled={!canChoose} onClick={() => { if (canChoose) finishMaterialGalleryPick(row.id); }}>使用所选图片</Button></div>
  </section>;
}
