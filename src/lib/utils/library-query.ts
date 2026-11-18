import type { RowQuery } from "../api/rows";
import { cloneLibraryFilters } from "./library-filters";

export type LibraryQueryFilters = Pick<RowQuery, "tags" | "tagMode" | "dedupe" | "singleArtistOnly" | "artistFilter" | "hasVibe" | "untaggedOnly" | "filters" | "search">;

/** Copy shared filters once; callers add their pagination, view and selection scope. */
export function snapshotQueryFilters(source: LibraryQueryFilters & Partial<Pick<RowQuery, "hideGrouped" | "groupView">>): LibraryQueryFilters {
  const filters = cloneLibraryFilters(source.filters);
  if (source.hideGrouped && !source.groupView) filters.push({ type: "group", operator: "isEmpty", groupId: null });
  return {
    tags: [...source.tags],
    tagMode: source.tagMode,
    dedupe: source.dedupe,
    singleArtistOnly: source.singleArtistOnly,
    artistFilter: source.artistFilter,
    hasVibe: source.hasVibe,
    untaggedOnly: source.untaggedOnly,
    filters,
    search: source.search,
  };
}
