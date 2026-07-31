use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use image::ImageEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use sha2::{Digest, Sha256};
use thiserror::Error;
use walkdir::WalkDir;

use super::{DataDirectory, StorageError};
use crate::db::{ExportRow, RowSelection, TagMutationError};
use crate::pipeline::parallel;

const PROGRESS_EVERY_FILES: usize = 20;
const MAX_EXPORT_WORKERS: usize = 8;
const MAX_COPY_WORKERS: usize = 4;
const BYTES_PER_PIXEL_WORKING_SET: u64 = 8;
const SUPPORTED_IMAGE_EXTENSIONS: [&str; 8] = [
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff",
];
#[cfg(test)]
const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
#[cfg(test)]
const PNG_METADATA_FREE_CHUNKS: [[u8; 4]; 5] =
    [*b"IHDR", *b"PLTE", *b"tRNS", *b"IDAT", *b"IEND"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFileExportMode {
    /// 复制原图（默认）。
    Copy,
    /// NTFS 硬链接，单文件失败时自动回退为复制。
    Hardlink,
}

impl ImageFileExportMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "copy" => Some(Self::Copy),
            "hardlink" => Some(Self::Hardlink),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageFileNaming {
    /// 保留原文件名；同名时自动追加序号，绝不覆盖。
    Original,
    /// 使用不可读的随机式十六进制文件名。
    Random,
    /// 使用“自定义前缀_序号”。
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageFilesProgress {
    pub processed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFilesExportOutcome {
    pub directory: PathBuf,
    pub exported: usize,
    /// 硬链接失败回退为复制的文件数。
    pub hardlink_fallbacks: usize,
    /// 原路径和受管副本均不可用的行数。
    pub missing: usize,
}

#[derive(Debug, Error)]
pub enum ImageFilesExportError {
    #[error("导出目标必须是已存在的文件夹: {0}")]
    InvalidParent(PathBuf),
    #[error("没有可导出的行")]
    EmptySelection,
    #[error("自定义文件名前缀不能为空或仅由文件名非法字符组成")]
    EmptyCustomName,
    #[error("应用数据目录不可用: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Selection(#[from] TagMutationError),
    #[error("导出副本重新编码失败: {0}")]
    Image(#[from] image::ImageError),
    #[error("导出文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("扫描图片文件夹失败: {0}")]
    Walk(#[from] walkdir::Error),
}

/// 递归展开用户选择或拖入的图片/文件夹，按文件名自然排序并按规范化路径去重。
/// 不支持的文件会被忽略，文件夹中的符号链接目录不会被继续跟随。
pub fn collect_export_image_paths(
    input_paths: impl IntoIterator<Item = PathBuf>,
) -> Result<Vec<PathBuf>, ImageFilesExportError> {
    let mut images = Vec::new();
    let mut seen = HashSet::new();

    for input in input_paths {
        if input.is_file() {
            append_export_image(&mut images, &mut seen, input)?;
            continue;
        }
        if !input.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&input) {
            let entry = entry?;
            if entry.file_type().is_file() {
                append_export_image(&mut images, &mut seen, entry.into_path())?;
            }
        }
    }

    images.sort_by(|left, right| natural_path_cmp(left, right));
    Ok(images)
}

fn append_export_image(
    images: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    path: PathBuf,
) -> Result<(), ImageFilesExportError> {
    if !is_supported_export_image(&path) {
        return Ok(());
    }
    let identity = fs::canonicalize(&path)?;
    if seen.insert(identity) {
        images.push(path);
    }
    Ok(())
}

fn is_supported_export_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SUPPORTED_IMAGE_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

fn natural_path_cmp(left: &Path, right: &Path) -> Ordering {
    let left_name = left
        .file_name()
        .map(|value| value.to_string_lossy())
        .unwrap_or_else(|| left.as_os_str().to_string_lossy());
    let right_name = right
        .file_name()
        .map(|value| value.to_string_lossy())
        .unwrap_or_else(|| right.as_os_str().to_string_lossy());
    natural_str_cmp(&left_name, &right_name).then_with(|| {
        left.to_string_lossy()
            .to_lowercase()
            .cmp(&right.to_string_lossy().to_lowercase())
    })
}

fn natural_str_cmp(left: &str, right: &str) -> Ordering {
    let mut left_chars = left.char_indices().peekable();
    let mut right_chars = right.char_indices().peekable();

    loop {
        match (left_chars.peek().copied(), right_chars.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some((_, left_char)), Some((_, right_char)))
                if left_char.is_ascii_digit() && right_char.is_ascii_digit() =>
            {
                let left_number = take_ascii_digits(left, &mut left_chars);
                let right_number = take_ascii_digits(right, &mut right_chars);
                let left_trimmed = left_number.trim_start_matches('0');
                let right_trimmed = right_number.trim_start_matches('0');
                let left_value = if left_trimmed.is_empty() { "0" } else { left_trimmed };
                let right_value = if right_trimmed.is_empty() { "0" } else { right_trimmed };
                let number_order = left_value
                    .len()
                    .cmp(&right_value.len())
                    .then_with(|| left_value.cmp(right_value))
                    .then_with(|| left_number.len().cmp(&right_number.len()));
                if number_order != Ordering::Equal {
                    return number_order;
                }
            }
            (Some((_, left_char)), Some((_, right_char))) => {
                left_chars.next();
                right_chars.next();
                let character_order = left_char
                    .to_lowercase()
                    .to_string()
                    .cmp(&right_char.to_lowercase().to_string());
                if character_order != Ordering::Equal {
                    return character_order;
                }
            }
        }
    }
}

fn take_ascii_digits<'a>(
    value: &'a str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'a>>,
) -> &'a str {
    let start = chars.peek().map(|(index, _)| *index).unwrap_or(value.len());
    let mut end = start;
    while let Some((index, character)) = chars.peek().copied() {
        if !character.is_ascii_digit() {
            break;
        }
        chars.next();
        end = index + character.len_utf8();
    }
    &value[start..end]
}

impl DataDirectory {
    /// 把选中行的图片直接导出到用户选择的目标文件夹，
    /// 文件名带库内顺序前缀（`00001_原名.png`），保证排序与库一致。
    /// 来源优先原路径，其次受管副本；两者都不可用的行计入 missing。
    pub fn export_image_files(
        &self,
        selection: &RowSelection,
        parent_dir: impl AsRef<Path>,
        mode: ImageFileExportMode,
        progress: impl Fn(ImageFilesProgress) + Sync,
    ) -> Result<ImageFilesExportOutcome, ImageFilesExportError> {
        let parent_dir = parent_dir.as_ref();
        if !parent_dir.is_dir() {
            return Err(ImageFilesExportError::InvalidParent(parent_dir.to_owned()));
        }

        let rows = self.open_database()?.export_rows(selection)?;
        if rows.is_empty() {
            return Err(ImageFilesExportError::EmptySelection);
        }
        let total = rows.len();
        progress(ImageFilesProgress {
            processed: 0,
            total,
        });

        let output_dir = parent_dir.to_owned();
        let mut exported = 0;
        let mut hardlink_fallbacks = 0;
        let mut missing = 0;

        for (index, row) in rows.iter().enumerate() {
            match resolve_source(self, row) {
                Some(source) => {
                    let file_name = output_file_name(index + 1, &source);
                    let target = unique_output_target(&output_dir, &file_name);
                    match mode {
                        ImageFileExportMode::Copy => {
                            fs::copy(&source, &target)?;
                        }
                        ImageFileExportMode::Hardlink => {
                            if fs::hard_link(&source, &target).is_err() {
                                fs::copy(&source, &target)?;
                                hardlink_fallbacks += 1;
                            }
                        }
                    }
                    exported += 1;
                }
                None => missing += 1,
            }
            let processed = index + 1;
            if processed % PROGRESS_EVERY_FILES == 0 || processed == total {
                progress(ImageFilesProgress { processed, total });
            }
        }

        Ok(ImageFilesExportOutcome {
            directory: output_dir,
            exported,
            hardlink_fallbacks,
            missing,
        })
    }

    /// 按工具箱选项导出主窗口选中的图片。
    ///
    /// 直接写入用户选择的目标文件夹；同名文件自动追加序号，避免覆盖已有文件。
    /// `strip_metadata` 只重新编码导出副本，来源文件和资料库均保持不变。
    pub fn export_selected_images(
        &self,
        selection: &RowSelection,
        extra_sources: &[PathBuf],
        parent_dir: impl AsRef<Path>,
        naming: ImageFileNaming,
        strip_metadata: bool,
        progress: impl Fn(ImageFilesProgress) + Sync,
    ) -> Result<ImageFilesExportOutcome, ImageFilesExportError> {
        let parent_dir = parent_dir.as_ref();
        if !parent_dir.is_dir() {
            return Err(ImageFilesExportError::InvalidParent(parent_dir.to_owned()));
        }
        let naming = validate_naming(naming)?;
        let rows = self.open_database()?.export_rows(selection)?;
        if rows.is_empty() && extra_sources.is_empty() {
            return Err(ImageFilesExportError::EmptySelection);
        }

        let mut sources = Vec::with_capacity(rows.len() + extra_sources.len());
        let mut source_identities = HashSet::with_capacity(rows.len() + extra_sources.len());
        let mut missing = 0;
        for row in &rows {
            match resolve_source(self, row) {
                Some(source) => {
                    append_unique_source(&mut sources, &mut source_identities, source);
                }
                None => missing += 1,
            }
        }
        for source in extra_sources {
            if source.is_file() && is_supported_export_image(source) {
                append_unique_source(
                    &mut sources,
                    &mut source_identities,
                    source.to_owned(),
                );
            }
        }
        if sources.is_empty() {
            return Err(ImageFilesExportError::EmptySelection);
        }

        let total = sources.len() + missing;
        progress(ImageFilesProgress { processed: 0, total });
        let output_dir = parent_dir.to_owned();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut jobs = Vec::with_capacity(sources.len());
        let mut reserved_targets = HashSet::with_capacity(total);
        for source in sources {
            let ordinal = jobs.len() + 1;
            let file_name = selected_output_file_name(
                &naming,
                ordinal,
                nonce,
                i64::try_from(ordinal).unwrap_or(i64::MAX),
                &source,
            );
            let target =
                reserve_unique_output_target(&output_dir, &file_name, &mut reserved_targets);
            jobs.push((source, target));
        }

        let exported = jobs.len();
        let workers = adaptive_export_worker_count(&jobs, strip_metadata);
        let results = parallel::parallel_map(
            jobs,
            workers,
            |_, (source, target)| write_exported_copy(&source, &target, strip_metadata),
            |completed| {
                let processed = missing + completed;
                if processed % PROGRESS_EVERY_FILES == 0 || processed == total {
                    progress(ImageFilesProgress { processed, total });
                }
            },
        );
        if exported == 0 {
            progress(ImageFilesProgress {
                processed: total,
                total,
            });
        }
        for result in results {
            result?;
        }

        Ok(ImageFilesExportOutcome {
            directory: output_dir,
            exported,
            hardlink_fallbacks: 0,
            missing,
        })
    }
}

impl DataDirectory {
    pub fn export_single_image(
        &self,
        row_id: i64,
        destination: &Path,
    ) -> Result<(), ImageFilesExportError> {
        let locator = self
            .open_database()?
            .row_image_locator(row_id)
            .map_err(|e| ImageFilesExportError::Storage(StorageError::Database(e)))?;
        let source = resolve_locator_source(self, &locator).ok_or_else(|| {
            std::io::Error::other(format!("第 {row_id} 行没有可用的原图文件"))
        })?;
        fs::copy(&source, destination)?;
        Ok(())
    }
}

pub fn resolve_locator_source(
    directory: &DataDirectory,
    locator: &crate::db::RowImageLocator,
) -> Option<PathBuf> {
    if let Some(path) = locator
        .image_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let path = Path::new(path);
        if path.is_file() {
            return Some(path.to_owned());
        }
    }
    if let Some(stored) = locator
        .stored_image_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let path = directory.root().join(stored);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn append_unique_source(
    sources: &mut Vec<PathBuf>,
    identities: &mut HashSet<PathBuf>,
    source: PathBuf,
) {
    let identity = fs::canonicalize(&source).unwrap_or_else(|_| source.clone());
    if identities.insert(identity) {
        sources.push(source);
    }
}

/// 只解析"完整原件"：外部原图，或明确标记为完整原件的受管副本。
/// 历史导入产生的无元数据缩略图不算原件——拖出这种副本
/// 会让 NovelAI 等下游丢失全部元数据，宁可报错也不静默降级。
pub fn resolve_original_source(
    directory: &DataDirectory,
    locator: &crate::db::RowImageLocator,
) -> Result<PathBuf, OriginalSourceError> {
    if let Some(path) = locator
        .image_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let path = Path::new(path);
        if path.is_file() {
            return Ok(path.to_owned());
        }
        if !locator.stored_image_is_original {
            return Err(OriginalSourceError::OriginalMissing(path.to_owned()));
        }
    }
    if locator.stored_image_is_original
        && let Some(stored) = locator
            .stored_image_path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
    {
        let path = directory.root().join(stored);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(OriginalSourceError::NoOriginal)
}

#[derive(Debug, Error)]
pub enum OriginalSourceError {
    #[error("原图文件不存在或已移动: {0}（没有可用的完整受管原图副本）")]
    OriginalMissing(PathBuf),
    #[error("该行没有可用的完整原图文件（旧表格内嵌缩略图不含元数据，已阻止使用）")]
    NoOriginal,
}

fn resolve_source(directory: &DataDirectory, row: &ExportRow) -> Option<PathBuf> {
    if let Some(path) = row.image_path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        let path = Path::new(path);
        if path.is_file() {
            return Some(path.to_owned());
        }
    }
    if let Some(stored) = row
        .stored_image_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let path = directory.root().join(stored);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn validate_naming(naming: ImageFileNaming) -> Result<ImageFileNaming, ImageFilesExportError> {
    match naming {
        ImageFileNaming::Custom(prefix) => {
            let prefix = sanitize_file_stem(prefix.trim());
            if prefix.is_empty() {
                Err(ImageFilesExportError::EmptyCustomName)
            } else {
                Ok(ImageFileNaming::Custom(prefix))
            }
        }
        naming => Ok(naming),
    }
}

fn selected_output_file_name(
    naming: &ImageFileNaming,
    ordinal: usize,
    nonce: u128,
    row_id: i64,
    source: &Path,
) -> String {
    match naming {
        ImageFileNaming::Original => sanitized_source_file_name(source),
        ImageFileNaming::Random => {
            let stem = random_file_stem(nonce, ordinal, row_id);
            format!("{stem}{}", extension_suffix(source))
        }
        ImageFileNaming::Custom(prefix) => {
            format!("{prefix}_{ordinal}{}", extension_suffix(source))
        }
    }
}

fn random_file_stem(nonce: u128, ordinal: usize, row_id: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(nonce.to_le_bytes());
    hasher.update(ordinal.to_le_bytes());
    hasher.update(row_id.to_le_bytes());
    let digest = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(16);
    for byte in &digest[..8] {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn extension_suffix(source: &Path) -> String {
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 10
                && value.chars().all(|character| character.is_ascii_alphanumeric())
        })
        .unwrap_or("png");
    format!(".{}", extension.to_ascii_lowercase())
}

fn sanitized_source_file_name(source: &Path) -> String {
    let original = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image.png".to_owned());
    let sanitized = sanitize_file_name(&original);
    if sanitized.is_empty() {
        format!("image{}", extension_suffix(source))
    } else {
        sanitized
    }
}

fn sanitize_file_name(value: &str) -> String {
    value
        .chars()
        .take(180)
        .map(sanitize_file_name_character)
        .collect::<String>()
        .trim_matches([' ', '.'])
        .to_owned()
}

fn sanitize_file_stem(value: &str) -> String {
    value
        .chars()
        .take(120)
        .map(sanitize_file_name_character)
        .collect::<String>()
        .trim_matches([' ', '.', '_'])
        .to_owned()
}

fn sanitize_file_name_character(character: char) -> char {
    match character {
        '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
        character if (character as u32) < 0x20 => '_',
        character => character,
    }
}

fn unique_output_target(directory: &Path, file_name: &str) -> PathBuf {
    let first = directory.join(file_name);
    if !first.exists() {
        return first;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "image".to_owned());
    let suffix = path
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();
    for index in 2_usize.. {
        let candidate = directory.join(format!("{stem}_{index}{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("unbounded file numbering always finds a candidate")
}

fn reserve_unique_output_target(
    directory: &Path,
    file_name: &str,
    reserved: &mut HashSet<PathBuf>,
) -> PathBuf {
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "image".to_owned());
    let suffix = path
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();

    for index in 1_usize.. {
        let name = if index == 1 {
            file_name.to_owned()
        } else {
            format!("{stem}_{index}{suffix}")
        };
        let candidate = directory.join(name);
        if !candidate.exists() && reserved.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("unbounded file numbering always finds a candidate")
}

fn adaptive_export_worker_count(jobs: &[(PathBuf, PathBuf)], strip_metadata: bool) -> usize {
    if jobs.is_empty() {
        return 0;
    }
    let logical_processors = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    if !strip_metadata {
        return export_worker_limit(logical_processors, jobs.len(), false, 1);
    }
    let largest_working_set = jobs
        .iter()
        .filter_map(|(source, _)| image::image_dimensions(source).ok())
        .filter_map(|(width, height)| {
            u64::from(width)
                .checked_mul(u64::from(height))?
                .checked_mul(BYTES_PER_PIXEL_WORKING_SET)
        })
        .max()
        .unwrap_or(128 * 1024 * 1024)
        .max(1);
    export_worker_limit(
        logical_processors,
        jobs.len(),
        strip_metadata,
        largest_working_set,
    )
}

fn export_worker_limit(
    logical_processors: usize,
    job_count: usize,
    strip_metadata: bool,
    largest_working_set: u64,
) -> usize {
    if job_count == 0 {
        return 0;
    }
    let cpu_limit = logical_processors
        .max(1)
        .div_ceil(2)
        .clamp(1, MAX_EXPORT_WORKERS)
        .min(job_count);
    if !strip_metadata {
        return cpu_limit.min(MAX_COPY_WORKERS);
    }

    let memory_budget_mib = match logical_processors {
        0..=2 => 192_u64,
        3..=4 => 256,
        5..=8 => 384,
        _ => 512,
    };
    let memory_budget = memory_budget_mib * 1024 * 1024;
    let memory_limit = usize::try_from(memory_budget / largest_working_set)
        .unwrap_or(1)
        .max(1);
    cpu_limit.min(memory_limit)
}

fn write_exported_copy(
    source: &Path,
    target: &Path,
    strip_metadata: bool,
) -> Result<(), ImageFilesExportError> {
    if !strip_metadata {
        fs::copy(source, target)?;
        return Ok(());
    }

    let reader = image::ImageReader::open(source)?.with_guessed_format()?;
    let format = reader.format().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("无法识别图片格式: {}", source.display()),
        )
    })?;
    let image = reader.decode()?;
    if format == image::ImageFormat::Png {
        let mut rgba = image.to_rgba8();
        crate::pipeline::stealth_png::scrub_stealth_alpha_lsb(&mut rgba);
        let mut writer = BufWriter::new(File::create(target)?);
        let encoder =
            PngEncoder::new_with_quality(&mut writer, CompressionType::Fast, FilterType::Sub);
        if let Err(error) = encoder.write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        ) {
            let _ = fs::remove_file(target);
            return Err(error.into());
        }
        if let Err(error) = writer.flush() {
            drop(writer);
            let _ = fs::remove_file(target);
            return Err(error.into());
        }
    } else {
        image.save_with_format(target, format)?;
    }
    Ok(())
}

fn output_file_name(ordinal: usize, source: &Path) -> String {
    format!("{ordinal:05}_{}", sanitized_source_file_name(source))
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};
    use std::time::{SystemTime, UNIX_EPOCH};

    use flate2::{Compression, write::GzEncoder};

    use super::*;
    use crate::db::{NewRow, SourceType, TagMatchMode};
    use crate::pipeline::png_text::read_png_text_chunks;
    use crate::pipeline::stealth_png::read_stealth_png_metadata;
    use crate::storage::test_fixtures::metadata_png_bytes;

    #[test]
    fn collects_nested_export_images_with_natural_sorting_and_path_deduplication() {
        let temporary = TemporaryImageFilesExport::new();
        let input = temporary.root.join("input");
        let nested = input.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(input.join("image10.png"), b"ten").unwrap();
        fs::write(input.join("image2.png"), b"two").unwrap();
        fs::write(nested.join("image1.JPG"), b"one").unwrap();
        fs::write(nested.join("notes.txt"), b"ignored").unwrap();

        let images = collect_export_image_paths([
            input.clone(),
            input.join("image2.png"),
            nested.join("notes.txt"),
        ])
        .unwrap();
        let names: Vec<String> = images
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        assert_eq!(names, ["image1.JPG", "image2.png", "image10.png"]);
    }

    #[test]
    fn exports_extra_image_paths_without_a_main_window_selection() {
        let temporary = TemporaryImageFilesExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let input = temporary.root.join("input");
        let output = temporary.root.join("output");
        fs::create_dir_all(&input).unwrap();
        fs::create_dir_all(&output).unwrap();
        let image = input.join("sample.webp");
        fs::write(&image, b"image-bytes").unwrap();

        let outcome = directory
            .export_selected_images(
                &RowSelection::Explicit { row_ids: Vec::new() },
                &[image.clone(), image],
                &output,
                ImageFileNaming::Original,
                false,
                |_| {},
            )
            .unwrap();

        assert_eq!(outcome.exported, 1);
        assert_eq!(fs::read(output.join("sample.webp")).unwrap(), b"image-bytes");
    }

    #[test]
    fn exports_images_with_ordered_names_and_counts_missing() {
        let temporary = TemporaryImageFilesExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let source_a = temporary.root.join("图片 A.png");
        fs::write(&source_a, b"a-bytes").unwrap();
        // 行 2 引用受管副本，行 3 两个来源都缺失。
        let stored_dir = directory.files_path().join("1");
        fs::create_dir_all(&stored_dir).unwrap();
        fs::write(stored_dir.join("b.png"), b"b-bytes").unwrap();
        {
            let mut database = directory.open_database().unwrap();
            let rows = vec![
                NewRow {
                    source_ordinal: 1,
                    identity: "file:a".into(),
                    image_path: Some(source_a.display().to_string()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 2,
                    identity: "archive:p!b".into(),
                    image_path: Some(r"D:\missing\pack.zip > b.png".into()),
                    stored_image_rel: Some("b.png".into()),
                    ..NewRow::default()
                },
                NewRow {
                    source_ordinal: 3,
                    identity: "file:gone".into(),
                    image_path: Some(r"D:\missing\gone.png".into()),
                    ..NewRow::default()
                },
            ];
            // 受管副本写到 files/1/，批次 ID 也必须是 1 才能对上。
            let outcome = database
                .append_batch(SourceType::Archive, r"D:\pack.zip", &rows, |_| Ok(()))
                .unwrap();
            assert_eq!(outcome.batch_id, 1);
        }

        let outcome = directory
            .export_image_files(
                &RowSelection::Filtered {
                    tags: Vec::new(),
                    tag_mode: TagMatchMode::And,
                    dedupe: crate::db::DedupeMode::None,
                    single_artist_only: false,
                    artist_filter: String::new(),
                    has_vibe: false,
                    untagged_only: false,
                    search: String::new(),
                    excluded_row_ids: Vec::new(),
                },
                &temporary.root,
                ImageFileExportMode::Copy,
                |_| {},
            )
            .unwrap();

        assert_eq!(outcome.exported, 2);
        assert_eq!(outcome.missing, 1);
        assert_eq!(
            fs::read(outcome.directory.join("00001_图片 A.png")).unwrap(),
            b"a-bytes"
        );
        assert_eq!(
            fs::read(outcome.directory.join("00002_b.png")).unwrap(),
            b"b-bytes"
        );

        // 再导一次：仍写入所选目录，同名文件自动编号且不覆盖旧输出。
        let second = directory
            .export_image_files(
                &RowSelection::Explicit { row_ids: vec![1] },
                &temporary.root,
                ImageFileExportMode::Hardlink,
                |_| {},
            )
            .unwrap();
        assert_eq!(outcome.directory, temporary.root);
        assert_eq!(second.directory, outcome.directory);
        assert!(second.directory.join("00001_图片 A_2.png").is_file());
    }

    #[test]
    fn selected_export_keeps_original_names_without_overwriting_duplicates() {
        let temporary = TemporaryImageFilesExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let first_dir = temporary.root.join("first");
        let second_dir = temporary.root.join("second");
        fs::create_dir_all(&first_dir).unwrap();
        fs::create_dir_all(&second_dir).unwrap();
        let first = first_dir.join("same.png");
        let second = second_dir.join("same.png");
        let first_bytes = metadata_png_bytes("first", None);
        let second_bytes = metadata_png_bytes("second", None);
        fs::write(&first, &first_bytes).unwrap();
        fs::write(&second, &second_bytes).unwrap();
        directory
            .open_database()
            .unwrap()
            .append_batch(
                SourceType::Folder,
                &temporary.root.to_string_lossy(),
                &[
                    NewRow {
                        source_ordinal: 1,
                        identity: "file:first".into(),
                        image_path: Some(first.to_string_lossy().into_owned()),
                        ..NewRow::default()
                    },
                    NewRow {
                        source_ordinal: 2,
                        identity: "file:second".into(),
                        image_path: Some(second.to_string_lossy().into_owned()),
                        ..NewRow::default()
                    },
                ],
                |_| Ok(()),
            )
            .unwrap();

        let outcome = directory
            .export_selected_images(
                &RowSelection::Explicit {
                    row_ids: vec![1, 2],
                },
                &[],
                &temporary.root,
                ImageFileNaming::Original,
                false,
                |_| {},
            )
            .unwrap();

        assert_eq!(outcome.directory, temporary.root);
        assert_eq!(fs::read(outcome.directory.join("same.png")).unwrap(), first_bytes);
        assert_eq!(
            fs::read(outcome.directory.join("same_2.png")).unwrap(),
            second_bytes
        );
    }

    #[test]
    fn selected_export_passes_builtin_detection_and_removes_all_png_metadata() {
        let temporary = TemporaryImageFilesExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let source = temporary.root.join("metadata.png");
        let mut source_bytes =
            stealth_png_bytes("artist:source", r#"{"seed":42,"uc":"bad hands"}"#);
        insert_text_chunk_after_ihdr(&mut source_bytes, "Description", "artist:source");
        insert_text_chunk_after_ihdr(
            &mut source_bytes,
            "Comment",
            r#"{"seed":42,"uc":"bad hands"}"#,
        );
        insert_png_chunk_after_ihdr(
            &mut source_bytes,
            *b"eXIf",
            b"Exif\0\0II*\0\x08\0\0\0\0\0\0\0",
        );
        insert_png_chunk_after_ihdr(&mut source_bytes, *b"pHYs", &[0, 0, 0, 72, 0, 0, 0, 72, 1]);
        insert_png_chunk_after_ihdr(&mut source_bytes, *b"tIME", &[0x07, 0xe8, 1, 2, 3, 4, 5]);
        insert_png_chunk_after_ihdr(&mut source_bytes, *b"raNd", b"private metadata");
        fs::write(&source, &source_bytes).unwrap();
        directory
            .open_database()
            .unwrap()
            .append_batch(
                SourceType::Folder,
                &temporary.root.to_string_lossy(),
                &[NewRow {
                    source_ordinal: 1,
                    identity: "file:metadata".into(),
                    image_path: Some(source.to_string_lossy().into_owned()),
                    ..NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();

        let outcome = directory
            .export_selected_images(
                &RowSelection::Explicit { row_ids: vec![1] },
                &[],
                &temporary.root,
                ImageFileNaming::Custom(" 自定义命名 ".into()),
                true,
                |_| {},
            )
            .unwrap();
        let exported = outcome.directory.join("自定义命名_1.png");

        assert!(exported.is_file());
        // 与表格导入/元数据检测完全相同的读取器必须检测不到任何文本元数据。
        assert!(read_png_text_chunks(&exported).unwrap().is_empty());
        assert!(read_stealth_png_metadata(&exported).unwrap().is_none());
        assert_eq!(image::image_dimensions(&exported).unwrap(), (64, 64));
        let exported_bytes = fs::read(&exported).unwrap();
        let exported_chunks = png_chunks(&exported_bytes);
        assert!(
            exported_chunks
                .iter()
                .all(|(chunk_type, _)| PNG_METADATA_FREE_CHUNKS.contains(chunk_type))
        );
        for removed in [*b"tEXt", *b"eXIf", *b"pHYs", *b"tIME", *b"raNd"] {
            assert!(!exported_chunks.iter().any(|(kind, _)| *kind == removed));
        }
        let source_pixels = image::load_from_memory(&source_bytes).unwrap().to_rgba8();
        let exported_pixels = image::load_from_memory(&exported_bytes).unwrap().to_rgba8();
        for (before, after) in source_pixels.pixels().zip(exported_pixels.pixels()) {
            assert_eq!(&before.0[..3], &after.0[..3]);
            assert_eq!(after.0[3], before.0[3] | 1);
        }

        // 抹除只作用于导出副本，原图仍能被表格检测出 Description/Comment。
        assert_eq!(
            read_png_text_chunks(&source)
                .unwrap()
                .get("Description")
                .map(String::as_str),
            Some("artist:source")
        );
        assert_eq!(
            read_png_text_chunks(&source)
                .unwrap()
                .get("Comment")
                .map(String::as_str),
            Some(r#"{"seed":42,"uc":"bad hands"}"#)
        );
        assert_eq!(
            read_stealth_png_metadata(&source)
                .unwrap()
                .unwrap()
                .get("Description")
                .map(String::as_str),
            Some("artist:source")
        );
        assert_eq!(fs::read(&source).unwrap(), source_bytes);
    }

    fn stealth_png_bytes(description: &str, comment: &str) -> Vec<u8> {
        let metadata = serde_json::json!({
            "Description": description,
            "Comment": comment,
            "Source": "NovelAI"
        });
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(serde_json::to_string(&metadata).unwrap().as_bytes())
            .unwrap();
        let compressed = encoder.finish().unwrap();
        let mut payload = b"stealth_pngcomp".to_vec();
        payload.extend_from_slice(
            &u32::try_from(compressed.len() * 8)
                .unwrap()
                .to_be_bytes(),
        );
        payload.extend_from_slice(&compressed);

        let mut image = image::RgbaImage::from_pixel(64, 64, image::Rgba([12, 34, 56, 255]));
        let height = image.height() as usize;
        for (position, bit) in payload
            .iter()
            .flat_map(|byte| (0..8).map(move |shift| (byte >> (7 - shift)) & 1))
            .enumerate()
        {
            let x = position / height;
            let y = position % height;
            let pixel = image.get_pixel_mut(x as u32, y as u32);
            pixel.0[3] = (pixel.0[3] & 0xfe) | bit;
        }
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        encoded.into_inner()
    }

    fn insert_text_chunk_after_ihdr(png: &mut Vec<u8>, keyword: &str, text: &str) {
        let mut data = keyword.as_bytes().to_vec();
        data.push(0);
        data.extend_from_slice(text.as_bytes());
        insert_png_chunk_after_ihdr(png, *b"tEXt", &data);
    }

    fn insert_png_chunk_after_ihdr(png: &mut Vec<u8>, chunk_type: [u8; 4], data: &[u8]) {
        const AFTER_IHDR: usize = 8 + 4 + 4 + 13 + 4;
        let mut encoded = Vec::with_capacity(12 + data.len());
        encoded.extend_from_slice(&(data.len() as u32).to_be_bytes());
        encoded.extend_from_slice(&chunk_type);
        encoded.extend_from_slice(data);
        let mut crc = flate2::Crc::new();
        crc.update(&chunk_type);
        crc.update(data);
        encoded.extend_from_slice(&crc.sum().to_be_bytes());
        png.splice(AFTER_IHDR..AFTER_IHDR, encoded);
    }

    fn png_chunks(bytes: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        assert_eq!(bytes.get(..8), Some(PNG_SIGNATURE.as_slice()));
        let mut cursor = 8_usize;
        let mut chunks = Vec::new();
        while cursor < bytes.len() {
            let length = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
            let chunk_type = bytes[cursor + 4..cursor + 8].try_into().unwrap();
            let data_start = cursor + 8;
            let data_end = data_start + length;
            chunks.push((chunk_type, bytes[data_start..data_end].to_vec()));
            cursor = data_end + 4;
            if chunk_type == *b"IEND" {
                break;
            }
        }
        chunks
    }

    #[test]
    fn export_workers_adapt_to_cpu_memory_and_copy_workloads() {
        let mib = 1024_u64 * 1024;

        assert_eq!(export_worker_limit(2, 10_000, true, 128 * mib), 1);
        assert_eq!(export_worker_limit(4, 10_000, true, 128 * mib), 2);
        assert_eq!(export_worker_limit(8, 10_000, true, 128 * mib), 3);
        assert_eq!(export_worker_limit(32, 10_000, true, 128 * mib), 4);
        assert_eq!(export_worker_limit(32, 10_000, false, 1), 4);
        assert_eq!(export_worker_limit(32, 2, true, 1), 2);
        assert_eq!(export_worker_limit(32, 0, true, 1), 0);
    }

    #[test]
    fn random_naming_is_opaque_and_keeps_the_extension() {
        let source = Path::new("sample.PNG");
        let first =
            selected_output_file_name(&ImageFileNaming::Random, 1, 123, 10, source);
        let second =
            selected_output_file_name(&ImageFileNaming::Random, 2, 123, 11, source);

        assert_eq!(first.len(), 20);
        assert!(first.ends_with(".png"));
        assert!(first[..16].chars().all(|character| character.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn resolves_folder_managed_original_when_external_path_moved() {
        let temporary = TemporaryImageFilesExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let managed = directory.files_path().join("1").join("original.png");
        fs::create_dir_all(managed.parent().unwrap()).unwrap();
        fs::write(&managed, b"complete-original").unwrap();
        directory
            .open_database()
            .unwrap()
            .append_batch(
                SourceType::Folder,
                r"D:\old",
                &[NewRow {
                    source_ordinal: 1,
                    identity: r"file:d:\old\original.png".into(),
                    image_path: Some(r"D:\old\original.png".into()),
                    stored_image_rel: Some("original.png".into()),
                    ..NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();

        let locator = directory
            .open_database()
            .unwrap()
            .row_image_locator(1)
            .unwrap();

        assert!(locator.stored_image_is_original);
        assert_eq!(
            resolve_original_source(&directory, &locator).unwrap(),
            managed
        );
    }

    struct TemporaryImageFilesExport {
        root: PathBuf,
        data: PathBuf,
    }

    impl TemporaryImageFilesExport {
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
                "smart-spreadsheet-export-images-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&root).unwrap();
            Self {
                data: root.join("data"),
                root,
            }
        }
    }

    impl Drop for TemporaryImageFilesExport {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
