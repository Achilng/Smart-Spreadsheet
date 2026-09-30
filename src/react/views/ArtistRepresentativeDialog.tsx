import { closeRepresentativeConflict, markArtistRepresentative, revealArtistRepresentative, useArtistRepresentative } from "../state/artist-representatives";
import { useTasks } from "../state/tasks";
import { Button, Modal } from "../ui/controls";

export function ArtistRepresentativeDialog() {
  const { conflict, error } = useArtistRepresentative();
  const busy = useTasks(state => state.busy);
  return <Modal open={Boolean(conflict)} onClose={closeRepresentativeConflict} busy={busy} width={500}
    title={conflict?.representativeId == null ? "这组的代表图已改变" : "这组画师串已经设置了代表图"}
    description="每组相同画师串只能标记一张代表图。"
    footer={<><Button disabled={busy} onClick={closeRepresentativeConflict}>取消</Button>
      {conflict?.representativeId != null && <Button disabled={busy} onClick={() => void revealArtistRepresentative()}>查看已标记图片</Button>}
      <Button variant="primary" disabled={busy || !conflict} onClick={() => { if (conflict) void markArtistRepresentative(conflict.row, true, conflict.representativeId); }}>替换为当前图片</Button></>}>
    <p>查看已标记图片会清除当前图片筛选，跳转到画廊并高亮该图片。替换后，旧图片的代表图标记会自动取消。</p>
    {error && <p role="alert" className="r-field-error">{error}</p>}
  </Modal>;
}
