import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import {
  importImages,
  undoImportBatch,
  updateExistingImages,
  type ImageImportResult,
  type ImageImportProgress,
} from "../api";
import { app, bumpDataVersion, errorText, formatCount, runAction, setNotice } from "./app-state.svelte";
import { clearHistory, recordHistory } from "./history.svelte";

export async function chooseImageFolder(): Promise<void> {
  let selection: unknown;
  try {
    selection = await open({
      directory: true,
      multiple: false,
      title: "选择要导入的图片文件夹（追加进资料库）",
    });
  } catch (error) {
    setNotice({ tone: "error", text: `无法打开文件夹选择器：${errorText(error)}` });
    return;
  }
  if (typeof selection !== "string") {
    return;
  }
  await runImageImport(selection);
}

export async function chooseImageArchive(): Promise<void> {
  let selection: unknown;
  try {
    selection = await open({
      multiple: false,
      directory: false,
      title: "选择要导入的压缩包（追加进资料库）",
      filters: [{ name: "压缩包", extensions: ["zip", "7z", "rar"] }],
    });
  } catch (error) {
    setNotice({ tone: "error", text: `无法打开文件选择器：${errorText(error)}` });
    return;
  }
  if (typeof selection !== "string") {
    return;
  }
  await runImageImport(selection);
}

export async function runImageImport(path: string): Promise<void> {
  await runAction(async () => {
    let batchId = 0;
    let initial: ImageImportResult;
    try {
      initial = await performImageImport(path, true);
    } catch (error) {
      // 用户主动取消不是失败：给一条温和的提示并正常返回
      if (errorText(error).includes("已被用户取消")) {
        setNotice({ tone: "success", text: "导入已取消，未写入任何数据。" });
        return;
      }
      throw error;
    }
    if (initial.added === 0) {
      return;
    }
    batchId = initial.batchId;
    recordHistory({
      label: `导入 ${formatCount(initial.added)} 张图片`,
      undo: async () => {
        const result = await undoImportBatch(batchId);
        app.snapshot = result.snapshot;
        bumpDataVersion({ preserveScroll: true });
      },
      redo: async () => {
        const result = await performImageImport(path, false);
        if (result.added !== initial.added) {
          const cleanup = await undoImportBatch(result.batchId);
          app.snapshot = cleanup.snapshot;
          bumpDataVersion({ preserveScroll: true });
          throw new Error(
            `来源已变化：原操作导入 ${formatCount(initial.added)} 张，本次只能导入 ${formatCount(result.added)} 张`,
          );
        }
        batchId = result.batchId;
      },
    });
  });
}

async function performImageImport(path: string, showResult: boolean): Promise<ImageImportResult> {
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
      bumpDataVersion();
    }
    if (showResult) {
      const parts = [`新增 ${formatCount(result.added)} 行`];
      if (result.skippedExisting > 0) {
        parts.push(`跳过 ${formatCount(result.skippedExisting)} 张已入库`);
      }
      if (result.skippedContent > 0) {
        parts.push(`内容重复跳过 ${formatCount(result.skippedContent)} 张`);
      }
      if (result.changedExisting > 0) {
        parts.push(`其中 ${formatCount(result.changedExisting)} 张源文件有变化（未改动库内数据）`);
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
      const ruleFailures = result.ruleExecution.reports.filter(report => report.error).length;
      if (result.ruleExecution.reports.length > 0) {
        const matched = result.ruleExecution.reports.reduce((sum, report) => sum + report.matchedRows, 0);
        parts.push(
          `自动规则共命中 ${formatCount(matched)} 次、修改 ${formatCount(result.ruleExecution.changedRows)} 张`,
        );
      }
      if (ruleFailures > 0 || result.ruleExecution.engineError) {
        parts.push(
          result.ruleExecution.engineError
            ? `自动规则引擎失败：${result.ruleExecution.engineError}`
            : `${formatCount(ruleFailures)} 条自动规则执行失败（图片已正常导入）`,
        );
      }
      // 含有任何失败/拒收计数时用 error 色调：error 通知不会自动消失，避免用户错过
      const hasFailures =
        ruleFailures > 0 ||
        Boolean(result.ruleExecution.engineError) ||
        result.metadataRejected > 0 ||
        result.rejectedMoveFailures > 0;
      setNotice({
        tone: hasFailures ? "error" : "success",
        text: `导入完成（共发现 ${formatCount(result.totalFound)} 张）：${parts.join("，")}。`,
      });
    }
    return result;
  } finally {
    unlisten();
    app.importProgress = null;
  }
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
        clearHistory();
        bumpDataVersion();
      }
      const parts = [`更新 ${formatCount(result.updated)} 张`];
      if (result.updated > 0) {
        parts.push("已清空撤销/重做记录");
      }
      if (result.relinkedByContent > 0) {
        parts.push(`按文件内容重新关联 ${formatCount(result.relinkedByContent)} 张`);
      }
      if (result.relinkedByMetadata > 0) {
        parts.push(`按完整元数据重新关联 ${formatCount(result.relinkedByMetadata)} 张`);
      }
      if (result.ambiguous > 0) {
        parts.push(`${formatCount(result.ambiguous)} 张匹配到多条记录（为安全起见未覆盖）`);
      }
      if (result.unmatched > 0) {
        parts.push(`忽略 ${formatCount(result.unmatched)} 张未入库图片（未新增）`);
      }
      if (result.metadataRejected > 0) {
        parts.push(`${formatCount(result.metadataRejected)} 张元数据读取失败（保留原数据）`);
      }
      if (result.copyFailures > 0) {
        parts.push(`${formatCount(result.copyFailures)} 张副本刷新失败（保留原数据）`);
      }
      const ruleFailures = result.ruleExecution.reports.filter(report => report.error).length;
      if (result.ruleExecution.reports.length > 0) {
        const matched = result.ruleExecution.reports.reduce((sum, report) => sum + report.matchedRows, 0);
        parts.push(
          `自动规则共命中 ${formatCount(matched)} 次、修改 ${formatCount(result.ruleExecution.changedRows)} 张`,
        );
      }
      if (ruleFailures > 0 || result.ruleExecution.engineError) {
        parts.push(
          result.ruleExecution.engineError
            ? `自动规则引擎失败：${result.ruleExecution.engineError}`
            : `${formatCount(ruleFailures)} 条自动规则执行失败（图片元数据已正常更新）`,
        );
      }
      const updateHasFailures =
        ruleFailures > 0 ||
        Boolean(result.ruleExecution.engineError) ||
        result.ambiguous > 0 ||
        result.metadataRejected > 0 ||
        result.copyFailures > 0;
      setNotice({
        tone: updateHasFailures ? "error" : "success",
        text: `更新完成（共发现 ${formatCount(result.totalFound)} 张，匹配 ${formatCount(result.matched)} 张）：${parts.join("，")}。`,
      });
    } finally {
      unlisten();
      app.importProgress = null;
    }
  });
}
