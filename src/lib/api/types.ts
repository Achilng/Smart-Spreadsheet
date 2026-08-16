export type SourceType = "legacy" | "folder" | "archive";

export type TagMatchMode = "and" | "or";
export type DedupeMode = "none" | "positivePrompt" | "artists" | "vibes";

export type FilterNumericOperator = "equal" | "notEqual" | "greaterThan" | "greaterOrEqual" | "lessThan" | "lessOrEqual" | "between";
export interface FilterNumericComparison {
  operator: FilterNumericOperator;
  value: number;
  secondValue: number | null;
}

export type LibraryFilter =
  | { type: "tag"; operator: "hasAll" | "hasAny" | "hasNone" | "isEmpty"; values: string[] }
  | { type: "group"; operator: "is" | "isNot" | "isEmpty"; groupId: number | null }
  | { type: "artist"; operator: "containsAny" | "containsNone" | "isSingle" | "isMultiple" | "isEmpty"; values: string[] }
  | { type: "prompt"; operator: "containsAll" | "containsAny" | "containsNone" | "isEmpty"; values: string[]; caseSensitive: boolean }
  | { type: "vibe"; operator: "hasAny" | "hasNone" | "count"; comparison: FilterNumericComparison | null }
  | { type: "note"; operator: "contains" | "isEmpty" | "isNotEmpty"; value: string; caseSensitive: boolean }
  | { type: "metadata"; parsed: boolean }
  | { type: "imageDimension"; field: "width" | "height" | "aspectRatio"; comparison: FilterNumericComparison }
  | { type: "orientation"; orientation: "landscape" | "portrait" | "square" }
  | { type: "generationText"; field: "model" | "sampler" | "noiseSchedule" | "seed"; operator: "contains" | "equals"; value: string; caseSensitive: boolean }
  | { type: "generationNumber"; field: "steps" | "scale" | "cfgRescale"; comparison: FilterNumericComparison };

export type RowSelection =
  | { kind: "explicit"; rowIds: number[] }
  | {
      kind: "filtered";
      tags: string[];
      tagMode: TagMatchMode;
      dedupe: DedupeMode;
      singleArtistOnly: boolean;
      artistFilter: string;
      hasVibe: boolean;
      untaggedOnly: boolean;
      filters: LibraryFilter[];
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
