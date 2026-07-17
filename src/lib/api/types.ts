export type SourceType = "legacy" | "folder" | "archive";

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
// 重复项聚合结构。

export interface DedupeCluster {
  key: string;
  memberCount: number;
  alias: string | null;
}
