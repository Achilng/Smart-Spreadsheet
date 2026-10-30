use super::error_text;
use crate::app::AppRuntime;
use crate::storage::SimilarImageMatch;
use std::path::PathBuf;
use tauri::Manager;
use tauri::ipc::Response;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowCharacterPrompts {
    character_prompt: String,
    characters: Vec<crate::pipeline::CharacterPrompt>,
}

#[tauri::command]
pub(crate) async fn get_row_character_prompts(
    row_id: i64,
    runtime: tauri::State<'_, AppRuntime>,
) -> Result<RowCharacterPrompts, String> {
    // Capture the library before dispatch so a library switch cannot redirect this read.
    let directory = runtime.active_directory().map_err(error_text)?;
    tauri::async_runtime::spawn_blocking(move || {
        let locator = directory.open_database().map_err(error_text)?
            .row_image_locator(row_id).map_err(error_text)?;
        let path = crate::storage::resolve_image_source(&directory, &locator)
            .ok_or_else(|| "原图和库内副本均不可用".to_string())?;
        let read = crate::pipeline::read_metadata::read_novelai_metadata(&path, None);
        if read.is_unreadable() {
            return Err(read.warnings.join("；"));
        }
        Ok(RowCharacterPrompts {
            character_prompt: crate::pipeline::parse_novelai_metadata(&read.chunks).character_prompt,
            characters: crate::pipeline::character_prompts(&read.chunks),
        })
    }).await.map_err(error_text)?
}

#[tauri::command]
pub(crate) async fn get_row_thumbnail(
    row_id: i64,
    app: tauri::AppHandle,
) -> Result<Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .row_thumbnail(row_id)
            .map(Response::new)
            .map_err(error_text)
    })
    .await
    .map_err(|e| format!("缩略图加载异常: {e}"))?
}

#[tauri::command]
pub(crate) async fn get_row_gallery_preview(
    row_id: i64,
    app: tauri::AppHandle,
) -> Result<Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .row_gallery_preview(row_id)
            .map(Response::new)
            .map_err(error_text)
    })
    .await
    .map_err(|e| format!("画廊高清图加载异常: {e}"))?
}

#[tauri::command]
pub(crate) async fn get_row_preview(
    row_id: i64,
    app: tauri::AppHandle,
) -> Result<Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .row_preview(row_id)
            .map(Response::new)
            .map_err(error_text)
    })
    .await
    .map_err(|e| format!("预览图加载异常: {e}"))?
}

#[tauri::command]
pub(crate) async fn get_row_original(
    row_id: i64,
    app: tauri::AppHandle,
) -> Result<Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .row_original(row_id)
            .map(Response::new)
            .map_err(error_text)
    })
    .await
    .map_err(|e| format!("原图加载异常: {e}"))?
}

/// 以图搜图：选择一张图片，返回库中相似的行。
#[tauri::command]
pub(crate) async fn search_similar_images(
    path: String,
    threshold: u32,
    app: tauri::AppHandle,
) -> Result<Vec<SimilarImageMatch>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .search_similar_images(PathBuf::from(path), threshold)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("以图搜图任务异常中止: {error}"))?
}

/// 行图片的 vibe 引用数：读取导入/升级时建立的元数据索引。
#[tauri::command]
pub(crate) async fn get_row_vibe_status(
    row_id: i64,
    app: tauri::AppHandle,
) -> Result<Option<u32>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .active_directory()
            .map_err(error_text)?
            .open_database()
            .map_err(error_text)?
            .row_vibe_reference_count(row_id)
            .map_err(error_text)
    })
    .await
    .map_err(|e| format!("vibe 状态读取异常: {e}"))?
}
