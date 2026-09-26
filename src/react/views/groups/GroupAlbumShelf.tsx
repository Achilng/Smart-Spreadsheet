import { useEffect, useRef, useState } from "react";
import { getGroupMembers, queryRows, type GroupSummary, type RowPage } from "../../../lib/api";
import { errorText } from "../../../lib/utils/format";
import { useRows } from "../../state/library";
import { AlbumStack } from "../../ui/AlbumStack";
import { Thumbnail } from "../../ui/Thumbnail";
import { Button } from "../../ui/controls";

let active = 0;
const waiting: (() => void)[] = [];
function limited<T>(run: () => Promise<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const begin = () => { active++; void run().then(resolve, reject).finally(() => { active--; waiting.shift()?.(); }); };
    if (active < 4) begin(); else waiting.push(begin);
  });
}
export function GroupAlbumShelf({ groups, version, onOpen }: { groups: GroupSummary[]; version: number; onOpen: (key: string) => void }) {
  return <div className="r-group-shelf" role="list" aria-label="分组相册">{groups.map(group => <GroupAlbum key={`${group.id}-${version}`} group={group} onOpen={() => onOpen(String(group.id))} />)}<GroupAlbum key={`ungrouped-${version}`} onOpen={() => onOpen("ungrouped")} /></div>;
}
function GroupAlbum({ group, onOpen }: { group?: GroupSummary; onOpen: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false), [page, setPage] = useState<RowPage | null>(null);
  const [error, setError] = useState(""), [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!ref.current) return;
    const observer = new IntersectionObserver(entries => setVisible(entries[0].isIntersecting), { root: ref.current.closest(".r-section-list"), rootMargin: "160px" });
    observer.observe(ref.current); return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (!visible || page || group?.memberCount === 0) return;
    let cancelled = false;
    const query = structuredClone(useRows.getState().query);
    setError("");
    void limited(async () => {
      if (cancelled) return null;
      return group ? getGroupMembers(group.id, 0, 3) : queryRows({ ...query, offset: 0, limit: 3, dedupe: "none", groupView: false, hideGrouped: true, sort: "timeAsc" });
    }).then(result => { if (!cancelled && result) setPage(result); }, cause => { if (!cancelled) setError(errorText(cause)); });
    return () => { cancelled = true; };
  }, [visible, group, attempt, page]);
  const label = group?.name ?? "未分组", count = group?.memberCount ?? page?.totalCount;
  return <div ref={ref} role="listitem" className="r-group-album">
    <button className="r-album-trigger" type="button" aria-label={`打开分组 ${label}`} onClick={onOpen}>
      <AlbumStack images={visible ? (page?.rows ?? []).map(row => <Thumbnail key={row.id} rowId={row.id} hasImage={Boolean(row.imagePath?.trim() || row.storedImagePath?.trim())} alt={label} />) : []} empty={error ? "封面加载失败" : count === 0 ? "空相册" : !page ? "正在加载…" : "暂无图片"} />
      <span className="r-album-title" title={label}>{label}</span><span className="r-album-meta">{count === undefined ? "待整理的图片" : `${count.toLocaleString()} 张图片`}</span>
    </button>
    {error && <div className="r-group-cover-error"><span role="alert">{error}</span><Button size="sm" onClick={() => setAttempt(value => value + 1)}>重试封面</Button></div>}
  </div>;
}
