use super::AppRuntime;
use crate::{
    db::{
        TagSummary,
        materials::{Material, MaterialDraft, MaterialPage},
    },
    pipeline,
};
use image::{DynamicImage, ImageFormat, ImageReader};
use serde::Serialize;
use std::{collections::BTreeMap, io::Cursor, path::Path};
use tauri::{State, ipc::Response};

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[derive(Debug, Serialize)]
pub(crate) struct MetadataSection {
    id: String,
    label: String,
    text: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaterialInspection {
    title: String,
    preview: Vec<u8>,
    sections: Vec<MetadataSection>,
    warning: Option<String>,
}

fn encode(image: &DynamicImage, edge: u32) -> Result<Vec<u8>, String> {
    let mut out = Cursor::new(Vec::new());
    image
        .thumbnail(edge, edge)
        .write_to(&mut out, ImageFormat::Png)
        .map_err(text)?;
    Ok(out.into_inner())
}

fn decode(path: &Path) -> Result<DynamicImage, String> {
    let mut reader = ImageReader::open(path)
        .map_err(text)?
        .with_guessed_format()
        .map_err(text)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(text)
}

fn metadata_sections(chunks: &BTreeMap<String, String>) -> Vec<MetadataSection> {
    let parsed = pipeline::parse_novelai_metadata(chunks);
    let mut sections = Vec::new();
    let mut add = |id: String, label: String, value: String| {
        if !value.trim().is_empty() {
            sections.push(MetadataSection {
                id,
                label,
                text: value,
            });
        }
    };
    add(
        "positive".into(),
        "正向提示词".into(),
        parsed.positive_prompt,
    );
    add(
        "negative".into(),
        "负向提示词".into(),
        parsed.negative_prompt,
    );
    if let Some(comment) = chunks
        .get("Comment")
        .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
    {
        for (key, label) in [
            ("v4_prompt", "提示词"),
            ("v4_negative_prompt", "负向提示词"),
        ] {
            if let Some(characters) = comment
                .pointer(&format!("/{key}/caption/char_captions"))
                .and_then(|v| v.as_array())
            {
                for (index, character) in characters.iter().enumerate() {
                    if let Some(value) = character.get("char_caption").and_then(|v| v.as_str()) {
                        add(
                            format!("{key}-{index}"),
                            format!("角色 {} {label}", index + 1),
                            value.into(),
                        );
                    }
                }
            }
        }
    }
    if !parsed.character_prompt.trim().is_empty()
        && !sections.iter().any(|s| s.id.starts_with("v4_prompt-"))
    {
        sections.push(MetadataSection {
            id: "characters".into(),
            label: "角色提示词".into(),
            text: parsed.character_prompt,
        });
    }
    for (key, value) in chunks {
        if !value.trim().is_empty() {
            sections.push(MetadataSection {
                id: format!("raw-{key}"),
                label: format!("原始元数据 · {key}"),
                text: value.clone(),
            });
        }
    }
    sections
}

fn inspect(path: &Path) -> Result<MaterialInspection, String> {
    let image = decode(path)?;
    let mut warning = None;
    let mut chunks = BTreeMap::new();
    // Identify actual contents, not the extension. JPEG/WebP etc. remain usable as covers.
    if ImageReader::open(path)
        .map_err(text)?
        .with_guessed_format()
        .map_err(text)?
        .format()
        == Some(ImageFormat::Png)
    {
        match pipeline::png_text::read_png_text_chunks(path) {
            Ok(value) => chunks = value,
            Err(error) => {
                warning = Some(format!("PNG 元数据读取失败：{error}。仍可手动填写文本。"))
            }
        }
        match pipeline::stealth_png::read_stealth_png_metadata_from_rgba(&image.to_rgba8()) {
            Ok(Some(hidden)) => {
                for (key, value) in hidden {
                    chunks.entry(key).or_insert(value);
                }
            }
            Ok(None) => {}
            Err(error) => {
                warning = Some(format!("隐藏元数据读取失败：{error}。仍可手动填写文本。"))
            }
        }
    } else {
        warning =
            Some("此格式可作为展示图；目前元数据提取支持 PNG（含 NovelAI 隐藏元数据）。".into());
    }
    Ok(MaterialInspection {
        title: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        preview: encode(&image, 600)?,
        sections: metadata_sections(&chunks),
        warning,
    })
}

#[tauri::command]
pub(crate) async fn inspect_material_image(path: String) -> Result<MaterialInspection, String> {
    tauri::async_runtime::spawn_blocking(move || inspect(Path::new(&path)))
        .await
        .map_err(text)?
}

#[derive(Serialize)]
pub(crate) struct LibraryMaterialInspection {
    path: String,
    inspection: MaterialInspection,
}

fn inspect_library_image(
    directory: &crate::storage::DataDirectory,
    row_id: i64,
) -> Result<LibraryMaterialInspection, String> {
    let locator = directory
        .open_database()
        .map_err(text)?
        .row_image_locator(row_id)
        .map_err(text)?;
    let path = crate::storage::resolve_image_source(directory, &locator)
        .ok_or_else(|| "这张图片的原文件和库内副本均不可用，请选择其他图片。".to_string())?;
    let mut inspection = inspect(&path)?;
    if let Some(name) = locator
        .image_path
        .as_deref()
        .and_then(|value| Path::new(value).file_stem())
        .filter(|name| !name.is_empty())
    {
        inspection.title = name.to_string_lossy().into_owned();
    }
    Ok(LibraryMaterialInspection {
        path: path.to_string_lossy().into_owned(),
        inspection,
    })
}

#[tauri::command]
pub(crate) async fn inspect_material_library_image(
    runtime: State<'_, AppRuntime>,
    row_id: i64,
) -> Result<LibraryMaterialInspection, String> {
    let directory = runtime.active_directory().map_err(text)?;
    tauri::async_runtime::spawn_blocking(move || inspect_library_image(&directory, row_id))
        .await
        .map_err(text)?
}

#[tauri::command]
pub(crate) async fn list_materials(
    runtime: State<'_, AppRuntime>,
    search: String,
    tags: Vec<String>,
    untagged: bool,
    offset: u32,
) -> Result<MaterialPage, String> {
    let directory = runtime.active_directory().map_err(text)?;
    tauri::async_runtime::spawn_blocking(move || {
        directory
            .open_database()
            .map_err(text)?
            .list_materials(&search, &tags, untagged, offset)
            .map_err(text)
    })
    .await
    .map_err(text)?
}

#[tauri::command]
pub(crate) async fn material_tag_counts(
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<TagSummary>, String> {
    let directory = runtime.active_directory().map_err(text)?;
    tauri::async_runtime::spawn_blocking(move || {
        directory
            .open_database()
            .map_err(text)?
            .material_tag_counts()
            .map_err(text)
    })
    .await
    .map_err(text)?
}

#[tauri::command]
pub(crate) async fn material_ids_for_tag(
    runtime: State<'_, AppRuntime>,
    name: String,
) -> Result<Vec<i64>, String> {
    let directory = runtime.active_directory().map_err(text)?;
    tauri::async_runtime::spawn_blocking(move || {
        directory
            .open_database()
            .map_err(text)?
            .material_ids_for_tag(&name)
            .map_err(text)
    })
    .await
    .map_err(text)?
}

#[tauri::command]
pub(crate) async fn restore_material_tag(
    runtime: State<'_, AppRuntime>,
    name: String,
    ids: Vec<i64>,
) -> Result<(), String> {
    let directory = runtime.active_directory().map_err(text)?;
    tauri::async_runtime::spawn_blocking(move || {
        directory
            .open_database()
            .map_err(text)?
            .restore_material_tag(&name, &ids)
            .map_err(text)
    })
    .await
    .map_err(text)?
}

#[tauri::command]
pub(crate) async fn save_material(
    runtime: State<'_, AppRuntime>,
    draft: MaterialDraft,
) -> Result<Material, String> {
    let directory = runtime.active_directory().map_err(text)?;
    tauri::async_runtime::spawn_blocking(move || {
        if draft.id.is_none() && draft.image_path.is_none() {
            return Err("请选择素材展示图".into());
        }
        let images = draft
            .image_path
            .as_ref()
            .map(|path| -> Result<_, String> {
                let image = decode(Path::new(path))?;
                Ok((encode(&image, 1600)?, encode(&image, 360)?))
            })
            .transpose()?;
        directory
            .open_database()
            .map_err(text)?
            .save_material(
                &draft,
                images.as_ref().map(|(a, b)| (a.as_slice(), b.as_slice())),
            )
            .map_err(text)
    })
    .await
    .map_err(text)?
}

#[tauri::command]
pub(crate) async fn delete_material(runtime: State<'_, AppRuntime>, id: i64) -> Result<(), String> {
    let directory = runtime.active_directory().map_err(text)?;
    tauri::async_runtime::spawn_blocking(move || {
        directory
            .open_database()
            .map_err(text)?
            .delete_material(id)
            .map_err(text)
    })
    .await
    .map_err(text)?
}

#[tauri::command]
pub(crate) async fn material_image(
    runtime: State<'_, AppRuntime>,
    id: i64,
    thumbnail: bool,
) -> Result<Response, String> {
    let directory = runtime.active_directory().map_err(text)?;
    tauri::async_runtime::spawn_blocking(move || {
        directory
            .open_database()
            .map_err(text)?
            .material_image(id, thumbnail)
            .map(Response::new)
            .map_err(text)
    })
    .await
    .map_err(text)?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_selection_reads_metadata_and_falls_back_to_stored_copy_without_saving() {
        let folder = std::env::temp_dir().join(format!(
            "materials-library-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let directory = crate::storage::DataDirectory::initialize(&folder).unwrap();
        let original = folder.join("original.png");
        let stored = folder.join("files/1/stored.png");
        std::fs::create_dir_all(stored.parent().unwrap()).unwrap();
        let bytes = crate::storage::test_fixtures::metadata_png_bytes(
            "gold dress",
            Some(r#"{"v4_prompt":{"caption":{"char_captions":[{"char_caption":"silver hair"}]}}}"#),
        );
        std::fs::write(&original, &bytes).unwrap();
        std::fs::write(&stored, &bytes).unwrap();
        let mut db = directory.open_database().unwrap();
        db.append_batch(
            crate::db::SourceType::Folder,
            "fixture",
            &[crate::db::NewRow {
                source_ordinal: 1,
                identity: "material-library".into(),
                image_path: Some(original.to_string_lossy().into_owned()),
                stored_image_rel: Some("stored.png".into()),
                ..crate::db::NewRow::default()
            }],
            |_| Ok(()),
        )
        .unwrap();
        let result = inspect_library_image(&directory, 1).unwrap();
        assert_eq!(Path::new(&result.path), original);
        assert!(
            result
                .inspection
                .sections
                .iter()
                .any(|s| s.id == "v4_prompt-0" && s.text == "silver hair")
        );
        std::fs::rename(&original, folder.join("moved.png")).unwrap();
        let fallback = inspect_library_image(&directory, 1).unwrap();
        assert_eq!(Path::new(&fallback.path), stored);
        assert!(
            fallback
                .inspection
                .sections
                .iter()
                .any(|s| s.id == "positive" && s.text == "gold dress")
        );
        assert!(image::load_from_memory(&fallback.inspection.preview).is_ok());
        assert_eq!(db.list_materials("", &[], false, 0).unwrap().total, 0);
        assert_eq!(std::fs::read(&stored).unwrap(), bytes);
        assert!(inspect_library_image(&directory, 999).is_err());
        std::fs::rename(&stored, folder.join("moved-copy.png")).unwrap();
        assert!(inspect_library_image(&directory, 1).is_err());
        drop(db);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn metadata_exposes_individual_characters_and_preserves_prompt_text() {
        let chunks = BTreeMap::from([
            ("Description".into(), " best quality, dress ".into()),
            ("Comment".into(), serde_json::json!({"uc":"bad", "v4_prompt":{"caption":{"char_captions":[{"char_caption":"girl, red dress"},{"char_caption":"boy, suit"}]}}, "v4_negative_prompt":{"caption":{"char_captions":[{"char_caption":"hat"}]}}}).to_string()),
        ]);
        let sections = metadata_sections(&chunks);
        assert_eq!(
            sections
                .iter()
                .find(|s| s.id == "v4_prompt-1")
                .unwrap()
                .text,
            "boy, suit"
        );
        assert!(sections.iter().any(|s| s.label == "角色 1 负向提示词"));
        assert!(sections.iter().any(|s| s.id == "raw-Comment"));
        assert!(metadata_sections(&BTreeMap::new()).is_empty());
    }

    #[test]
    fn inspecting_real_png_is_read_only_and_returns_decodable_preview() {
        let folder = std::env::temp_dir().join(format!(
            "materials-inspect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("source.png");
        let bytes = crate::storage::test_fixtures::metadata_png_bytes(
            "quality, dress",
            Some(
                r#"{"uc":"bad","v4_prompt":{"caption":{"char_captions":[{"char_caption":"girl"}]}}}"#,
            ),
        );
        std::fs::write(&path, &bytes).unwrap();
        let result = inspect(&path).unwrap();
        assert!(
            result
                .sections
                .iter()
                .any(|s| s.id == "positive" && s.text == "quality, dress")
        );
        assert!(
            result
                .sections
                .iter()
                .any(|s| s.id == "v4_prompt-0" && s.text == "girl")
        );
        assert!(image::load_from_memory(&result.preview).is_ok());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        std::fs::write(&path, b"not an image").unwrap();
        assert!(inspect(&path).is_err());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
