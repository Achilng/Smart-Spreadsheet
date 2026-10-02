import { useCallback } from "react";
import type { DedupeCluster } from "../../../lib/api";
import { clusterLabel, loadClusterPreview, useDuplicates, type DuplicateMode } from "../../state/duplicates";
import { RowAlbumCard } from "../../ui/RowAlbumCard";
import type { MenuItem } from "../../ui/controls";

export function DuplicateAlbumShelf({ clusters, mode, version, onOpen, menuFor }: {
  clusters: DedupeCluster[]; mode: DuplicateMode; version: number; onOpen: (key: string) => void; menuFor: (cluster: DedupeCluster) => MenuItem[];
}) {
  return <div className="r-group-shelf" role="list" aria-label="重复项相册">{clusters.map(cluster => <DuplicateAlbum key={`${mode}-${version}-${cluster.key}`} cluster={cluster} mode={mode} version={version} onOpen={() => onOpen(cluster.key)} menu={menuFor(cluster)} />)}</div>;
}
function DuplicateAlbum({ cluster, mode, version, onOpen, menu }: { cluster: DedupeCluster; mode: DuplicateMode; version: number; onOpen: () => void; menu: MenuItem[] }) {
  const cached = useDuplicates(state => state.members[cluster.key]);
  const loadPreview = useCallback(() => loadClusterPreview(cluster.key), [cluster.key]);
  const label = clusterLabel(cluster, mode);
  return <RowAlbumCard albumKey={cluster.key} previewKey={`duplicates:${mode}:${version}:${cluster.key}`} label={label} count={cluster.memberCount} actionLabel={`打开重复项 ${label}`} loadPreview={loadPreview} initialPreview={cached?.rows.length ? cached : undefined} onOpen={onOpen} menu={menu} />;
}
