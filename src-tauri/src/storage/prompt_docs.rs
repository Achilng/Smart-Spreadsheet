use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::{DataDirectory, canonical_display_path};

const PROMPT_DOCS_DIR: &str = "prompt-docs";
const META_FILE: &str = "meta.json";
const CONTENT_FILE: &str = "content.json";
const IMAGES_DIR: &str = "images";

#[derive(Debug, Error)]
pub enum PromptDocError {
    #[error("提示词文档文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("提示词文档 JSON 无效: {0}")]
    Json(#[from] serde_json::Error),
    #[error("提示词文档 ID 无效: {0}")]
    InvalidDocId(String),
    #[error("提示词文档不存在: {0}")]
    NotFound(String),
    #[error("图片文件路径无效: {0}")]
    InvalidImagePath(PathBuf),
    #[error("不支持的图片格式: {0}")]
    UnsupportedImageExtension(String),
    #[error("图片内容为空")]
    EmptyImage,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptDocSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub plain_text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptDocDetail {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub plain_text: String,
    pub content: Value,
    pub assets: Vec<PromptDocAsset>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptDocAsset {
    pub src: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptDocMeta {
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
    plain_text: String,
}

impl DataDirectory {
    pub fn prompt_docs_path(&self) -> PathBuf {
        self.root().join(PROMPT_DOCS_DIR)
    }

    pub fn list_prompt_docs(&self) -> Result<Vec<PromptDocSummary>, PromptDocError> {
        let root = self.ensure_prompt_docs_dir()?;
        let mut docs = Vec::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            if validate_doc_id(&id).is_err() {
                continue;
            }
            let meta = read_meta(&entry.path())?;
            docs.push(PromptDocSummary::from(meta));
        }
        docs.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.title.cmp(&right.title))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(docs)
    }

    pub fn create_prompt_doc(&self, title: &str) -> Result<PromptDocDetail, PromptDocError> {
        let root = self.ensure_prompt_docs_dir()?;
        let now = timestamp();
        let title = clean_title(title);

        for attempt in 0..1000_u32 {
            let id = format!("doc-{}-{attempt}", now_millis());
            let doc_dir = root.join(&id);
            match fs::create_dir(&doc_dir) {
                Ok(()) => {
                    fs::create_dir(doc_dir.join(IMAGES_DIR))?;
                    let meta = PromptDocMeta {
                        id: id.clone(),
                        title,
                        created_at: now.clone(),
                        updated_at: now,
                        plain_text: String::new(),
                    };
                    write_json(doc_dir.join(META_FILE), &meta)?;
                    write_json(doc_dir.join(CONTENT_FILE), &empty_doc_content())?;
                    return self.load_prompt_doc(&id);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }

        Err(PromptDocError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "无法创建唯一的提示词文档目录",
        )))
    }

    pub fn load_prompt_doc(&self, doc_id: &str) -> Result<PromptDocDetail, PromptDocError> {
        let doc_dir = self.doc_dir(doc_id)?;
        let meta = read_meta(&doc_dir)?;
        let content = read_json(doc_dir.join(CONTENT_FILE))?;
        let assets = list_assets(&doc_dir)?;
        Ok(PromptDocDetail {
            id: meta.id,
            title: meta.title,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
            plain_text: meta.plain_text,
            content,
            assets,
        })
    }

    pub fn save_prompt_doc(
        &self,
        doc_id: &str,
        title: &str,
        content: &Value,
        plain_text: &str,
    ) -> Result<PromptDocDetail, PromptDocError> {
        let doc_dir = self.doc_dir(doc_id)?;
        let mut meta = read_meta(&doc_dir)?;
        meta.title = clean_title(title);
        meta.updated_at = timestamp();
        meta.plain_text = plain_text.to_owned();
        write_json(doc_dir.join(CONTENT_FILE), content)?;
        write_json(doc_dir.join(META_FILE), &meta)?;
        self.load_prompt_doc(doc_id)
    }

    pub fn delete_prompt_doc(&self, doc_id: &str) -> Result<(), PromptDocError> {
        let doc_dir = self.doc_dir(doc_id)?;
        fs::remove_dir_all(doc_dir)?;
        Ok(())
    }

    pub fn import_prompt_doc_image_from_path(
        &self,
        doc_id: &str,
        path: impl AsRef<Path>,
    ) -> Result<PromptDocAsset, PromptDocError> {
        let path = path.as_ref();
        if !path.is_file() {
            return Err(PromptDocError::InvalidImagePath(path.to_owned()));
        }
        let extension = image_extension(path)?;
        let (src, target) = self.reserve_asset_path(doc_id, extension)?;
        fs::copy(path, &target)?;
        Ok(asset_from_path(src, target))
    }

    pub fn import_prompt_doc_image_bytes(
        &self,
        doc_id: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<PromptDocAsset, PromptDocError> {
        if bytes.is_empty() {
            return Err(PromptDocError::EmptyImage);
        }
        let extension = image_extension(Path::new(file_name))?;
        let (src, target) = self.reserve_asset_path(doc_id, extension)?;
        fs::write(&target, bytes)?;
        Ok(asset_from_path(src, target))
    }

    fn ensure_prompt_docs_dir(&self) -> Result<PathBuf, PromptDocError> {
        let path = self.prompt_docs_path();
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    fn doc_dir(&self, doc_id: &str) -> Result<PathBuf, PromptDocError> {
        validate_doc_id(doc_id)?;
        let doc_dir = self.ensure_prompt_docs_dir()?.join(doc_id);
        if !doc_dir.is_dir() {
            return Err(PromptDocError::NotFound(doc_id.to_owned()));
        }
        Ok(doc_dir)
    }

    fn reserve_asset_path(
        &self,
        doc_id: &str,
        extension: &str,
    ) -> Result<(String, PathBuf), PromptDocError> {
        let images_dir = self.doc_dir(doc_id)?.join(IMAGES_DIR);
        fs::create_dir_all(&images_dir)?;
        for attempt in 0..1000_u32 {
            let file_name = format!("img-{}-{attempt}.{extension}", now_millis());
            let path = images_dir.join(&file_name);
            if !path.exists() {
                return Ok((format!("{IMAGES_DIR}/{file_name}"), path));
            }
        }
        Err(PromptDocError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "无法创建唯一的提示词文档图片文件",
        )))
    }
}

impl From<PromptDocMeta> for PromptDocSummary {
    fn from(meta: PromptDocMeta) -> Self {
        Self {
            id: meta.id,
            title: meta.title,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
            plain_text: meta.plain_text,
        }
    }
}

fn validate_doc_id(doc_id: &str) -> Result<(), PromptDocError> {
    let valid = !doc_id.is_empty()
        && doc_id.len() <= 128
        && doc_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(PromptDocError::InvalidDocId(doc_id.to_owned()))
    }
}

fn clean_title(title: &str) -> String {
    let title = title.trim();
    if title.is_empty() {
        "未命名文档".to_owned()
    } else {
        title.chars().take(80).collect()
    }
}

fn timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn empty_doc_content() -> Value {
    serde_json::json!({
        "type": "doc",
        "content": [
            { "type": "paragraph" }
        ]
    })
}

fn read_meta(doc_dir: &Path) -> Result<PromptDocMeta, PromptDocError> {
    read_json(doc_dir.join(META_FILE))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: impl AsRef<Path>) -> Result<T, PromptDocError> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_json(path: impl AsRef<Path>, value: &impl Serialize) -> Result<(), PromptDocError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes)?;
    Ok(())
}

fn list_assets(doc_dir: &Path) -> Result<Vec<PromptDocAsset>, PromptDocError> {
    let images_dir = doc_dir.join(IMAGES_DIR);
    if !images_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut assets = Vec::new();
    for entry in fs::read_dir(images_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.contains('/') || file_name.contains('\\') {
            continue;
        }
        assets.push(asset_from_path(
            format!("{IMAGES_DIR}/{file_name}"),
            entry.path(),
        ));
    }
    assets.sort_by(|left, right| left.src.cmp(&right.src));
    Ok(assets)
}

fn asset_from_path(src: String, path: PathBuf) -> PromptDocAsset {
    PromptDocAsset {
        src,
        path: canonical_display_path(&path),
    }
}

fn image_extension(path: &Path) -> Result<&'static str, PromptDocError> {
    let raw = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match raw.as_str() {
        "png" => Ok("png"),
        "jpg" | "jpeg" => Ok("jpg"),
        "webp" => Ok("webp"),
        "gif" => Ok("gif"),
        "bmp" => Ok("bmp"),
        "tif" | "tiff" => Ok("tiff"),
        _ => Err(PromptDocError::UnsupportedImageExtension(raw)),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn creates_lists_loads_saves_and_deletes_docs() {
        let temporary = TemporaryDirectory::new("crud");
        let directory = DataDirectory::initialize(&temporary.path).unwrap();

        let created = directory.create_prompt_doc("  常用提示词  ").unwrap();
        assert_eq!(created.title, "常用提示词");
        assert_eq!(created.plain_text, "");

        let docs = directory.list_prompt_docs().unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].id, created.id);

        let content = serde_json::json!({
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "best quality" }] }
            ]
        });
        let saved = directory
            .save_prompt_doc(&created.id, "Prompt A", &content, "best quality")
            .unwrap();
        assert_eq!(saved.title, "Prompt A");
        assert_eq!(saved.plain_text, "best quality");
        assert_eq!(saved.content, content);

        let loaded = directory.load_prompt_doc(&created.id).unwrap();
        assert_eq!(loaded.title, "Prompt A");
        assert_eq!(loaded.plain_text, "best quality");

        directory.delete_prompt_doc(&created.id).unwrap();
        assert!(directory.list_prompt_docs().unwrap().is_empty());
    }

    #[test]
    fn rejects_doc_id_path_traversal() {
        let temporary = TemporaryDirectory::new("traversal");
        let directory = DataDirectory::initialize(&temporary.path).unwrap();

        assert!(matches!(
            directory.load_prompt_doc("../escape").unwrap_err(),
            PromptDocError::InvalidDocId(_)
        ));
        assert!(matches!(
            directory
                .import_prompt_doc_image_bytes("..\\escape", "x.png", b"image")
                .unwrap_err(),
            PromptDocError::InvalidDocId(_)
        ));
    }

    #[test]
    fn imports_images_from_path_and_bytes() {
        let temporary = TemporaryDirectory::new("images");
        let directory = DataDirectory::initialize(&temporary.path).unwrap();
        let doc = directory.create_prompt_doc("图文").unwrap();

        let source = temporary.path.join("source.png");
        fs::write(&source, b"source image bytes").unwrap();
        let from_path = directory
            .import_prompt_doc_image_from_path(&doc.id, &source)
            .unwrap();
        assert!(from_path.src.starts_with("images/img-"));
        assert!(Path::new(&from_path.path).is_file());
        assert_eq!(fs::read(&from_path.path).unwrap(), b"source image bytes");

        let from_bytes = directory
            .import_prompt_doc_image_bytes(&doc.id, "clipboard.webp", b"clipboard image")
            .unwrap();
        assert!(from_bytes.src.ends_with(".webp"));
        assert_eq!(fs::read(&from_bytes.path).unwrap(), b"clipboard image");

        let loaded = directory.load_prompt_doc(&doc.id).unwrap();
        assert_eq!(loaded.assets.len(), 2);
    }

    #[test]
    fn reset_data_preserves_prompt_docs() {
        let temporary = TemporaryDirectory::new("reset");
        let directory = DataDirectory::initialize(&temporary.path).unwrap();
        let doc = directory.create_prompt_doc("保留").unwrap();

        directory.reset_data().unwrap();

        let loaded = directory.load_prompt_doc(&doc.id).unwrap();
        assert_eq!(loaded.title, "保留");
    }

    #[test]
    fn migration_preserves_prompt_docs() {
        let temporary = TemporaryMigration::new();
        let source = DataDirectory::initialize(&temporary.source).unwrap();
        let doc = source.create_prompt_doc("迁移").unwrap();
        source
            .save_prompt_doc(&doc.id, "迁移", &empty_doc_content(), "plain")
            .unwrap();
        fs::create_dir_all(&temporary.destination).unwrap();

        let outcome = source.migrate_to(&temporary.destination).unwrap();

        let loaded = outcome.data_directory.load_prompt_doc(&doc.id).unwrap();
        assert_eq!(loaded.plain_text, "plain");
    }

    struct TemporaryDirectory {
        path: PathBuf,
    }

    impl TemporaryDirectory {
        fn new(label: &str) -> Self {
            let parent = temporary_parent();
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            Self {
                path: parent.join(format!(
                    "smart-spreadsheet-prompt-docs-{label}-{}-{nonce}",
                    std::process::id()
                )),
            }
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct TemporaryMigration {
        root: PathBuf,
        source: PathBuf,
        destination: PathBuf,
    }

    impl TemporaryMigration {
        fn new() -> Self {
            let parent = temporary_parent();
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            let root = parent.join(format!(
                "smart-spreadsheet-prompt-docs-migration-{}-{nonce}",
                std::process::id()
            ));
            Self {
                source: root.join("source"),
                destination: root.join("destination"),
                root,
            }
        }
    }

    impl Drop for TemporaryMigration {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn temporary_parent() -> PathBuf {
        let local_agent_temp = Path::new(r"D:\Agent\Agent_temp");
        if local_agent_temp.is_dir() {
            local_agent_temp.to_owned()
        } else {
            std::env::temp_dir()
        }
    }
}
