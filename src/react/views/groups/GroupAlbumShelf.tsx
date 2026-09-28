import { useCallback } from "react";
import { getGroupMembers, queryRows, type GroupSummary } from "../../../lib/api";
import { useRows } from "../../state/library";
import { useGroups } from "../../state/groups";
import { RowAlbumCard } from "../../ui/RowAlbumCard";
import type { MenuItem } from "../../ui/controls";

export function GroupAlbumShelf({ groups, version, onOpen, menuFor }: { groups: GroupSummary[]; version: number; onOpen: (key: string) => void; menuFor: (group: GroupSummary) => MenuItem[] }) {
  return <div className="r-group-shelf" role="list" aria-label="分组相册">{groups.map(group => <GroupAlbum key={`${group.id}-${version}`} group={group} version={version} onOpen={() => onOpen(String(group.id))} menu={menuFor(group)} />)}<GroupAlbum key={`ungrouped-${version}`} version={version} onOpen={() => onOpen("ungrouped")} /></div>;
}
function GroupAlbum({ group, version, onOpen, menu }: { group?: GroupSummary; version: number; onOpen: () => void; menu?: MenuItem[] }) {
  const id = group?.id;
  const cached = useGroups(state => state.members[String(id ?? "ungrouped")]);
  const loadPreview = useCallback(() => id !== undefined ? getGroupMembers(id, 0, 3) : queryRows({
    ...structuredClone(useRows.getState().query), offset: 0, limit: 3, dedupe: "none", groupView: false, hideGrouped: true, sort: "timeAsc",
  }), [id]);
  const label = group?.name ?? "未分组";
  return <RowAlbumCard albumKey={String(id ?? "ungrouped")} previewKey={`groups:${version}:${id ?? "ungrouped"}`} label={label} count={group?.memberCount} actionLabel={`打开分组 ${label}`} loadPreview={loadPreview} initialPreview={cached?.rows.length ? cached : undefined} onOpen={onOpen} menu={menu} />;
}
