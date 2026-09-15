<script lang="ts">
  import type { RowRecord } from "../../api";
  import { showContextMenu } from "../../stores/context-menu.svelte";
  import { beginFileDrag } from "../../stores/file-drag";
  import { rowStore } from "../../stores/row-store.svelte";
  import { materialGalleryPicker } from "../../stores/material-gallery-picker.svelte";
  import { getSelectedCount, isRowSelected, modifierSelect, toggleRow } from "../../stores/selection-store.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import GalleryTile from "./GalleryTile.svelte";
  import { rowFileName, rowResolution } from "../../utils/row-display";
  import { modelVersionBadge } from "../../utils/model-version";
  import { vibeStatuses } from "../../images/vibe-statuses";

  let {
    row,
    index,
    x,
    y,
    width,
    imageHeight,
    enhance = false,
  }: {
    row: RowRecord | undefined;
    index: number;
    x: number;
    y: number;
    width: number;
    imageHeight: number;
    enhance?: boolean;
  } = $props();

  const hasImage = $derived(
    Boolean(row && (row.imagePath?.trim() || row.storedImagePath?.trim())),
  );
  const isActive = $derived(row != null && (materialGalleryPicker.active ? materialGalleryPicker.selected?.id : rowStore.activeRow?.id) === row.id);
  const isChecked = $derived(!materialGalleryPicker.active && row != null && isRowSelected(row.id));
  const selectionActive = $derived(!materialGalleryPicker.active && getSelectedCount() > 0);

  const fileName = $derived(row ? rowFileName(row) : null);
  const resolution = $derived(row ? rowResolution(row) : null);
  const versionBadge = $derived(
    row ? modelVersionBadge(row.generationModel) : null,
  );

  let dragging = $state(false);
  let vibeRefs = $state<number | null>(null);

  $effect(() => {
    vibeRefs = null;
    const current = row;
    if (!current || !hasImage) {
      return;
    }
    let cancelled = false;
    void vibeStatuses.load(current.id).then(
      count => {
        if (!cancelled) {
          vibeRefs = count;
        }
      },
      () => {},
    );
    return () => {
      cancelled = true;
    };
  });

  function onContextMenu(event: MouseEvent): void {
    if (!row) return;
    event.preventDefault();
    rowStore.activeRow = row;
    showContextMenu(row, event.clientX, event.clientY);
  }

  function onThumbMouseDown(event: MouseEvent): void {
    if (!row || !hasImage || materialGalleryPicker.active) return;
    beginFileDrag(
      event,
      row.id,
      () => { dragging = true; },
      // 外拖结束后主动复位，避免下一次点击被吞（死点击）
      () => { dragging = false; },
    );
  }
</script>

<GalleryTile {x} {y} {width} {imageHeight} {isActive} {isChecked} {selectionActive}
  skeleton={!row} showCheckbox={!!row && !materialGalleryPicker.active}
  selectionLabel={`选择第 ${row?.sourceOrdinal} 行`}
  label={`查看第 ${row?.sourceOrdinal} 行详情`}
  title={fileName ?? `#${row?.sourceOrdinal}`} subtitle={resolution ?? `#${row?.sourceOrdinal}`}
  tags={row?.tags ?? []} oncontextmenu={onContextMenu}
  oncheck={event => { if (row) toggleRow(row.id, index, event.shiftKey); }}
  onmousedowncapture={event => { if (materialGalleryPicker.active) event.stopPropagation(); }}
  onmousedown={onThumbMouseDown}
  onclick={event => {
    if (dragging) { dragging = false; return; }
    const current = row;
    if (!current) return;
    if (materialGalleryPicker.active) {
      materialGalleryPicker.selected = current;
      rowStore.activeRow = current;
      return;
    }
    if (modifierSelect(current.id, index, event)) return;
    rowStore.activeRow = current;
  }}>
  {#snippet image()}
    {#if row}<Thumbnail rowId={row.id} {hasImage} alt={`第 ${row.sourceOrdinal} 行缩略图`} {enhance} highPriority={isActive} />{/if}
  {/snippet}
  {#snippet upperLeft()}
    {#if versionBadge}<span class="version-badge {versionBadge.className}" title={"作画模型：" + (row?.generationModel ?? "")}>{versionBadge.label}</span>{/if}
  {/snippet}
  {#snippet upperRight()}
    {#if vibeRefs}<span class="vibe-badge" title={`原图元数据包含 ${vibeRefs} 个 vibe 引用`}>VIBE ×{vibeRefs}</span>{/if}
  {/snippet}
</GalleryTile>
