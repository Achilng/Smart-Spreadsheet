import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import {
  importImages,
  updateExistingImages,
  type ImageImportProgress,
} from "../api";
import { app, formatCount, runAction, setNotice } from "./app-state.svelte";

export async function chooseImageFolder(): Promise<void> {
  const selection = await open({
    directory: true,
    multiple: false,
    title: "选择要导入的图片文件夹（追加进资料库）",
  });
  if (typeof selection !== "string") {
    return;
  }
  await runImageImport(selection);
}

export async function chooseImageArchive(): Promise<void> {
  const selection = await open({
    multiple: false,
    directory: false,
    title: "选择要导入的压缩包（追加进资料库）",
    filters: [{ name: "压缩包", extensions: ["zip", "7z", "rar"] }],
  });
  if (typeof selection !== "string") {
    return;
  }
  await runImageImport(selection);
}

export async function runImageImport(path: string): Promise<void> {
  await runAction(async () => {
    const unlisten = await listen<ImageImportProgress>(
      "import-images://progress",
      event => {
        app.importProgress = event.payload;
      },
    );
    try {
      const result = await importImages(path);
      app.snapshot = result.snapshot;
      if (result.added > 0) {
        app.dataVersion += 1;
      }
      const parts = [`新增 ${formatCount(result.added)} 行`];
      if (result.skippedExisting > 0) {
        parts.push(`跳过 ${formatCount(result.skippedExisting)} 张已入库`);
      }
      if (result.skippedContent > 0) {
        parts.push(`内容重复跳过 ${formatCount(result.skippedContent)} 张`);
      }
      if (result.changedExisting > 0) {
        parts.push(
          `其中 ${formatCount(result.changedExisting)} 张源文件有变化（未改动库内数据）`,
        );
      }
      if (result.metadataRejected > 0) {
        parts.push(`${formatCount(result.metadataRejected)} 张无 metadata、不入库`);
      }
      if (result.rejectedMoved > 0) {
        parts.push(`${formatCount(result.rejectedMoved)} 张已移至异常图片目录`);
      }
      if (result.rejectedMoveFailures > 0) {
        parts.push(`${formatCount(result.rejectedMoveFailures)} 张移动失败（仍未入库）`);
      }
      setNotice({
        tone: "success",
        text: `导入完成（共发现 ${formatCount(result.totalFound)} 张）：${parts.join("，")}。`,
      });
    } finally {
      unlisten();
      app.importProgress = null;
    }
  });
}

export async function runExistingImageUpdate(path: string): Promise<void> {
  await runAction(async () => {
    const unlisten = await listen<ImageImportProgress>(
      "import-images://progress",
      event => {
        app.importProgress = event.payload;
      },
    );
    try {
      const result = await updateExistingImages(path);
      app.snapshot = result.snapshot;
      if (result.updated > 0) {
        app.dataVersion += 1;
      }
      const parts = [`更新 ${formatCount(result.updated)} 张`];
      if (result.unmatched > 0) {
        parts.push(`忽略 ${formatCount(result.unmatched)} 张未入库图片（未新增）`);
      }
      if (result.metadataRejected > 0) {
        parts.push(`${formatCount(result.metadataRejected)} 张元数据读取失败（保留原数据）`);
      }
      if (result.copyFailures > 0) {
        parts.push(`${formatCount(result.copyFailures)} 张副本刷新失败（保留原数据）`);
      }
      setNotice({
        tone: "success",
        text: `更新完成（共发现 ${formatCount(result.totalFound)} 张，匹配 ${formatCount(result.matched)} 张）：${parts.join("，")}。`,
      });
    } finally {
      unlisten();
      app.importProgress = null;
    }
  });
}
