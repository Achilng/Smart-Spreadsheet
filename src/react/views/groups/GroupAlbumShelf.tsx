import { memo, useCallback } from "react";
import { motion } from "motion/react";
import { getGroupMembers, queryRows, type GroupSummary } from "../../../lib/api";
import { useRows } from "../../state/library";
import { useGroups } from "../../state/groups";
import { useGridColumns } from "../../ui/use-grid-columns";
import { useReducedMotionPreference } from "../../ui/use-reduced-motion";
import { RowAlbumCard } from "../../ui/RowAlbumCard";
import type { MenuItem } from "../../ui/controls";

export const GroupAlbumShelf = memo(function GroupAlbumShelf({ groups, version, onOpen, menuFor }: { groups: GroupSummary[]; version: number; onOpen: (key: string) => void; menuFor: (group: GroupSummary) => MenuItem[] }) {
  const { ref, columns } = useGridColumns(200, 16);
  const reducedMotion = useReducedMotionPreference();
  return <div ref={ref} style={columns ? { gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` } : undefined} className="r-group-shelf" role="list" aria-label="分组相册">{groups.map((group, index) => <motion.div className="r-group-album-frame" layout="position" layoutDependency={`${columns}:${index}:${reducedMotion}`} initial={false} key={`${group.id}-${version}`}><GroupAlbum group={group} version={version} onOpen={onOpen} menuFor={menuFor} /></motion.div>)}<motion.div className="r-group-album-frame" layout="position" layoutDependency={`${columns}:${groups.length}:${reducedMotion}`} initial={false} key={`ungrouped-${version}`}><GroupAlbum version={version} onOpen={onOpen} /></motion.div></div>;
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
