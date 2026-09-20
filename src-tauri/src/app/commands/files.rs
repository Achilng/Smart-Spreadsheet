use super::dto::FileDragInfo;
use super::error_text;
use crate::app::AppRuntime;
use crate::db::RowSelection;
use std::path::{Path, PathBuf};
use tauri::State;

#[tauri::command]
pub(crate) fn show_item_in_explorer(
    row_id: i64,
    runtime: State<'_, AppRuntime>,
) -> Result<(), String> {
    let directory = runtime.active_directory().map_err(error_text)?;
    let locator = directory
        .open_database()
        .map_err(error_text)?
        .row_image_locator(row_id)
        .map_err(error_text)?;
    let source = crate::storage::resolve_image_source(&directory, &locator)
        .ok_or_else(|| format!("第 {row_id} 行没有可用的图片文件"))?;
    open_path_in_explorer(&source);
    Ok(())
}

#[tauri::command]
pub(crate) fn open_rejected_images_directory(runtime: State<'_, AppRuntime>) -> Result<(), String> {
    let directory = runtime.active_directory().map_err(error_text)?;
    let rejected_dir = directory
        .rejected_images_directory()
        .map_err(error_text)?
        .unwrap_or_else(|| directory.default_rejected_images_directory());
    if rejected_dir.is_dir() {
        open_path_in_explorer(&rejected_dir);
    } else {
        return Err(format!("失败图片目录不存在: {}", rejected_dir.display()));
    }
    Ok(())
}

pub(super) fn drag_row_ids(row_id: i64, mut selected_row_ids: Vec<i64>) -> Vec<i64> {
    if let Some(anchor_index) = selected_row_ids
        .iter()
        .position(|candidate| *candidate == row_id)
    {
        selected_row_ids.remove(anchor_index);
        selected_row_ids.insert(0, row_id);
        selected_row_ids
    } else {
        vec![row_id]
    }
}

#[tauri::command]
pub(crate) fn prepare_file_drag(
    row_id: i64,
    selection: Option<RowSelection>,
    runtime: State<'_, AppRuntime>,
) -> Result<FileDragInfo, String> {
    let selected_row_ids = match selection {
        Some(selection) => runtime.selected_row_ids(&selection).map_err(error_text)?,
        None => vec![row_id],
    };
    // 只有从选区成员开始拖动时才带出整个选区；拖动未选中图片不能误带旧选区。
    let row_ids = drag_row_ids(row_id, selected_row_ids);

    let directory = runtime.active_directory().map_err(error_text)?;
    let database = directory.open_database().map_err(error_text)?;
    // 拖出的文件会被下游（如 NovelAI）读取元数据，必须是完整原件，
    // 不能静默回退到不可信的历史缩略图副本。
    let file_paths = row_ids
        .iter()
        .map(|selected_row_id| {
            let locator = database
                .row_image_locator(*selected_row_id)
                .map_err(error_text)?;
            crate::storage::resolve_original_source(&directory, &locator)
                .map_err(|error| format!("第 {selected_row_id} 行无法拖出：{error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let thumb_dir = directory.thumbnail_cache_path();
    let thumbnail_prefix = format!("row-{row_id}-thumb-");
    let legacy_prefix = format!("row-{row_id}-");
    let find_icon = || -> Option<PathBuf> {
        let mut legacy = None;
        for entry in std::fs::read_dir(&thumb_dir).ok()?.filter_map(Result::ok) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&thumbnail_prefix) {
                return Some(entry.path());
            }
            if name.starts_with(&legacy_prefix) {
                let suffix = name.strip_prefix(&legacy_prefix)?;
                if suffix.strip_suffix(".png").is_some_and(|hash| {
                    hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit())
                }) {
                    legacy = Some(entry.path());
                }
            }
        }
        legacy
    };
    let icon_path = find_icon().unwrap_or_else(|| {
        let _ = directory.load_row_image(row_id, crate::images::ImageVariant::Thumbnail);
        find_icon().unwrap_or_else(|| file_paths[0].clone())
    });

    Ok(FileDragInfo {
        file_paths: file_paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        icon_path: icon_path.to_string_lossy().into_owned(),
    })
}

fn open_path_in_explorer(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        if path.is_file() {
            let _ = std::process::Command::new("explorer")
                .arg("/select,")
                .arg(path)
                .spawn();
        } else {
            let _ = std::process::Command::new("explorer").arg(path).spawn();
        }
    }
    #[cfg(target_os = "macos")]
    {
        if path.is_file() {
            let _ = std::process::Command::new("open")
                .arg("-R")
                .arg(path)
                .spawn();
        } else {
            let _ = std::process::Command::new("open").arg(path).spawn();
        }
    }
    #[cfg(target_os = "linux")]
    {
        let target = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}
