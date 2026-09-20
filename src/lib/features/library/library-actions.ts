import { listen } from "@tauri-apps/api/event";
import { confirm as confirmDialog, open } from "@tauri-apps/plugin-dialog";
import { getAppSnapshot, initializeDataDirectory, migrateDataDirectory, openDataDirectory, resetConfiguration, resetData as apiResetData, type ContentHashProgress, type MigrationProgress } from "../../api";
import { libraryState } from "../../stores/library-state.svelte";
import { taskState } from "../../stores/task-state.svelte";
import { runAction } from "../../stores/tasks";
import { setNotice } from "../../stores/notices.svelte";
import { clearOperationHistory } from "../../stores/history-context";
import { bumpDataVersion } from "../../stores/library-changes";
import { notifyMainStateChanged, notifyCompareLibraryReset } from "../../windows/library-events";
import { errorText } from "../../utils/format";
import { runVibeBackfill, runStyleSignatureBackfill } from "./maintenance";

export async function refreshSnapshot(): Promise<void> {
  taskState.busy = true;
  try {
    libraryState.snapshot = await getAppSnapshot();
  } catch (error) {
    setNotice({ tone: "error", text: errorText(error) });
  } finally {
    taskState.busy = false;
    libraryState.loaded = true;
  }
}

export async function resetAndReconfigure(): Promise<void> {
  await runAction(async () => {
    libraryState.snapshot = await resetConfiguration();
    clearOperationHistory();
  });
}

export async function resetDataWithConfirmation(): Promise<void> {
  const confirmed = await confirmDialog(
    "将清空所有已导入的数据（图片副本、缩略图、数据库），回到初始导入页面。原始图片文件不受影响。此操作不可撤销，是否继续？",
    { title: "重置表格", kind: "warning", okLabel: "确认重置", cancelLabel: "取消" },
  );
  if (!confirmed) return;
  await runAction(async () => {
    libraryState.snapshot = await apiResetData();
    clearOperationHistory();
    bumpDataVersion();
    notifyCompareLibraryReset();
    await notifyMainStateChanged("reset");
    setNotice({ tone: "success", text: "表格已重置，请重新导入数据。" });
  });
}

export async function chooseDirectory(mode: "initialize" | "open"): Promise<void> {
  const selection = await open({
    directory: true,
    multiple: false,
    title: mode === "initialize" ? "选择空的数据目录" : "打开智能表格数据目录",
  });
  if (typeof selection !== "string") {
    return;
  }
  await runAction(async () => {
    if (mode === "initialize") {
      libraryState.snapshot = await initializeDataDirectory(selection);
    } else {
      const unlisten = await listen<ContentHashProgress>("content-hash://progress", event => {
        taskState.hashProgress = event.payload;
      });
      try {
        libraryState.snapshot = await openDataDirectory(selection);
      } finally {
        unlisten();
        taskState.hashProgress = null;
      }
      // 换到的旧库可能还没有 VIBE 聚合索引，后台补齐（已就绪时立即返回）。
      void runVibeBackfill().then(() => runStyleSignatureBackfill());
    }
    clearOperationHistory();
    notifyCompareLibraryReset();
    setNotice({ tone: "success", text: "数据目录已连接。" });
  });
}

export async function chooseMigration(): Promise<void> {
  if (!libraryState.snapshot?.dataDirectory) {
    return;
  }
  const confirmed = await confirmDialog(
    "迁移会复制并校验数据库、工作簿和缓存，切换成功后再清理旧目录。目标必须是空文件夹；失败时应用继续使用当前目录。是否继续？",
    { title: "迁移数据目录", kind: "warning", okLabel: "继续", cancelLabel: "取消" },
  );
  if (!confirmed) {
    return;
  }
  const selection = await open({
    directory: true,
    multiple: false,
    title: "选择空的数据迁移目标目录",
  });
  if (typeof selection !== "string") {
    return;
  }
  await runAction(async () => {
    const unlisten = await listen<MigrationProgress>("migration://progress", event => {
      taskState.migrationProgress = event.payload;
    });
    taskState.migrationProgress = {
      stage: "preparing",
      completed: 0,
      total: 0,
      stageCompleted: 0,
      stageTotal: 0,
    };
    try {
      const result = await migrateDataDirectory(selection);
      libraryState.snapshot = result.snapshot;
      clearOperationHistory();
      notifyCompareLibraryReset();
      await notifyMainStateChanged("migrated");
      setNotice(
        result.retiredSource
          ? { tone: "error", text: `迁移成功，但旧目录未能自动清理：${result.retiredSource}` }
          : { tone: "success", text: `数据目录已迁移到 ${selection}` },
      );
    } finally {
      unlisten();
      taskState.migrationProgress = null;
    }
  });
}
