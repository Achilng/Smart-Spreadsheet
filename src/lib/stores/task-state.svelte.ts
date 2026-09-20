import type { ContentHashProgress, ExportProgress, ImageImportProgress, MigrationProgress, PerceptualHashProgress, StyleSignatureProgress, VibeStatusProgress } from "../api";

/** Progress of tasks owned by this window. */
export const taskState = $state({
  busy: false,
  /** 文件夹/压缩包导入进行中的进度，空闲时为 null */
  importProgress: null as ImageImportProgress | null,
  /** 当前追加导入会在写库后自动运行库内画师前缀检查。 */
  autoArtistPrefixImportActive: false,
  /** 打开旧目录时历史图片内容哈希补算进度，空闲时为 null */
  hashProgress: null as ContentHashProgress | null,
  /** 导出（xlsx / JSON / 图片文件）进行中的进度，空闲时为 null */
  exportProgress: null as ExportProgress | null,
  /** 感知哈希补算进度，空闲时为 null */
  phashProgress: null as PerceptualHashProgress | null,
  /** 升级后首启的 VIBE 索引回填进度，空闲时为 null */
  vibeBackfillProgress: null as VibeStatusProgress | null,
  /** 画风签名回填进度，空闲时为 null */
  styleSignatureProgress: null as StyleSignatureProgress | null,
  /** 数据目录迁移的分阶段进度，空闲时为 null */
  migrationProgress: null as MigrationProgress | null
});
