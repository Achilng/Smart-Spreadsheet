import { useGalleryPick, finishGalleryPick } from "../state/gallery-pick";
import { useRows } from "../state/library";
import { useTasks } from "../state/tasks";
import { rowFileName } from "../../lib/utils/row-display";
import { Button } from "../ui/controls";

// This is a toolbar inside the real gallery, not a separate image browser.
export function GalleryPicker() {
  const pick = useGalleryPick(state => state.pick);
  const row = useRows(state => state.activeRow);
  const loading = useRows(state => state.loading || state.refreshing);
  const busy = useTasks(state => state.busy);
  if (!pick) return null;
  const compare = pick.kind === "compare";
  const isSample = compare && row?.id === pick.sampleId;
  const canChoose = row && !isSample && !loading && !busy && (compare || row.imagePath || row.storedImagePath);
  const title = compare ? "选择对比目标 B" : `为${pick.label}选择图片`;
  return <section className="rm-gallery-pick" aria-label={compare ? title : "素材选图"}>
    <div><strong>{title}</strong><span>{isSample ? "这是参考样本 A，请选择另一张图片" : row ? `已选：${rowFileName(row) ?? `图片 ${row.id}`}` : compare ? "点击画廊图片预览，选好后开始对比" : "点击画廊图片预览，选好后返回素材编辑"}</span></div>
    <div className="rm-actions"><Button onClick={() => finishGalleryPick()}>{compare ? "取消，返回对比" : "取消选图，返回编辑"}</Button><Button variant="primary" disabled={!canChoose} onClick={() => { if (canChoose) finishGalleryPick(row); }}>{compare ? "使用这张图作为 B" : "使用所选图片"}</Button></div>
  </section>;
}
