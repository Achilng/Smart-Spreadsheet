use super::error_text;
use crate::app::AppRuntime;
use crate::storage::{PromptDocAsset, PromptDocDetail, PromptDocSummary};
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub(crate) fn list_prompt_docs(
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<PromptDocSummary>, String> {
    runtime.list_prompt_docs().map_err(error_text)
}

#[tauri::command]
pub(crate) fn create_prompt_doc(
    title: String,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptDocDetail, String> {
    runtime.create_prompt_doc(&title).map_err(error_text)
}

#[tauri::command]
pub(crate) fn load_prompt_doc(
    doc_id: String,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptDocDetail, String> {
    runtime.load_prompt_doc(&doc_id).map_err(error_text)
}

#[tauri::command]
pub(crate) fn save_prompt_doc(
    doc_id: String,
    title: String,
    content: serde_json::Value,
    plain_text: String,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptDocDetail, String> {
    runtime
        .save_prompt_doc(&doc_id, &title, &content, &plain_text)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn delete_prompt_doc(
    doc_id: String,
    runtime: State<'_, AppRuntime>,
) -> Result<(), String> {
    runtime.delete_prompt_doc(&doc_id).map_err(error_text)
}

#[tauri::command]
pub(crate) fn import_prompt_doc_image_from_path(
    doc_id: String,
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptDocAsset, String> {
    runtime
        .import_prompt_doc_image_from_path(&doc_id, PathBuf::from(path))
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn import_prompt_doc_image_bytes(
    doc_id: String,
    file_name: String,
    bytes: Vec<u8>,
    runtime: State<'_, AppRuntime>,
) -> Result<PromptDocAsset, String> {
    runtime
        .import_prompt_doc_image_bytes(&doc_id, &file_name, &bytes)
        .map_err(error_text)
}
