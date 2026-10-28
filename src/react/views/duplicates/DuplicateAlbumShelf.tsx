import { memo, useCallback } from "react";
import type { DedupeCluster } from "../../../lib/api";
import { clusterLabel, loadClusterPreview, useDuplicates, type DuplicateMode } from "../../state/duplicates";
import { RowAlbumCard } from "../../ui/RowAlbumCard";
import { AlbumShelf } from "../../ui/AlbumShelf";
import type { MenuItem } from "../../ui/controls";

export const DuplicateAlbumShelf = memo(function DuplicateAlbumShelf({ clusters, mode, version, onOpen, menuFor }: {
  clusters: DedupeCluster[]; mode: DuplicateMode; version: number; onOpen: (key: string) => void; menuFor: (cluster: DedupeCluster) => MenuItem[];
}) {
  return <AlbumShelf label="重复项相册" count={clusters.length} itemKey={index => `${mode}-${version}-${clusters[index].key}`} render={index => {
    const cluster = clusters[index];
    return <DuplicateAlbum cluster={cluster} mode={mode} version={version} onOpen={() => onOpen(cluster.key)} menu={menuFor(cluster)} />;
  }} />;
});
function DuplicateAlbum({ cluster, mode, version, onOpen, menu }: { cluster: DedupeCluster; mode: DuplicateMode; version: number; onOpen: () => void; menu: MenuItem[] }) {
  const cached = useDuplicates(state => state.members[cluster.key]);
  const loadPreview = useCallback(() => loadClusterPreview(cluster.key), [cluster.key]);
  const label = clusterLabel(cluster, mode);
  return <RowAlbumCard albumKey={cluster.key} previewKey={`duplicates:${mode}:${version}:${cluster.key}`} label={label} count={cluster.memberCount} actionLabel={`打开重复项 ${label}`} loadPreview={loadPreview} initialPreview={cached?.rows.length ? cached : undefined} onOpen={onOpen} menu={menu} />;
}
