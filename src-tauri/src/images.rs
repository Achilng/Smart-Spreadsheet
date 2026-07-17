use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use image::{DynamicImage, ImageFormat, ImageReader};
use thiserror::Error;

use crate::db::{DatabaseError, RowImageLocator};
use crate::storage::{DataDirectory, StorageError};

const THUMBNAIL_MAX_EDGE: u32 = 256;
const GALLERY_PREVIEW_MAX_EDGE: u32 = 1024;
const PREVIEW_MAX_EDGE: u32 = 2048;
const DERIVED_CACHE_MAX_BYTES: u64 = 512 * 1024 * 1024;
const DERIVED_CACHE_TARGET_BYTES: u64 = 448 * 1024 * 1024;
const CACHE_PRUNE_INTERVAL: usize = 32;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
static CACHE_WRITES: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageVariant {
    Thumbnail,
    GalleryPreview,
    Preview,
    Original,
}

impl ImageVariant {
    fn max_edge(self) -> Option<u32> {
        match self {
            Self::Thumbnail => Some(THUMBNAIL_MAX_EDGE),
            Self::GalleryPreview => Some(GALLERY_PREVIEW_MAX_EDGE),
            Self::Preview => Some(PREVIEW_MAX_EDGE),
            Self::Original => None,
        }
    }

    fn cache_label(self) -> Option<&'static str> {
        match self {
            Self::Thumbnail => Some("thumb"),
            Self::GalleryPreview => Some("gallery"),
            Self::Preview => Some("preview"),
            Self::Original => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageOrigin {
    ExternalPath,
    /// 受管数据目录中的图片副本。
    StoredCopy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImagePayload {
    pub png_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub origin: ImageOrigin,
    pub cache_hit: bool,
}

#[derive(Debug, Error)]
pub(crate) enum RowImageError {
    #[error("图片行 ID 必须为正整数: {0}")]
    InvalidRowId(i64),
    #[error("图片数据库查询失败: {0}")]
    Storage(#[from] StorageError),
    #[error("图片行查询失败: {0}")]
    Database(#[from] DatabaseError),
    #[error("图片文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("图片解码或编码失败: {0}")]
    Image(#[from] image::ImageError),
    #[error("第 {row_id} 行没有可用图片。路径来源: {external}; 副本来源: {stored}")]
    Unavailable {
        row_id: i64,
        external: String,
        stored: String,
    },
}

impl DataDirectory {
    pub(crate) fn load_row_image(
        &self,
        row_id: i64,
        variant: ImageVariant,
    ) -> Result<ImagePayload, RowImageError> {
        if row_id <= 0 {
            return Err(RowImageError::InvalidRowId(row_id));
        }
        let locator = self.open_database()?.row_image_locator(row_id)?;
        load_row_image(self, &locator, variant)
    }
}

fn load_row_image(
    directory: &DataDirectory,
    locator: &RowImageLocator,
    variant: ImageVariant,
) -> Result<ImagePayload, RowImageError> {
    let mut external_error = "未配置路径".to_owned();
    if let Some(path) = nonempty_path(locator.image_path.as_deref()) {
        match load_external(directory, locator.row_id, path, variant) {
            Ok(payload) => return Ok(payload),
            Err(error) => external_error = error.to_string(),
        }
    }

    let mut stored_error = "无受管副本".to_owned();
    if let Some(relative) = nonempty_text(locator.stored_image_path.as_deref()) {
        match load_stored(directory, locator.row_id, relative, variant) {
            Ok(payload) => return Ok(payload),
            Err(error) => stored_error = error.to_string(),
        }
    }

    Err(RowImageError::Unavailable {
        row_id: locator.row_id,
        external: external_error,
        stored: stored_error,
    })
}

fn load_external(
    directory: &DataDirectory,
    row_id: i64,
    path: &Path,
    variant: ImageVariant,
) -> Result<ImagePayload, RowImageError> {
    let metadata = fs::metadata(path)?;
    if let Some(cache_label) = variant.cache_label() {
        let cache_path = image_cache_path(
            directory,
            row_id,
            cache_label,
            &(
                "external",
                path.to_string_lossy().as_ref(),
                metadata_signature(&metadata),
                variant.max_edge(),
            ),
        );
        if let Some(payload) = read_cached_image(&cache_path, ImageOrigin::ExternalPath)? {
            return Ok(payload);
        }
        let image = ImageReader::open(path)?.with_guessed_format()?.decode()?;
        return encode_and_cache_image(
            directory,
            row_id,
            cache_label,
            cache_path,
            image,
            variant
                .max_edge()
                .expect("cached image variants always have a maximum edge"),
            ImageOrigin::ExternalPath,
        );
    }

    load_original(path, ImageOrigin::ExternalPath)
}

fn load_stored(
    directory: &DataDirectory,
    row_id: i64,
    relative: &str,
    variant: ImageVariant,
) -> Result<ImagePayload, RowImageError> {
    let path = directory.root().join(relative);
    let metadata = fs::metadata(&path)?;
    if let Some(cache_label) = variant.cache_label() {
        let cache_path = image_cache_path(
            directory,
            row_id,
            cache_label,
            &(
                "stored",
                relative,
                metadata_signature(&metadata),
                variant.max_edge(),
            ),
        );
        if let Some(payload) = read_cached_image(&cache_path, ImageOrigin::StoredCopy)? {
            return Ok(payload);
        }
        let image = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
        return encode_and_cache_image(
            directory,
            row_id,
            cache_label,
            cache_path,
            image,
            variant
                .max_edge()
                .expect("cached image variants always have a maximum edge"),
            ImageOrigin::StoredCopy,
        );
    }

    load_original(&path, ImageOrigin::StoredCopy)
}

fn load_original(path: &Path, origin: ImageOrigin) -> Result<ImagePayload, RowImageError> {
    let dimensions = image::image_dimensions(path)?;
    Ok(ImagePayload {
        png_bytes: fs::read(path)?,
        width: dimensions.0,
        height: dimensions.1,
        origin,
        cache_hit: false,
    })
}

fn encode_and_cache_image(
    directory: &DataDirectory,
    row_id: i64,
    cache_label: &str,
    cache_path: PathBuf,
    image: DynamicImage,
    max_edge: u32,
    origin: ImageOrigin,
) -> Result<ImagePayload, RowImageError> {
    let payload = encode_resized(image, max_edge, origin, false)?;
    write_cache_atomically(&cache_path, &payload.png_bytes)?;
    remove_stale_row_variant_cache(directory, row_id, cache_label, &cache_path)?;
    prune_derived_cache_if_needed(directory, &cache_path);
    Ok(payload)
}

fn encode_resized(
    image: DynamicImage,
    max_edge: u32,
    origin: ImageOrigin,
    cache_hit: bool,
) -> Result<ImagePayload, RowImageError> {
    let resized = if image.width() <= max_edge && image.height() <= max_edge {
        image
    } else {
        image.thumbnail(max_edge, max_edge)
    };
    let width = resized.width();
    let height = resized.height();
    let mut output = Cursor::new(Vec::new());
    resized.write_to(&mut output, ImageFormat::Png)?;
    Ok(ImagePayload {
        png_bytes: output.into_inner(),
        width,
        height,
        origin,
        cache_hit,
    })
}

fn read_cached_image(
    path: &Path,
    origin: ImageOrigin,
) -> Result<Option<ImagePayload>, RowImageError> {
    if !path.is_file() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    if !bytes.starts_with(PNG_SIGNATURE) {
        let _ = fs::remove_file(path);
        return Ok(None);
    }
    let dimensions = image::image_dimensions(path)?;
    Ok(Some(ImagePayload {
        png_bytes: bytes,
        width: dimensions.0,
        height: dimensions.1,
        origin,
        cache_hit: true,
    }))
}

fn image_cache_path<T: Hash>(
    directory: &DataDirectory,
    row_id: i64,
    cache_label: &str,
    signature: &T,
) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    signature.hash(&mut hasher);
    directory.thumbnail_cache_path().join(format!(
        "row-{row_id}-{cache_label}-{:016x}.png",
        hasher.finish()
    ))
}

fn metadata_signature(metadata: &fs::Metadata) -> (u64, u128) {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    (metadata.len(), modified)
}

fn write_cache_atomically(path: &Path, bytes: &[u8]) -> Result<(), RowImageError> {
    if path.is_file() {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("缩略图缓存路径没有父目录"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".thumbnail-{}-{nonce}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        if !path.is_file() {
            return Err(error.into());
        }
    }
    Ok(())
}

fn remove_stale_row_variant_cache(
    directory: &DataDirectory,
    row_id: i64,
    cache_label: &str,
    current: &Path,
) -> Result<(), RowImageError> {
    let prefix = format!("row-{row_id}-{cache_label}-");
    let legacy_prefix = format!("row-{row_id}-");
    for entry in fs::read_dir(directory.thumbnail_cache_path())? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let is_legacy_thumbnail = cache_label == "thumb"
            && name.starts_with(&legacy_prefix)
            && !name.starts_with(&prefix)
            && name.strip_prefix(&legacy_prefix).is_some_and(|suffix| {
                suffix.strip_suffix(".png").is_some_and(|hash| {
                    hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit())
                })
            });
        if path != current && path.is_file() && (name.starts_with(&prefix) || is_legacy_thumbnail) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn prune_derived_cache_if_needed(directory: &DataDirectory, protected_path: &Path) {
    if !CACHE_WRITES
        .fetch_add(1, Ordering::Relaxed)
        .is_multiple_of(CACHE_PRUNE_INTERVAL)
    {
        return;
    }

    let Ok(entries) = fs::read_dir(directory.thumbnail_cache_path()) else {
        return;
    };
    let mut total_bytes = 0_u64;
    let mut cached_files = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !path.is_file() || !name.starts_with("row-") || !name.ends_with(".png") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        total_bytes = total_bytes.saturating_add(metadata.len());
        cached_files.push((
            metadata.modified().unwrap_or(UNIX_EPOCH),
            metadata.len(),
            path,
        ));
    }
    if total_bytes <= DERIVED_CACHE_MAX_BYTES {
        return;
    }

    cached_files.sort_unstable_by_key(|(modified, _, _)| *modified);
    for (_, bytes, path) in cached_files {
        if path == protected_path {
            continue;
        }
        if fs::remove_file(path).is_ok() {
            total_bytes = total_bytes.saturating_sub(bytes);
        }
        if total_bytes <= DERIVED_CACHE_TARGET_BYTES {
            break;
        }
    }
}

fn nonempty_path(value: Option<&str>) -> Option<&Path> {
    nonempty_text(value).map(Path::new)
}

fn nonempty_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn falls_back_to_stored_copy_and_reuses_thumbnail_cache() {
        let temporary = TemporaryImageDirectory::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let stored_dir = directory.files_path().join("1").join("embedded");
        fs::create_dir_all(&stored_dir).unwrap();
        image::DynamicImage::new_rgb8(800, 600)
            .save_with_format(stored_dir.join("row-1.png"), ImageFormat::Png)
            .unwrap();
        let locator = RowImageLocator {
            row_id: 1,
            image_path: Some(
                temporary
                    .root
                    .join("missing.png")
                    .to_string_lossy()
                    .into_owned(),
            ),
            stored_image_path: Some("files/1/embedded/row-1.png".into()),
            stored_image_is_original: false,
        };

        let first = load_row_image(&directory, &locator, ImageVariant::Thumbnail).unwrap();
        let second = load_row_image(&directory, &locator, ImageVariant::Thumbnail).unwrap();

        assert_eq!(first.origin, ImageOrigin::StoredCopy);
        assert!(!first.cache_hit);
        assert!(first.width <= THUMBNAIL_MAX_EDGE);
        assert!(first.height <= THUMBNAIL_MAX_EDGE);
        assert!(first.png_bytes.starts_with(PNG_SIGNATURE));
        assert_eq!(second.png_bytes, first.png_bytes);
        assert!(second.cache_hit);
        assert_eq!(
            fs::read_dir(directory.thumbnail_cache_path())
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn prefers_external_image_over_stored_copy() {
        let temporary = TemporaryImageDirectory::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let external = temporary.root.join("external.png");
        image::DynamicImage::new_rgb8(640, 320)
            .save_with_format(&external, ImageFormat::Png)
            .unwrap();
        let locator = RowImageLocator {
            row_id: 7,
            image_path: Some(external.to_string_lossy().into_owned()),
            stored_image_path: Some("files/1/missing.png".into()),
            stored_image_is_original: true,
        };

        let payload = load_row_image(&directory, &locator, ImageVariant::Preview).unwrap();

        assert_eq!(payload.origin, ImageOrigin::ExternalPath);
        assert_eq!((payload.width, payload.height), (640, 320));
        assert!(!payload.cache_hit);
    }

    #[test]
    fn caches_each_resized_tier_and_returns_original_bytes_unchanged() {
        let temporary = TemporaryImageDirectory::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let external = temporary.root.join("large.png");
        image::DynamicImage::new_rgb8(2_400, 1_200)
            .save_with_format(&external, ImageFormat::Png)
            .unwrap();
        let locator = RowImageLocator {
            row_id: 9,
            image_path: Some(external.to_string_lossy().into_owned()),
            stored_image_path: None,
            stored_image_is_original: true,
        };

        let thumbnail = load_row_image(&directory, &locator, ImageVariant::Thumbnail).unwrap();
        let gallery = load_row_image(&directory, &locator, ImageVariant::GalleryPreview).unwrap();
        let preview = load_row_image(&directory, &locator, ImageVariant::Preview).unwrap();
        let original = load_row_image(&directory, &locator, ImageVariant::Original).unwrap();

        assert_eq!((thumbnail.width, thumbnail.height), (256, 128));
        assert_eq!((gallery.width, gallery.height), (1_024, 512));
        assert_eq!((preview.width, preview.height), (2_048, 1_024));
        assert_eq!((original.width, original.height), (2_400, 1_200));
        assert_eq!(original.png_bytes, fs::read(&external).unwrap());
        assert_eq!(
            fs::read_dir(directory.thumbnail_cache_path())
                .unwrap()
                .count(),
            3
        );

        assert!(
            load_row_image(&directory, &locator, ImageVariant::GalleryPreview)
                .unwrap()
                .cache_hit
        );
        assert!(
            load_row_image(&directory, &locator, ImageVariant::Preview)
                .unwrap()
                .cache_hit
        );
    }

    struct TemporaryImageDirectory {
        root: PathBuf,
        data: PathBuf,
    }

    impl TemporaryImageDirectory {
        fn new() -> Self {
            let parent = Path::new(r"D:\Agent\Agent_temp");
            let parent = if parent.is_dir() {
                parent.to_owned()
            } else {
                std::env::temp_dir()
            };
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            let root = parent.join(format!(
                "smart-spreadsheet-images-{}-{nonce}",
                std::process::id()
            ));
            Self {
                data: root.join("data"),
                root,
            }
        }
    }

    impl Drop for TemporaryImageDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
