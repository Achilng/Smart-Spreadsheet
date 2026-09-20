import { listen } from "@tauri-apps/api/event";
import { backfillPerceptualHashes, backfillStyleSignatures, backfillVibeStatuses, type PerceptualHashProgress, type StyleSignatureProgress, type VibeStatusProgress } from "../../api";
import { libraryState } from "../../stores/library-state.svelte";
import { taskState } from "../../stores/task-state.svelte";
import { runAction } from "../../stores/tasks";
import { setNotice } from "../../stores/notices.svelte";
import { bumpDataVersion } from "../../stores/library-changes";
import { errorText, formatCount } from "../../utils/format";

let vibeBackfillActive = false;

/**
 * 在后台补齐历史图片的 VIBE 数量与组合签名（升级后首启一次性工作）。
 * 无待补行时后端只做一次查询立即返回，因此启动和换库后都可以放心调用；
 * 不占用 taskState.busy，进度显示在右下角，完成且确实有补齐时刷新数据视图
 * 并给出一次性说明。失败不阻塞使用，下次启动自动重试剩余行。
 */
export async function runVibeBackfill(): Promise<void> {
  if (vibeBackfillActive) return;
  if (!libraryState.snapshot?.dataDirectory || libraryState.snapshot.startupError) return;
  vibeBackfillActive = true;
  try {
    const unlisten = await listen<VibeStatusProgress>("vibe-status://progress", event => {
      taskState.vibeBackfillProgress = event.payload;
    });
    try {
      const result = await backfillVibeStatuses();
      if (result.total > 0) {
        bumpDataVersion({ preserveScroll: true, preserveSelection: true });
        setNotice({
          tone: "success",
          text: `已为 ${formatCount(result.total)} 张历史图片补齐 VIBE 聚合索引与作画模型信息${result.unreadable > 0 ? `（${formatCount(result.unreadable)} 张原图不可读，已跳过）` : ""}，重复视图现在可以按 VIBE 分组，预览图左上角会显示模型版本徽章。`,
        });
      }
    } finally {
      unlisten();
      taskState.vibeBackfillProgress = null;
    }
  } catch (error) {
    setNotice({ tone: "error", text: `VIBE 聚合索引建立失败：${errorText(error)}` });
  } finally {
    vibeBackfillActive = false;
  }
}

let styleSignatureBackfillActive = false;

/**
 * 在后台补齐历史图片的画风签名（对比窗口“相同画风”分区的依据）。
 * 纯 SQL 读算写、数万行秒级完成；已就绪（版本一致）时后端立即返回。
 * 失败不阻塞使用，下次启动自动续跑。
 */
export async function runStyleSignatureBackfill(): Promise<void> {
  if (styleSignatureBackfillActive) return;
  if (!libraryState.snapshot?.dataDirectory || libraryState.snapshot.startupError) return;
  styleSignatureBackfillActive = true;
  try {
    const unlisten = await listen<StyleSignatureProgress>(
      "style-signature://progress",
      event => {
        taskState.styleSignatureProgress = event.payload;
      },
    );
    try {
      const result = await backfillStyleSignatures();
      if (result.total > 0) {
        bumpDataVersion({ preserveScroll: true, preserveSelection: true });
        setNotice({
          tone: "success",
          text: `已为 ${formatCount(result.total)} 张历史图片补齐画风签名，图片对比窗口现在可以按“相同画风”找图。`,
        });
      }
    } finally {
      unlisten();
      taskState.styleSignatureProgress = null;
    }
  } catch (error) {
    setNotice({ tone: "error", text: `画风签名建立失败：${errorText(error)}` });
  } finally {
    styleSignatureBackfillActive = false;
  }
}

export async function runPhashBackfill(): Promise<void> {
  await runAction(async () => {
    const unlisten = await listen<PerceptualHashProgress>(
      "perceptual-hash://progress",
      event => {
        taskState.phashProgress = event.payload;
      },
    );
    try {
      const result = await backfillPerceptualHashes();
      if (result.total === 0) {
        setNotice({ tone: "success", text: "所有图片的感知哈希已是最新。" });
      } else {
        setNotice({
          tone: "success",
          text: `感知哈希更新完成：共 ${formatCount(result.total)} 张，成功 ${formatCount(result.updated)} 张${result.unreadable > 0 ? `，${formatCount(result.unreadable)} 张不可读` : ""}。`,
        });
      }
    } finally {
      unlisten();
      taskState.phashProgress = null;
    }
  });
}
