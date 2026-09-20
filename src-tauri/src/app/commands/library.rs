use super::dto::{AppSnapshotDto, MigrationResultDto};
use super::error_text;
use crate::app::AppRuntime;
use crate::storage::MigrationStage;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager, State};

#[tauri::command]
pub(crate) fn get_app_snapshot(runtime: State<'_, AppRuntime>) -> Result<AppSnapshotDto, String> {
    runtime
        .snapshot()
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn reset_configuration(
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .reset_configuration()
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) async fn reset_data(app: tauri::AppHandle) -> Result<AppSnapshotDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .reset_data()
            .map(AppSnapshotDto::from)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("重置任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn initialize_data_directory(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .initialize_directory(PathBuf::from(path))
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

/// 打开已有受管目录可能需要为历史行补算内容哈希，在阻塞线程执行并上报进度。
#[tauri::command]
pub(crate) async fn open_data_directory(
    path: String,
    app: tauri::AppHandle,
) -> Result<AppSnapshotDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .open_directory(PathBuf::from(path), |progress| {
                let _ = app.emit("content-hash://progress", progress);
            })
            .map(AppSnapshotDto::from)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("打开数据目录任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn set_rejected_images_directory(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .set_rejected_images_directory(PathBuf::from(path))
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn set_auto_artist_prefix_on_import(
    enabled: bool,
    runtime: State<'_, AppRuntime>,
) -> Result<AppSnapshotDto, String> {
    runtime
        .set_auto_artist_prefix_on_import(enabled)
        .map(AppSnapshotDto::from)
        .map_err(error_text)
}

/// 迁移会整目录复制多 GB 数据，放阻塞线程执行避免拖住 UI 事件循环。
#[tauri::command]
pub(crate) async fn migrate_data_directory(
    path: String,
    app: tauri::AppHandle,
) -> Result<MigrationResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        let mut last_stage: Option<MigrationStage> = None;
        let mut last_emit: Option<Instant> = None;
        runtime
            .migrate_directory_with_progress(PathBuf::from(path), |progress| {
                let now = Instant::now();
                let stage_changed = last_stage != Some(progress.stage);
                let due = last_emit
                    .is_none_or(|last| now.duration_since(last) >= Duration::from_millis(100));
                let finished = progress.total > 0 && progress.completed >= progress.total;
                if stage_changed || due || finished {
                    let _ = app.emit("migration://progress", progress);
                    last_stage = Some(progress.stage);
                    last_emit = Some(now);
                }
            })
            .map(|outcome| MigrationResultDto {
                snapshot: outcome.snapshot.into(),
                retired_source: outcome
                    .retired_source
                    .map(|path| path.to_string_lossy().into_owned()),
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("迁移任务异常中止: {error}"))?
}
