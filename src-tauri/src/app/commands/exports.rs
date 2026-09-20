use super::dto::{
    ExportProgressDto, ImageFilesExportResultDto, JsonExportNoteInspectionDto, JsonExportResultDto,
    PromptRotationJsonExportResultDto, XlsxExportResultDto,
};
use super::error_text;
use crate::app::AppRuntime;
use crate::db::{ImageExportSettings, RowSelection};
use crate::storage::JsonExportOptions;
use std::path::{Path, PathBuf};
use tauri::{Emitter, Manager, State};

#[tauri::command]
pub(crate) fn get_image_export_settings(
    runtime: State<'_, AppRuntime>,
) -> Result<ImageExportSettings, String> {
    runtime.image_export_settings().map_err(error_text)
}

#[tauri::command]
pub(crate) fn set_image_export_settings(
    settings: ImageExportSettings,
    runtime: State<'_, AppRuntime>,
) -> Result<(), String> {
    runtime
        .set_image_export_settings(&settings)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) async fn export_row_image(
    row_id: i64,
    destination: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_row_image(row_id, PathBuf::from(destination))
            .map_err(error_text)
    })
    .await
    .map_err(|e| format!("图片导出异常: {e}"))?
}

/// 导出带缩略图的 xlsx；在阻塞线程上执行，进度经 `export://progress` 推送。
#[tauri::command]
pub(crate) async fn export_xlsx(
    selection: RowSelection,
    path: String,
    app: tauri::AppHandle,
) -> Result<XlsxExportResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_xlsx(&selection, PathBuf::from(path), |progress| {
                emit_export_progress(&app, progress.processed, progress.total);
            })
            .map(|outcome| XlsxExportResultDto {
                path: outcome.destination.to_string_lossy().into_owned(),
                row_count: outcome.row_count,
                images_embedded: outcome.images_embedded,
                image_failures: outcome.image_failures,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("导出任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) fn inspect_zhihuiji_export_notes(
    selection: RowSelection,
    runtime: State<'_, AppRuntime>,
) -> Result<JsonExportNoteInspectionDto, String> {
    runtime
        .inspect_zhihuiji_export_notes(&selection)
        .map(|(total, empty_notes)| JsonExportNoteInspectionDto { total, empty_notes })
        .map_err(error_text)
}

#[tauri::command]
pub(crate) async fn export_zhihuiji_json(
    selection: RowSelection,
    path: String,
    options: JsonExportOptions,
    app: tauri::AppHandle,
) -> Result<JsonExportResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_zhihuiji_json(&selection, PathBuf::from(path), options, |progress| {
                emit_export_progress(&app, progress.processed, progress.total);
            })
            .map(|outcome| JsonExportResultDto {
                path: outcome.destination.to_string_lossy().into_owned(),
                exported: outcome.exported,
                duplicates_removed: outcome.duplicates_removed,
                artists_added: outcome.artists_added,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("导出任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) async fn export_prompt_rotation_json(
    selection: RowSelection,
    path: String,
    app: tauri::AppHandle,
) -> Result<PromptRotationJsonExportResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_prompt_rotation_json(&selection, PathBuf::from(path), |progress| {
                emit_export_progress(&app, progress.processed, progress.total);
            })
            .map(|outcome| PromptRotationJsonExportResultDto {
                path: outcome.destination.to_string_lossy().into_owned(),
                exported: outcome.exported,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("导出任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) async fn export_image_files(
    selection: RowSelection,
    parent_dir: String,
    mode: String,
    app: tauri::AppHandle,
) -> Result<ImageFilesExportResultDto, String> {
    let mode = crate::storage::ImageFileExportMode::parse(&mode)
        .ok_or_else(|| format!("未知的图片导出方式: {mode}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_image_files(&selection, PathBuf::from(parent_dir), mode, |progress| {
                emit_export_progress(&app, progress.processed, progress.total);
            })
            .map(|outcome| ImageFilesExportResultDto {
                directory: outcome.directory.to_string_lossy().into_owned(),
                exported: outcome.exported,
                hardlink_fallbacks: outcome.hardlink_fallbacks,
                missing: outcome.missing,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("导出任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) async fn export_selected_images(
    selection: RowSelection,
    source_paths: Vec<String>,
    parent_dir: String,
    rename_mode: String,
    custom_name: Option<String>,
    strip_metadata: bool,
    app: tauri::AppHandle,
) -> Result<ImageFilesExportResultDto, String> {
    let naming = match rename_mode.as_str() {
        "original" => crate::storage::ImageFileNaming::Original,
        "random" => crate::storage::ImageFileNaming::Random,
        "custom" => crate::storage::ImageFileNaming::Custom(custom_name.unwrap_or_default()),
        _ => return Err(format!("未知的图片重命名方式: {rename_mode}")),
    };
    tauri::async_runtime::spawn_blocking(move || {
        let extra_sources =
            crate::storage::collect_export_image_paths(source_paths.into_iter().map(PathBuf::from))
                .map_err(error_text)?;
        let runtime = app.state::<AppRuntime>();
        runtime
            .export_selected_images(
                &selection,
                &extra_sources,
                PathBuf::from(parent_dir),
                naming,
                strip_metadata,
                |progress| {
                    emit_export_progress(&app, progress.processed, progress.total);
                },
            )
            .map(|outcome| ImageFilesExportResultDto {
                directory: outcome.directory.to_string_lossy().into_owned(),
                exported: outcome.exported,
                hardlink_fallbacks: 0,
                missing: outcome.missing,
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("导出任务异常中止: {error}"))?
}

/// 工具箱导出入口：递归扫描图片或文件夹，返回自然排序、按完整路径去重后的图片。
#[tauri::command]
pub(crate) async fn collect_export_images(paths: Vec<String>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::storage::collect_export_image_paths(paths.into_iter().map(PathBuf::from))
            .map(|images| {
                images
                    .into_iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect()
            })
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("扫描图片文件夹任务异常中止: {error}"))?
}

fn emit_export_progress(app: &tauri::AppHandle, processed: usize, total: usize) {
    let _ = app.emit("export://progress", ExportProgressDto { processed, total });
}

/// 智绘姬 JSON 工具：检查重复项（只读，不修改文件）。
#[tauri::command]
pub(crate) async fn inspect_zhihuiji_json(
    path: String,
) -> Result<crate::pipeline::json_dedupe::JsonDedupeInspection, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::pipeline::json_dedupe::inspect_zhihuiji_json_file(Path::new(&path))
            .map_err(|error| format!("{error:#}"))
    })
    .await
    .map_err(|error| format!("检查任务异常中止: {error}"))?
}

/// 智绘姬 JSON 工具：去重并写出，进度经 `json-dedupe://progress` 推送。
#[tauri::command]
pub(crate) async fn dedupe_zhihuiji_json(
    input_path: String,
    output_path: String,
    app: tauri::AppHandle,
) -> Result<crate::pipeline::json_dedupe::JsonDedupeSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::pipeline::json_dedupe::dedupe_zhihuiji_json_file(
            Path::new(&input_path),
            Path::new(&output_path),
            |progress| {
                let _ = app.emit("json-dedupe://progress", progress);
            },
        )
        .map_err(|error| format!("{error:#}"))
    })
    .await
    .map_err(|error| format!("去重任务异常中止: {error}"))?
}
