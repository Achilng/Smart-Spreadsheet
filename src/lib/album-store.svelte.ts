import { listArtistAlbums, type DedupeCluster } from "../api";
import { app, errorText } from "./app-state.svelte";
import { sectionMenu } from "./section-context-menu.svelte";

/**
 * 画册视图的模块级状态：画册列表、打开的画册和每本的阅读进度跨视图切换保留。
 * 列表按 dataVersion + 别名版本失效；阅读进度只在数据变化时可能越界，由阅读器钳位。
 */
export const albumBrowse = $state({
  albums: [] as DedupeCluster[],
  loading: true,
  error: null as string | null,
  sortByCount: true,
  /** 当前打开的画册 key；null = 画册列表 */
  openAlbumKey: null as string | null,
});

/** 每本画册上次阅读到的图片下标 */
const readerPositions = new Map<string, number>();

export function savedAlbumIndex(key: string): number {
  return readerPositions.get(key) ?? 0;
}

export function saveAlbumIndex(key: string, index: number): void {
  readerPositions.set(key, index);
}

let signature = "\u{0}unloaded";
let loadGeneration = 0;

/** 数据/别名变化时重载画册列表，其余情况直接复用缓存。 */
export function syncAlbums(): void {
  const next = `${app.dataVersion}\u{1}${sectionMenu.aliasVersion}`;
  if (next !== signature) {
    signature = next;
    void loadAlbums();
  }
}

async function loadAlbums(): Promise<void> {
  const generation = ++loadGeneration;
  // 有旧内容时保持渲染，后台替换
  albumBrowse.loading = albumBrowse.albums.length === 0;
  albumBrowse.error = null;
  try {
    const albums = await listArtistAlbums();
    if (generation !== loadGeneration) {
      return;
    }
    albumBrowse.albums = albums;
    // 列表刷新后，若打开的画册已不存在则退回列表
    if (albumBrowse.openAlbumKey && !albums.some(album => album.key === albumBrowse.openAlbumKey)) {
      albumBrowse.openAlbumKey = null;
    }
  } catch (e) {
    if (generation !== loadGeneration) {
      return;
    }
    albumBrowse.error = errorText(e);
    albumBrowse.albums = [];
  } finally {
    if (generation === loadGeneration) {
      albumBrowse.loading = false;
    }
  }
}
