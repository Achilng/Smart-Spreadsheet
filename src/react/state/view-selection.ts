import { getDedupeClusterMembers, getGroupMembers, listDedupeClusters, listGroups, queryRows, type RowPage } from "../../lib/api";
import { useDuplicates } from "./duplicates";
import { useLibrary, useRows } from "./library";
import { selectAllFiltered, setExplicitSelection, useSelection } from "./selection";
import { useWorkspace } from "./workspace";

/** Enumerate collapsed sections too: the selection must match the whole view. */
export async function selectAllCurrentView(): Promise<void> {
  const mode = useWorkspace.getState().viewMode;
  if (mode === "materials" || mode === "promptDocs") return;
  if (mode !== "group" && mode !== "duplicates") return selectAllFiltered();
  const signature = () => JSON.stringify([useWorkspace.getState().viewMode, useLibrary.getState().snapshot?.dataDirectory, useRows.getState().query, useRows.getState().resetToken, useDuplicates.getState().mode, useSelection.getState().version]);
  const before = signature(), query = structuredClone(useRows.getState().query), ids = new Set<number>();
  const append = async (fetch: (offset: number) => Promise<RowPage>) => {
    for (let offset = 0; ; offset += 500) {
      if (signature() !== before) return;
      const page = await fetch(offset);
      for (const row of page.rows) ids.add(row.id);
      if (!page.hasMore || !page.rows.length) return;
    }
  };
  if (mode === "group") {
    for (const group of await listGroups()) await append(offset => getGroupMembers(group.id, offset, 500));
    await append(offset => queryRows({ ...query, offset, limit: 500, dedupe: "none", groupView: false, hideGrouped: true, sort: "timeAsc" }));
  } else {
    const filters = [useDuplicates.getState().mode, query.tags, query.tagMode, query.singleArtistOnly, query.hasVibe, query.untaggedOnly, query.filters, query.hideGrouped] as const;
    const duplicates = useDuplicates.getState();
    const opened = duplicates.layout === "shelf" ? duplicates.expanded[0] : undefined;
    const clusters = opened !== undefined ? [{ key: opened }] : await listDedupeClusters(...filters);
    for (const cluster of clusters) {
      const [dedupe, ...rest] = filters;
      await append(offset => getDedupeClusterMembers(dedupe, cluster.key, ...rest, offset, 500));
    }
  }
  if (signature() === before) setExplicitSelection([...ids]);
}
