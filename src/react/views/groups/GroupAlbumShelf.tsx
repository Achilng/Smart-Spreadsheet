import { memo, useCallback } from "react";
import { getGroupMembers, queryRows, type GroupSummary } from "../../../lib/api";
import { useRows } from "../../state/library";
import { useGroups } from "../../state/groups";
import { AlbumShelf } from "../../ui/AlbumShelf";
import { RowAlbumCard } from "../../ui/RowAlbumCard";
import type { MenuItem } from "../../ui/controls";

export const GroupAlbumShelf = memo(function GroupAlbumShelf({ groups, version, onOpen, menuFor }: { groups: GroupSummary[]; version: number; onOpen: (key: string) => void; menuFor: (group: GroupSummary) => MenuItem[] }) {
  return <AlbumShelf label="分组相册">{groups.map(group => <div className="r-group-album-frame" data-album-reflow key={`${group.id}-${version}`}><GroupAlbum group={group} version={version} onOpen={onOpen} menuFor={menuFor} /></div>)}<div className="r-group-album-frame" data-album-reflow key={`ungrouped-${version}`}><GroupAlbum version={version} onOpen={onOpen} /></div></AlbumShelf>;
});
const GroupAlbum = memo(function GroupAlbum({ group, version, onOpen, menuFor }: { group?: GroupSummary; version: number; onOpen: (key: string) => void; menuFor?: (group: GroupSummary) => MenuItem[] }) {
  const id = group?.id;
  const cached = useGroups(state => state.members[String(id ?? "ungrouped")]);
  const loadPreview = useCallback(() => id !== undefined ? getGroupMembers(id, 0, 3) : queryRows({
    ...structuredClone(useRows.getState().query), offset: 0, limit: 3, dedupe: "none", groupView: false, hideGrouped: true, sort: "timeAsc",
  }), [id]);
  const label = group?.name ?? "未分组";
  return <RowAlbumCard albumKey={String(id ?? "ungrouped")} previewKey={`groups:${version}:${id ?? "ungrouped"}`} label={label} count={group?.memberCount} actionLabel={`打开分组 ${label}`} loadPreview={loadPreview} initialPreview={cached?.rows.length ? cached : undefined} onOpen={() => onOpen(String(id ?? "ungrouped"))} menu={group && menuFor ? menuFor(group) : undefined} />;
});
