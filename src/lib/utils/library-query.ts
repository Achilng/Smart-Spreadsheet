import type { RowQuery } from "../api/rows";
import { cloneLibraryFilters } from "./library-filters";

export type LibraryQueryFilters = Pick<RowQuery, "tags" | "tagMode" | "dedupe" | "singleArtistOnly" | "artistFilter" | "hasVibe" | "untaggedOnly" | "filters" | "search">;

/** Copy shared filters once; callers add their pagination, view and selection scope. */
export function snapshotQueryFilters(source: LibraryQueryFilters): LibraryQueryFilters {
  return {
    tags: [...source.tags],
    tagMode: source.tagMode,
    dedupe: source.dedupe,
    singleArtistOnly: source.singleArtistOnly,
    artistFilter: source.artistFilter,
    hasVibe: source.hasVibe,
    untaggedOnly: source.untaggedOnly,
    filters: cloneLibraryFilters(source.filters),
    search: source.search,
  };
}
