import { invoke } from "@tauri-apps/api/core";

import type { RowPage } from "./rows";
import type { DedupeCluster, DedupeMode, TagMatchMode } from "./types";

export function listDedupeClusters(
  dedupe: DedupeMode,
  tags: string[],
  tagMode: TagMatchMode,
  singleArtistOnly: boolean,
  hasVibe: boolean,
  untaggedOnly: boolean,
  hideGrouped: boolean,
): Promise<DedupeCluster[]> {
  return invoke<DedupeCluster[]>("list_dedupe_clusters", {
    dedupe,
    tags,
    tagMode,
    singleArtistOnly,
    hasVibe,
    untaggedOnly,
    hideGrouped,
  });
}

export function getDedupeClusterMembers(
  dedupe: DedupeMode,
  key: string,
  tags: string[],
  tagMode: TagMatchMode,
  singleArtistOnly: boolean,
  hasVibe: boolean,
  untaggedOnly: boolean,
  hideGrouped: boolean,
  offset: number,
  limit: number,
): Promise<RowPage> {
  return invoke<RowPage>("get_dedupe_cluster_members", {
    dedupe,
    key,
    tags,
    tagMode,
    singleArtistOnly,
    hasVibe,
    untaggedOnly,
    hideGrouped,
    offset,
    limit,
  });
}

export function setDedupeAlias(
  dedupe: DedupeMode,
  key: string,
  alias: string,
): Promise<void> {
  return invoke<void>("set_dedupe_alias", { dedupe, key, alias });
}
