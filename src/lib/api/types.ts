export type SourceType = "xlsx" | "folder" | "archive";

export type TagMatchMode = "and" | "or";
export type DedupeMode = "none" | "positivePrompt" | "artists";

export type RowSelection =
  | { kind: "explicit"; rowIds: number[] }
  | {
      kind: "filtered";
      tags: string[];
      tagMode: TagMatchMode;
      dedupe: DedupeMode;
      singleArtistOnly: boolean;
      search: string;
      excludedRowIds: number[];
    };

// ── Dedupe Clusters ────────────────────────────────────────────
// 该类型由 duplicates.ts（真正的重复聚合）与 albums.ts（画师画册，复用同一结构）共用。

export interface DedupeCluster {
  key: string;
  memberCount: number;
  alias: string | null;
}
