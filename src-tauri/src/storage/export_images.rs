use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{DataDirectory, StorageError};
use crate::db::{ExportRow, RowSelection, TagMutationError};

const PROGRESS_EVERY_FILES: usize = 20;
const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
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
}

impl DataDirectory {
    /// 把选中行的图片导出到目标文件夹下新建的输出目录，
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

        let output_dir = create_unique_output_dir(parent_dir, "智能表格图片导出")?;
        let mut exported = 0;
        let mut hardlink_fallbacks = 0;
        let mut missing = 0;

        for (index, row) in rows.iter().enumerate() {
            match resolve_source(self, row) {
                Some(source) => {
                    let file_name = output_file_name(index + 1, &source);
                    let target = output_dir.join(file_name);
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
    /// 始终在目标文件夹内创建独立输出目录，避免覆盖用户已有文件。
    /// `strip_metadata` 只重新编码导出副本，来源文件和资料库均保持不变。
    pub fn export_selected_images(
        &self,
        selection: &RowSelection,
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
        if rows.is_empty() {
            return Err(ImageFilesExportError::EmptySelection);
        }

        let total = rows.len();
        progress(ImageFilesProgress {
            processed: 0,
            total,
        });
        let output_dir = create_unique_output_dir(parent_dir, "智能表格图片导出")?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut exported = 0;
        let mut missing = 0;

        for (index, row) in rows.iter().enumerate() {
            match resolve_source(self, row) {
                Some(source) => {
                    let file_name =
                        selected_output_file_name(&naming, exported + 1, nonce, row.id, &source);
                    let target = unique_output_target(&output_dir, &file_name);
                    write_exported_copy(&source, &target, strip_metadata)?;
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

fn write_exported_copy(
    source: &Path,
    target: &Path,
    strip_metadata: bool,
) -> Result<(), ImageFilesExportError> {
    if !strip_metadata {
        fs::copy(source, target)?;
        return Ok(());
    }

    if has_png_signature(source)? {
        if let Err(error) = write_png_without_metadata(source, target) {
            let _ = fs::remove_file(target);
            return Err(error.into());
        }
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
    image.save_with_format(target, format)?;
    Ok(())
}

fn has_png_signature(path: &Path) -> io::Result<bool> {
    let mut file = File::open(path)?;
    let mut signature = [0_u8; PNG_SIGNATURE.len()];
    match file.read_exact(&mut signature) {
        Ok(()) => Ok(signature == PNG_SIGNATURE),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(error),
    }
}

/// 无损复制 PNG 的像素数据，只保留构成静态图像必需的块。
///
/// PNG 的文本、EXIF、ICC、时间、物理尺寸、颜色建议以及私有附加块都会被丢弃。
/// PLTE 和 tRNS 可能直接参与调色板与透明度显示，因此必须保留。
fn write_png_without_metadata(source: &Path, target: &Path) -> io::Result<()> {
    let mut reader = BufReader::new(File::open(source)?);
    let mut writer = BufWriter::new(File::create(target)?);
    let mut signature = [0_u8; PNG_SIGNATURE.len()];
    reader.read_exact(&mut signature)?;
    if signature != PNG_SIGNATURE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("不是有效的 PNG 文件: {}", source.display()),
        ));
    }
    writer.write_all(&PNG_SIGNATURE)?;

    let mut chunk_index = 0_usize;
    let mut saw_idat = false;
    loop {
        let mut length_bytes = [0_u8; 4];
        reader.read_exact(&mut length_bytes).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("PNG 数据块不完整（缺少 IEND）: {}", source.display()),
            )
        })?;
        let length = u32::from_be_bytes(length_bytes) as u64;
        let mut chunk_type = [0_u8; 4];
        reader.read_exact(&mut chunk_type)?;

        if chunk_index == 0 && (chunk_type != *b"IHDR" || length != 13) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("PNG 首块不是有效 IHDR: {}", source.display()),
            ));
        }
        if chunk_type[0].is_ascii_uppercase() && !PNG_METADATA_FREE_CHUNKS.contains(&chunk_type) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "PNG 含有无法安全丢弃的未知关键块 {}: {}",
                    String::from_utf8_lossy(&chunk_type),
                    source.display()
                ),
            ));
        }

        let keep = PNG_METADATA_FREE_CHUNKS.contains(&chunk_type);
        if keep {
            writer.write_all(&length_bytes)?;
            writer.write_all(&chunk_type)?;
        }
        let mut chunk_body = reader.by_ref().take(length + 4);
        let copied = if keep {
            io::copy(&mut chunk_body, &mut writer)?
        } else {
            io::copy(&mut chunk_body, &mut io::sink())?
        };
        if copied != length + 4 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "PNG {} 块不完整: {}",
                    String::from_utf8_lossy(&chunk_type),
                    source.display()
                ),
            ));
        }

        chunk_index += 1;
        saw_idat |= chunk_type == *b"IDAT";
        if chunk_type == *b"IEND" {
            if length != 0 || !saw_idat {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("PNG 缺少图像数据或 IEND 无效: {}", source.display()),
                ));
            }
            break;
        }
    }
    writer.flush()
}

fn output_file_name(ordinal: usize, source: &Path) -> String {
    format!("{ordinal:05}_{}", sanitized_source_file_name(source))
}

fn create_unique_output_dir(parent_dir: &Path, base_name: &str) -> io::Result<PathBuf> {
    for index in 0_usize.. {
        let candidate_name = if index == 0 {
            base_name.to_owned()
        } else {
            format!("{base_name}_{index}")
        };
        let candidate = parent_dir.join(candidate_name);
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    unreachable!("unbounded output folder numbering always finds a candidate")
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::db::{NewRow, SourceType, TagMatchMode};
    use crate::pipeline::png_text::read_png_text_chunks;
    use crate::storage::test_fixtures::metadata_png_bytes;

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
                    has_vibe: false,
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

        // 再导一次：输出目录自动编号，不混入旧输出。
        let second = directory
            .export_image_files(
                &RowSelection::Explicit { row_ids: vec![1] },
                &temporary.root,
                ImageFileExportMode::Hardlink,
                |_| {},
            )
            .unwrap();
        assert_ne!(second.directory, outcome.directory);
        assert!(second.directory.join("00001_图片 A.png").is_file());
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
                &temporary.root,
                ImageFileNaming::Original,
                false,
                |_| {},
            )
            .unwrap();

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
        let mut source_bytes = metadata_png_bytes("artist:source", Some(r#"{"seed":42}"#));
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
        assert_eq!(image::image_dimensions(&exported).unwrap(), (16, 16));
        let exported_bytes = fs::read(&exported).unwrap();
        let exported_chunks = png_chunks(&exported_bytes);
        assert!(
            exported_chunks
                .iter()
                .all(|(chunk_type, _)| PNG_METADATA_FREE_CHUNKS.contains(chunk_type))
        );
        assert_eq!(
            png_chunk_payloads(&source_bytes, *b"IDAT"),
            png_chunk_payloads(&exported_bytes, *b"IDAT")
        );
        for removed in [*b"tEXt", *b"eXIf", *b"pHYs", *b"tIME", *b"raNd"] {
            assert!(!exported_chunks.iter().any(|(kind, _)| *kind == removed));
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
            Some(r#"{"seed":42}"#)
        );
        assert_eq!(fs::read(&source).unwrap(), source_bytes);
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

    fn png_chunk_payloads(bytes: &[u8], expected_type: [u8; 4]) -> Vec<Vec<u8>> {
        png_chunks(bytes)
            .into_iter()
            .filter_map(|(chunk_type, data)| (chunk_type == expected_type).then_some(data))
            .collect()
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
