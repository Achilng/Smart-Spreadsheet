use super::error_text;
use crate::app::AppRuntime;
use crate::storage::{PerceptualHashProgress, StyleSignatureProgress, VibeStatusProgress};
use tauri::{Emitter, Manager};

/// 手动刷新感知哈希：为库中缺少 pHash 的行补算。
#[tauri::command]
pub(crate) async fn backfill_perceptual_hashes(
    app: tauri::AppHandle,
) -> Result<PerceptualHashProgress, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .backfill_perceptual_hashes(|progress| {
                let _ = app.emit("perceptual-hash://progress", progress);
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("感知哈希计算任务异常中止: {error}"))?
}

/// 升级后首启为历史图片补齐 VIBE 数量与组合签名；逐行读原图元数据，
/// 在阻塞线程执行并上报进度。无待补行时立即返回 total = 0。
#[tauri::command]
pub(crate) async fn backfill_vibe_statuses(
    app: tauri::AppHandle,
) -> Result<VibeStatusProgress, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .backfill_vibe_statuses(|progress| {
                let _ = app.emit("vibe-status://progress", progress);
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("VIBE 索引回填任务异常中止: {error}"))?
}

/// 为历史图片补齐画风签名（正向提示词归一化哈希）；算法版本落后时全量
/// 重算。纯 SQL 读算写，不读图片文件；无待补行时立即返回 total = 0。
#[tauri::command]
pub(crate) async fn backfill_style_signatures(
    app: tauri::AppHandle,
) -> Result<StyleSignatureProgress, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .backfill_style_signatures(|progress| {
                let _ = app.emit("style-signature://progress", progress);
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("画风签名回填任务异常中止: {error}"))?
}
