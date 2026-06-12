use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local};
use thiserror::Error;

use super::{DataDirectory, StagingDir, StorageError, canonical_display_path};
use crate::db::identity::{archive_member_identity, file_identity};
use crate::db::{DatabaseError, NewRow, SourceType};
use crate::pipeline::archive::{ArchiveError, archive_extension, extract_archive};
use crate::pipeline::scan::{ScanError, SourceImage, collect_png_files};
use crate::pipeline::{parallel, parse_novelai_metadata, png_text};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageImportStage {
    /// 解压压缩包（仅压缩包输入）。
    Extracting,
    /// 扫描 PNG 文件。
    Scanning,
    /// 读取元数据并落位副本。
    Processing,
}

impl ImageImportStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Extracting => "extracting",
            Self::Scanning => "scanning",
            Self::Processing => "processing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageImportProgress {
    pub stage: ImageImportStage,
    pub processed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageImportOutcome {
    pub batch_id: i64,
    pub source_type: SourceType,
    pub total_found: usize,
    pub added: u64,
    pub skipped_existing: u64,
    pub changed_existing: u64,
    /// 新增行中元数据解析失败的数量（行仍然入库并带失败标记）。
    pub metadata_failed: u64,
}

#[derive(Debug, Error)]
pub enum ImageImportError {
    #[error("导入文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("应用数据目录不可用: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Archive(#[from] ArchiveError),
    #[error("{0}")]
    Scan(#[from] ScanError),
    #[error("数据库写入失败: {0}")]
    Database(#[from] DatabaseError),
    #[error("输入中没有找到 PNG 图片: {0}")]
    NoImagesFound(PathBuf),
}

impl DataDirectory {
    /// 从 PNG 文件夹、单个 PNG 或 zip/7z/rar 压缩包追加导入图片元数据。
    ///
    /// 已入库（身份键相同）的图片跳过；压缩包图片提取副本到 `files/<批次ID>/`，
    /// 文件夹图片直接引用原路径。元数据解析失败的图片仍然入库并带失败标记。
    /// 原始输入全程只读。
    pub fn import_images(
        &self,
        input: &Path,
        progress: impl Fn(ImageImportProgress) + Sync,
    ) -> Result<ImageImportOutcome, ImageImportError> {
        let reporter = ProgressReporter::new(progress);
        let input_display = canonical_display_path(input);

        // 压缩包先解压到运行临时目录；文件夹与单 PNG 直接扫描。
        let is_archive = input.is_file() && archive_extension(input).is_some();
        let mut run_temp: Option<RunTempDir> = None;
        let (scan_root, source_type) = if is_archive {
            reporter.emit(ImageImportStage::Extracting, 0, 0, true);
            let temp = RunTempDir::create()?;
            let extract_dir = temp.path().join("archive");
            extract_archive(input, &extract_dir)?;
            run_temp = Some(temp);
            (extract_dir, SourceType::Archive)
        } else {
            (input.to_owned(), SourceType::Folder)
        };

        reporter.emit(ImageImportStage::Scanning, 0, 0, true);
        let images = collect_png_files(&scan_root)?;
        if images.is_empty() {
            return Err(ImageImportError::NoImagesFound(input.to_owned()));
        }
        let total_found = images.len();
        reporter.emit(ImageImportStage::Scanning, total_found, total_found, true);

        // 身份键：文件夹图用规范化根目录拼相对路径（避免逐文件 canonicalize），
        // 压缩包图用压缩包路径 + 包内相对路径。
        let scan_root_display = if source_type == SourceType::Archive {
            input_display.clone()
        } else if input.is_file() {
            // 单 PNG：display 即文件本身，identity 基于其所在目录拼文件名。
            Path::new(&input_display)
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_else(|| input_display.clone())
        } else {
            input_display.clone()
        };
        let identity_for = |image: &SourceImage| -> String {
            match source_type {
                SourceType::Archive => {
                    archive_member_identity(&input_display, &image.relative_path)
                }
                _ => file_identity(&format!(
                    "{scan_root_display}\\{}",
                    image.relative_path
                )),
            }
        };
        let identities: Vec<String> = images.iter().map(identity_for).collect();

        let mut database = self.open_database()?;
        let existing = database.existing_identities(&identities)?;

        // 拆分已存在与新增：已存在的行只带身份键与变化检测字段，不读元数据。
        let mut rows: Vec<Option<NewRow>> = Vec::with_capacity(images.len());
        let mut new_jobs: Vec<(usize, SourceImage)> = Vec::new();
        for (index, (image, identity)) in images.iter().zip(&identities).enumerate() {
            let ordinal = u32::try_from(index + 1).map_err(|_| DatabaseError::RowCountOverflow)?;
            if existing.contains(identity) {
                rows.push(Some(NewRow {
                    source_ordinal: ordinal,
                    identity: identity.clone(),
                    source_size: i64::try_from(image.size).ok(),
                    source_mtime: image.modified_nanos,
                    ..NewRow::default()
                }));
            } else {
                rows.push(None);
                new_jobs.push((index, image.clone()));
            }
        }

        // 新图并行处理：读文本 chunk → 解析元数据；压缩包图移动副本到暂存目录。
        let staging = StagingDir::create(&self.files_path())?;
        let new_total = new_jobs.len();
        reporter.emit(ImageImportStage::Processing, 0, new_total, true);
        let worker_count = parallel::worker_count(new_total);
        let processed: Vec<Result<(usize, NewRow), std::io::Error>> = parallel::parallel_map(
            new_jobs,
            worker_count,
            |_, (index, image)| {
                let row = process_new_image(
                    &image,
                    &identities[index],
                    index,
                    source_type,
                    &input_display,
                    &scan_root_display,
                    staging.path(),
                )?;
                Ok((index, row))
            },
            |completed| {
                reporter.emit(
                    ImageImportStage::Processing,
                    completed,
                    new_total,
                    completed == new_total,
                );
            },
        );

        let mut metadata_failed: u64 = 0;
        for result in processed {
            let (index, row) = result?;
            if row.metadata_failed {
                metadata_failed += 1;
            }
            rows[index] = Some(row);
        }
        let rows: Vec<NewRow> = rows
            .into_iter()
            .map(|row| row.expect("every scanned image produces a candidate row"))
            .collect();

        let files_root = self.files_path();
        let has_staged_files = source_type == SourceType::Archive;
        let outcome = database.append_batch(source_type, &input_display, &rows, |batch_id| {
            if !has_staged_files {
                return Ok(());
            }
            let target = files_root.join(batch_id.to_string());
            fs::rename(staging.path(), &target)
                .map_err(|error| format!("无法落位批次文件目录 {}: {error}", target.display()))
        })?;

        drop(run_temp);
        Ok(ImageImportOutcome {
            batch_id: outcome.batch_id,
            source_type,
            total_found,
            added: outcome.added,
            skipped_existing: outcome.skipped_existing,
            changed_existing: outcome.changed_existing,
            metadata_failed,
        })
    }
}

fn process_new_image(
    image: &SourceImage,
    identity: &str,
    scan_index: usize,
    source_type: SourceType,
    input_display: &str,
    scan_root_display: &str,
    staging_root: &Path,
) -> Result<NewRow, std::io::Error> {
    let (positive, negative, artists, metadata_failed) =
        match png_text::read_png_text_chunks(&image.absolute_path) {
            Ok(chunks) => {
                let metadata = parse_novelai_metadata(&chunks);
                (
                    nonempty_string(metadata.positive_prompt),
                    nonempty_string(metadata.negative_prompt),
                    nonempty_string(metadata.artist_tags.join("\n")),
                    false,
                )
            }
            Err(_) => (None, None, None, true),
        };

    let (image_path, stored_image_rel) = match source_type {
        SourceType::Archive => {
            // 副本移动到暂存目录（同盘瞬间完成，跨盘回退复制），保持包内目录结构。
            let staged = staging_root.join(&image.relative_path);
            if let Some(parent) = staged.parent() {
                fs::create_dir_all(parent)?;
            }
            if fs::rename(&image.absolute_path, &staged).is_err() {
                fs::copy(&image.absolute_path, &staged)?;
            }
            (
                format!("{input_display} > {}", image.relative_path),
                Some(image.relative_path.replace('\\', "/")),
            )
        }
        _ => (
            format!("{scan_root_display}\\{}", image.relative_path),
            None,
        ),
    };

    Ok(NewRow {
        source_ordinal: u32::try_from(scan_index + 1).unwrap_or(u32::MAX),
        identity: identity.to_owned(),
        source_size: i64::try_from(image.size).ok(),
        source_mtime: image.modified_nanos,
        content_hash: None,
        time: image.created.map(format_local_time),
        positive_prompt: positive,
        negative_prompt: negative,
        artists,
        image_folder: None,
        image_path: Some(image_path),
        stored_image_rel,
        metadata_failed,
    })
}

fn nonempty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn format_local_time(time: SystemTime) -> String {
    let datetime: DateTime<Local> = time.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

struct ProgressReporter<F: Fn(ImageImportProgress) + Sync> {
    callback: F,
    last_emit: Mutex<Option<Instant>>,
}

impl<F: Fn(ImageImportProgress) + Sync> ProgressReporter<F> {
    fn new(callback: F) -> Self {
        Self {
            callback,
            last_emit: Mutex::new(None),
        }
    }

    /// 进度事件按最小间隔节流，`force` 用于阶段切换和完成事件。
    fn emit(&self, stage: ImageImportStage, processed: usize, total: usize, force: bool) {
        let now = Instant::now();
        {
            let mut last = self.last_emit.lock().expect("progress lock");
            if !force
                && let Some(previous) = *last
                && now.duration_since(previous) < PROGRESS_INTERVAL
            {
                return;
            }
            *last = Some(now);
        }
        (self.callback)(ImageImportProgress {
            stage,
            processed,
            total,
        });
    }
}

/// 运行临时目录（压缩包解压用），位于 `D:\Agent\Agent_temp`，结束时清理。
struct RunTempDir {
    path: PathBuf,
}

impl RunTempDir {
    fn create() -> Result<Self, std::io::Error> {
        let parent = Path::new(r"D:\Agent\Agent_temp");
        let parent = if parent.is_dir() {
            parent.join("smart-spreadsheet-import")
        } else {
            std::env::temp_dir().join("smart-spreadsheet-import")
        };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = parent.join(format!("{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RunTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::db::RowSelection;

    /// 仅含文本元数据的最小 PNG（签名 + tEXt + IEND，CRC 占位即可，
    /// 文本读取器跳过 CRC 校验）。
    fn create_metadata_png(path: &Path, description: &str) {
        let mut data = b"Description\0".to_vec();
        data.extend(description.as_bytes());
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        for (chunk_type, payload) in [(b"tEXt", data.as_slice()), (b"IEND", &[][..])] {
            png.extend((payload.len() as u32).to_be_bytes());
            png.extend(chunk_type);
            png.extend(payload);
            png.extend(0_u32.to_be_bytes());
        }
        fs::write(path, png).unwrap();
    }

    #[test]
    fn imports_folder_then_appends_only_new_images() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("input");
        fs::create_dir_all(input.join("nested")).unwrap();
        create_metadata_png(&input.join("a.png"), "best quality, artist:alpha");
        create_metadata_png(&input.join("nested").join("b.png"), "scenery");
        fs::write(input.join("broken.png"), b"not a png").unwrap();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();

        let outcome = directory.import_images(&input, |_| {}).unwrap();

        assert_eq!(outcome.total_found, 3);
        assert_eq!(outcome.added, 3);
        assert_eq!(outcome.skipped_existing, 0);
        assert_eq!(outcome.metadata_failed, 1);
        assert_eq!(outcome.source_type, SourceType::Folder);

        let mut database = directory.open_database().unwrap();
        let page = database
            .query_rows(&crate::db::RowQuery {
                offset: 0,
                limit: 10,
                tags: Vec::new(),
                tag_mode: crate::db::TagMatchMode::And,
            })
            .unwrap();
        assert_eq!(page.total_count, 3);
        let first = &page.rows[0];
        assert_eq!(
            first.positive_prompt.as_deref(),
            Some("best quality, artist:alpha")
        );
        assert_eq!(first.artists.as_deref(), Some("artist:alpha"));
        assert!(!first.metadata_failed);
        assert!(first.image_path.as_deref().unwrap().ends_with("a.png"));
        assert!(first.time.is_some());
        let failed = page
            .rows
            .iter()
            .find(|row| row.metadata_failed)
            .expect("broken png should be imported with failure flag");
        assert!(failed.positive_prompt.is_none());

        // 追加 2 张新图后重新导入：只新增 2，已有 3 张跳过。
        create_metadata_png(&input.join("c.png"), "new one");
        create_metadata_png(&input.join("d.png"), "new two");
        let second = directory.import_images(&input, |_| {}).unwrap();
        assert_eq!(second.added, 2);
        assert_eq!(second.skipped_existing, 3);
        assert_eq!(
            directory
                .open_database()
                .unwrap()
                .library_summary()
                .unwrap()
                .row_count,
            5
        );
    }

    #[test]
    fn imports_zip_archive_with_stored_copies_and_skips_reimport() {
        let temporary = TemporaryImageImport::new();
        fs::create_dir_all(&temporary.root).unwrap();
        let png_path = temporary.root.join("inner.png");
        create_metadata_png(&png_path, "artist:zip-sample");
        let archive_path = temporary.root.join("pack.zip");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("套图/图片 1.png", options).unwrap();
            writer.write_all(&fs::read(&png_path).unwrap()).unwrap();
            writer.finish().unwrap();
        }
        let directory = DataDirectory::initialize(&temporary.data).unwrap();

        let outcome = directory.import_images(&archive_path, |_| {}).unwrap();

        assert_eq!(outcome.added, 1);
        assert_eq!(outcome.source_type, SourceType::Archive);
        let database = directory.open_database().unwrap();
        let locator = database.row_image_locator(1).unwrap();
        let stored = locator.stored_image_path.unwrap();
        assert!(stored.starts_with(&format!("files/{}/", outcome.batch_id)));
        assert_eq!(
            fs::read(directory.root().join(&stored)).unwrap(),
            fs::read(&png_path).unwrap()
        );
        assert!(locator.image_path.unwrap().contains(" > "));

        // 同一压缩包重复导入：全部跳过，不留新批次目录。
        let repeat = directory.import_images(&archive_path, |_| {}).unwrap();
        assert_eq!(repeat.added, 0);
        assert_eq!(repeat.skipped_existing, 1);
        assert_eq!(
            directory
                .open_database()
                .unwrap()
                .library_summary()
                .unwrap()
                .row_count,
            1
        );
    }

    #[test]
    fn emits_progress_and_rejects_empty_input() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("input");
        fs::create_dir_all(&input).unwrap();
        create_metadata_png(&input.join("only.png"), "solo");
        let directory = DataDirectory::initialize(&temporary.data).unwrap();

        let events = Mutex::new(Vec::new());
        directory
            .import_images(&input, |progress| {
                events.lock().unwrap().push(progress);
            })
            .unwrap();
        let events = events.into_inner().unwrap();
        assert!(
            events
                .iter()
                .any(|event| event.stage == ImageImportStage::Scanning)
        );
        assert!(events.iter().any(|event| {
            event.stage == ImageImportStage::Processing && event.processed == event.total
        }));

        let empty = temporary.root.join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(matches!(
            directory.import_images(&empty, |_| {}),
            Err(ImageImportError::NoImagesFound(_))
        ));
    }

    /// M14 验收：万行库追加导入 5 张图 → 10005 行；重复导入不翻倍。
    #[test]
    fn appends_five_images_to_ten_thousand_row_library() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("pack");
        fs::create_dir_all(&input).unwrap();
        for index in 1..=5 {
            create_metadata_png(&input.join(format!("new-{index}.png")), "appended");
        }
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        {
            let mut database = directory.open_database().unwrap();
            crate::db::test_support::append_rows(
                &mut database,
                &crate::db::test_support::test_rows(10_000),
            );
        }

        let outcome = directory.import_images(&input, |_| {}).unwrap();
        assert_eq!(outcome.added, 5);
        assert_eq!(outcome.skipped_existing, 0);

        let summary = directory.open_database().unwrap().library_summary().unwrap();
        assert_eq!(summary.row_count, 10_005);

        let repeat = directory.import_images(&input, |_| {}).unwrap();
        assert_eq!(repeat.added, 0);
        assert_eq!(repeat.skipped_existing, 5);
        assert_eq!(
            directory
                .open_database()
                .unwrap()
                .library_summary()
                .unwrap()
                .row_count,
            10_005
        );
    }

    #[test]
    fn deleted_archive_row_cleans_stored_copy_and_can_reimport() {
        let temporary = TemporaryImageImport::new();
        fs::create_dir_all(&temporary.root).unwrap();
        let png_path = temporary.root.join("inner.png");
        create_metadata_png(&png_path, "artist:cycle");
        let archive_path = temporary.root.join("cycle.zip");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("one.png", options).unwrap();
            writer.write_all(&fs::read(&png_path).unwrap()).unwrap();
            writer.finish().unwrap();
        }
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        directory.import_images(&archive_path, |_| {}).unwrap();

        let report = directory
            .delete_rows(&RowSelection::Explicit { row_ids: vec![1] })
            .unwrap();
        assert_eq!(report.deleted_rows, 1);
        assert_eq!(report.removed_files, 1);

        let again = directory.import_images(&archive_path, |_| {}).unwrap();
        assert_eq!(again.added, 1);
    }

    struct TemporaryImageImport {
        root: PathBuf,
        data: PathBuf,
    }

    impl TemporaryImageImport {
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
                "smart-spreadsheet-image-import-{}-{nonce}",
                std::process::id()
            ));
            Self {
                data: root.join("data"),
                root,
            }
        }
    }

    impl Drop for TemporaryImageImport {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
