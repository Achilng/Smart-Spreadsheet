import { useCallback } from "react";
import { getGroupMembers, queryRows, type GroupSummary } from "../../../lib/api";
import { useRows } from "../../state/library";
import { RowAlbumCard } from "../../ui/RowAlbumCard";

export function GroupAlbumShelf({ groups, version, onOpen }: { groups: GroupSummary[]; version: number; onOpen: (key: string) => void }) {
  return <div className="r-group-shelf" role="list" aria-label="分组相册">{groups.map(group => <GroupAlbum key={`${group.id}-${version}`} group={group} onOpen={() => onOpen(String(group.id))} />)}<GroupAlbum key={`ungrouped-${version}`} onOpen={() => onOpen("ungrouped")} /></div>;
}
function GroupAlbum({ group, onOpen }: { group?: GroupSummary; onOpen: () => void }) {
  const id = group?.id;
  const loadPreview = useCallback(() => id !== undefined ? getGroupMembers(id, 0, 3) : queryRows({
    ...structuredClone(useRows.getState().query), offset: 0, limit: 3, dedupe: "none", groupView: false, hideGrouped: true, sort: "timeAsc",
  }), [id]);
  const label = group?.name ?? "未分组";
  return <RowAlbumCard label={label} count={group?.memberCount} actionLabel={`打开分组 ${label}`} loadPreview={loadPreview} onOpen={onOpen} />;
}
