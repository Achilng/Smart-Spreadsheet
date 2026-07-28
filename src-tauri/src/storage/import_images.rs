use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local};
use serde::Serialize;
use thiserror::Error;

use super::content_hash::sha256_file;
use super::perceptual_hash::compute_phash;
use super::{DataDirectory, StagingDir, StorageError, canonical_display_path};
use crate::db::identity::{archive_member_identity, file_identity};
use crate::db::{
    DatabaseError, ExistingImageUpdate, NewRow, RuleExecutionSummary, RuleExecutionTrigger,
    SourceType,
};
use crate::fsx::{replace_output_file, unique_sibling_path};
use crate::pipeline::archive::{ArchiveError, archive_extension, extract_archive};
use crate::pipeline::scan::{ScanError, SourceImage, collect_png_files};
use crate::pipeline::{
    cancel, metadata_fingerprint, parallel, parse_novelai_metadata, png_text, stealth_png,
};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageImportStage {
    /// 解压压缩包（仅压缩包输入）。
    Extracting,
    /// 扫描 PNG 文件。
    Scanning,
    /// 为身份键全新的图片计算内容哈希。
    Hashing,
    /// 读取元数据。
    Processing,
    /// 计算感知哈希（pHash）。
    PerceptualHashing,
    /// 把新图片复制/搬移进受管目录（落位副本）。
    Copying,
}

impl ImageImportStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Extracting => "extracting",
            Self::Scanning => "scanning",
            Self::Hashing => "hashing",
            Self::Processing => "processing",
            Self::PerceptualHashing => "perceptualHashing",
            Self::Copying => "copying",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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
    pub skipped_content: u64,
    pub changed_existing: u64,
    /// 因读取失败或正负提示词均为空而拒绝入库的图片数。
    pub metadata_rejected: u64,
    /// 成功移动到用户配置目录的异常图片数。
    pub rejected_moved: u64,
    /// 未能移动到用户配置目录的异常图片数；这些图片仍不入库。
    pub rejected_move_failures: u64,
    pub rule_execution: RuleExecutionSummary,
    pub artist_prefix_enabled: bool,
    pub artist_prefix_scanned_rows: u64,
    pub artist_prefix_changed_rows: u64,
    pub artist_prefix_changed_fields: u64,
    pub artist_prefix_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingImageUpdateOutcome {
    pub source_type: SourceType,
    pub total_found: usize,
    pub matched: u64,
    pub updated: u64,
    /// 由原路径身份键精确匹配的图片数。
    pub matched_by_identity: u64,
    /// 原路径失效后，由完整文件 SHA-256 唯一匹配并重新关联的图片数。
    pub relinked_by_content: u64,
    /// 文件字节变化后，由完整 NovelAI 元数据指纹唯一匹配并重新关联的图片数。
    pub relinked_by_metadata: u64,
    /// SHA 或元数据指向多条旧记录，未自动覆盖的图片数。
    pub ambiguous: u64,
    /// 来源中没有对应资料库身份键的图片；更新模式明确忽略，不追加。
    pub unmatched: u64,
    /// 已匹配但 PNG 元数据读取失败或正负提示词均为空；保留原行。
    pub metadata_rejected: u64,
    /// 已匹配且元数据有效，但受管原图副本刷新失败；保留原行。
    pub copy_failures: u64,
    pub rule_execution: RuleExecutionSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingImageMatchKind {
    Identity,
    ContentHash,
    Metadata,
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
    #[error("异常图片输出目录不能等于或位于导入文件夹内部: {0}")]
    RejectedDirectoryInsideInput(PathBuf),
    #[error("导入已被用户取消，未写入任何数据")]
    Cancelled,
    #[error("更新已被用户取消，未修改任何数据")]
    UpdateCancelled,
}

impl DataDirectory {
    /// 从 PNG 文件夹、单个 PNG 或 zip/7z/rar 压缩包追加导入图片元数据。
    ///
    /// 已入库（身份键相同）的图片跳过；压缩包正常图片提取副本到
    /// `files/<批次ID>/`，文件夹正常图片直接引用原路径。读取失败或正负提示词
    /// 均为空的图片不入库，并移动到用户配置的异常图片目录。
    pub fn import_images(
        &self,
        input: &Path,
        progress: impl Fn(ImageImportProgress) + Sync,
    ) -> Result<ImageImportOutcome, ImageImportError> {
        let reporter = ProgressReporter::new(progress);
        let input_display = canonical_display_path(input);
        let rejected_root = match self.rejected_images_directory()? {
            Some(dir) => dir,
            None => {
                let default = self.default_rejected_images_directory();
                self.set_rejected_images_directory(&default)?;
                default
            }
        };
        if rejected_root.exists() && !rejected_root.is_dir() {
            return Err(ImageImportError::Storage(
                StorageError::RejectedImagesPathNotDirectory(rejected_root),
            ));
        }
        fs::create_dir_all(&rejected_root)?;

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
        validate_rejected_directory(input, source_type, &rejected_root)?;
        if cancel::is_requested() {
            return Err(ImageImportError::Cancelled);
        }

        reporter.emit(ImageImportStage::Scanning, 0, 0, true);
        let images = collect_png_files(&scan_root)?;
        if images.is_empty() {
            return Err(ImageImportError::NoImagesFound(input.to_owned()));
        }
        if cancel::is_requested() {
            return Err(ImageImportError::Cancelled);
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
                _ => file_identity(&format!("{scan_root_display}\\{}", image.relative_path)),
            }
        };
        let identities: Vec<String> = images.iter().map(identity_for).collect();

        let mut database = self.open_database()?;
        let artist_prefix_enabled = database.auto_artist_prefix_on_import()?;
        let existing = database.existing_identities(&identities)?;

        // 拆分已存在与新增：已存在的行只带身份键与变化检测字段，不读元数据。
        let mut indexed_rows: Vec<(usize, NewRow)> = Vec::with_capacity(images.len());
        let mut metadata_jobs: Vec<(usize, SourceImage)> = Vec::new();
        for (index, (image, identity)) in images.iter().zip(&identities).enumerate() {
            let ordinal = u32::try_from(index + 1).map_err(|_| DatabaseError::RowCountOverflow)?;
            if existing.contains(identity) {
                indexed_rows.push((
                    index,
                    NewRow {
                        source_ordinal: ordinal,
                        identity: identity.clone(),
                        source_size: i64::try_from(image.size).ok(),
                        source_mtime: image.modified_nanos,
                        ..NewRow::default()
                    },
                ));
            } else {
                metadata_jobs.push((index, image.clone()));
            }
        }

        // 身份键全新的图片先读取 metadata；异常图片不进入数据库候选，也不参与内容去重。
        // 各并行阶段支持用户取消：取消只发生在写库之前，不会留下半截数据。
        let metadata_total = metadata_jobs.len();
        reporter.emit(ImageImportStage::Processing, 0, metadata_total, true);
        let inspected = parallel::parallel_map_cancellable(
            metadata_jobs,
            parallel::worker_count(metadata_total),
            cancel::flag(),
            |_, (index, image)| (index, inspect_metadata(image)),
            |completed| {
                reporter.emit(
                    ImageImportStage::Processing,
                    completed,
                    metadata_total,
                    completed == metadata_total,
                );
            },
        )
        .ok_or(ImageImportError::Cancelled)?;
        let mut rejected = Vec::new();
        let mut hash_jobs = Vec::new();
        for (index, inspection) in inspected {
            match inspection {
                MetadataInspection::Valid(image) => hash_jobs.push((index, *image)),
                MetadataInspection::Rejected(image) => rejected.push(image),
            }
        }

        // 仅正常图片并行计算内容哈希。按扫描顺序保留首次出现的内容；
        // 库内或本批次重复项只构造最小候选供追加事务计数，不复制副本。
        let hash_total = hash_jobs.len();
        reporter.emit(ImageImportStage::Hashing, 0, hash_total, true);
        let hashed = parallel::parallel_map_cancellable(
            hash_jobs,
            parallel::worker_count(hash_total),
            cancel::flag(),
            |_, (index, image)| {
                let content_hash = sha256_file(&image.source.absolute_path).ok();
                (index, image, content_hash)
            },
            |completed| {
                reporter.emit(
                    ImageImportStage::Hashing,
                    completed,
                    hash_total,
                    completed == hash_total,
                );
            },
        )
        .ok_or(ImageImportError::Cancelled)?;
        let candidate_hashes = hashed
            .iter()
            .filter_map(|(_, _, hash)| hash.clone())
            .collect::<Vec<_>>();
        let mut seen_content: HashSet<String> =
            database.existing_content_hashes(&candidate_hashes)?;
        let mut new_jobs: Vec<(usize, ParsedImage, Option<String>)> = Vec::new();
        for (index, image, content_hash) in hashed {
            let duplicate = content_hash
                .as_ref()
                .is_some_and(|hash| !seen_content.insert(hash.clone()));
            if duplicate {
                indexed_rows.push((
                    index,
                    NewRow {
                        source_ordinal: u32::try_from(index + 1)
                            .map_err(|_| DatabaseError::RowCountOverflow)?,
                        identity: identities[index].clone(),
                        source_size: i64::try_from(image.source.size).ok(),
                        source_mtime: image.source.modified_nanos,
                        content_hash,
                        ..NewRow::default()
                    },
                ));
            } else {
                new_jobs.push((index, image, content_hash));
            }
        }

        // 并行计算感知哈希，再串行构建行（Archive 需要文件操作）。
        let phash_total = new_jobs.len();
        reporter.emit(ImageImportStage::PerceptualHashing, 0, phash_total, true);
        let phash_jobs: Vec<PathBuf> = new_jobs
            .iter()
            .map(|(_, img, _)| img.source.absolute_path.clone())
            .collect();
        let phashes = parallel::parallel_map_cancellable(
            phash_jobs,
            parallel::worker_count(phash_total),
            cancel::flag(),
            |_, path| compute_phash(&path).ok(),
            |completed| {
                reporter.emit(
                    ImageImportStage::PerceptualHashing,
                    completed,
                    phash_total,
                    completed == phash_total,
                );
            },
        )
        .ok_or(ImageImportError::Cancelled)?;

        let staging = StagingDir::create(&self.files_path())?;
        let process_context = ProcessImageContext {
            source_type,
            input_display: &input_display,
            scan_root_display: &scan_root_display,
            staging_root: staging.path(),
        };
        // 落位副本是纯串行文件复制，大批量导入时耗时可观：
        // 逐张响应取消（staging 由 Drop 清理），并让进度条走 Copying 阶段。
        let copy_total = new_jobs.len();
        reporter.emit(ImageImportStage::Copying, 0, copy_total, true);
        for (copied, ((index, image, content_hash), perceptual_hash)) in
            new_jobs.into_iter().zip(phashes).enumerate()
        {
            if cancel::is_requested() {
                return Err(ImageImportError::Cancelled);
            }
            let row = build_new_row(
                image,
                &identities[index],
                index,
                &process_context,
                content_hash,
                perceptual_hash,
            )?;
            indexed_rows.push((index, row));
            reporter.emit(
                ImageImportStage::Copying,
                copied + 1,
                copy_total,
                copied + 1 == copy_total,
            );
        }
        indexed_rows.sort_by_key(|(index, _)| *index);
        let rows = indexed_rows
            .into_iter()
            .map(|(_, row)| row)
            .collect::<Vec<_>>();

        let files_root = self.files_path();
        let has_staged_files = fs::read_dir(staging.path())?.next().is_some();
        // 最后一个可取消检查点：从这里开始进入写库事务，不再响应取消。
        if cancel::is_requested() {
            return Err(ImageImportError::Cancelled);
        }
        let outcome = database.append_batch(source_type, &input_display, &rows, |batch_id| {
            if !has_staged_files {
                return Ok(());
            }
            let target = files_root.join(batch_id.to_string());
            fs::rename(staging.path(), &target)
                .map_err(|error| format!("无法落位批次文件目录 {}: {error}", target.display()))
        })?;
        let rule_execution = database
            .execute_automation_rules(RuleExecutionTrigger::Import, &outcome.added_row_ids)
            .unwrap_or_else(|error| {
                RuleExecutionSummary::failed(
                    RuleExecutionTrigger::Import,
                    outcome.added_row_ids.len(),
                    error,
                )
            });
        let (
            artist_prefix_scanned_rows,
            artist_prefix_changed_rows,
            artist_prefix_changed_fields,
            artist_prefix_error,
        ) = if artist_prefix_enabled {
            match database.apply_confirmed_artist_prefix_to_rows(&outcome.added_row_ids) {
                Ok(result) => (
                    result.scanned_rows,
                    result.changed_rows,
                    result.prompt_fields_changed,
                    None,
                ),
                Err(error) => (0, 0, 0, Some(error.to_string())),
            }
        } else {
            (0, 0, 0, None)
        };

        let metadata_rejected = u64::try_from(rejected.len()).unwrap_or(u64::MAX);
        let mut rejected_moved = 0_u64;
        let mut rejected_move_failures = 0_u64;
        for image in rejected {
            if move_rejected_image(&image, &rejected_root).is_ok() {
                rejected_moved += 1;
            } else {
                rejected_move_failures += 1;
            }
        }

        drop(run_temp);
        Ok(ImageImportOutcome {
            batch_id: outcome.batch_id,
            source_type,
            total_found,
            added: outcome.added,
            skipped_existing: outcome.skipped_existing,
            skipped_content: outcome.skipped_content,
            changed_existing: outcome.changed_existing,
            metadata_rejected,
            rejected_moved,
            rejected_move_failures,
            rule_execution,
            artist_prefix_enabled,
            artist_prefix_scanned_rows,
            artist_prefix_changed_rows,
            artist_prefix_changed_fields,
            artist_prefix_error,
        })
    }
}

impl DataDirectory {
    /// 重新读取来源中已入库图片的元数据和图片指纹，原位更新对应行。
    /// 新图片不追加，来源中缺失的旧图片不删除；Tag、分组和行 ID 均保持不变。
    pub fn update_existing_images(
        &self,
        input: &Path,
        progress: impl Fn(ImageImportProgress) + Sync,
    ) -> Result<ExistingImageUpdateOutcome, ImageImportError> {
        let reporter = ProgressReporter::new(progress);
        let input_display = canonical_display_path(input);
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
        if cancel::is_requested() {
            return Err(ImageImportError::UpdateCancelled);
        }

        reporter.emit(ImageImportStage::Scanning, 0, 0, true);
        let images = collect_png_files(&scan_root)?;
        if images.is_empty() {
            return Err(ImageImportError::NoImagesFound(input.to_owned()));
        }
        if cancel::is_requested() {
            return Err(ImageImportError::UpdateCancelled);
        }
        let total_found = images.len();
        reporter.emit(ImageImportStage::Scanning, total_found, total_found, true);

        let scan_root_display = if source_type == SourceType::Archive {
            input_display.clone()
        } else if input.is_file() {
            Path::new(&input_display)
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_else(|| input_display.clone())
        } else {
            input_display.clone()
        };
        let identities = images
            .iter()
            .map(|image| match source_type {
                SourceType::Archive => {
                    archive_member_identity(&input_display, &image.relative_path)
                }
                _ => file_identity(&format!("{scan_root_display}\\{}", image.relative_path)),
            })
            .collect::<Vec<_>>();
        let image_paths = images
            .iter()
            .map(|image| match source_type {
                SourceType::Archive => {
                    format!("{input_display} > {}", image.relative_path)
                }
                _ => format!("{scan_root_display}\\{}", image.relative_path),
            })
            .collect::<Vec<_>>();

        let mut database = self.open_database()?;
        let exact_targets = database.existing_image_targets(&identities)?;
        let mut assignments = HashMap::new();
        let mut assigned_rows = HashSet::new();
        for (index, identity) in identities.iter().enumerate() {
            if let Some(target) = exact_targets.get(identity).cloned() {
                assigned_rows.insert(target.row_id);
                assignments.insert(index, (target, ExistingImageMatchKind::Identity));
            }
        }
        let matched_by_identity = u64::try_from(assignments.len()).unwrap_or(u64::MAX);

        // 路径匹配优先；为支持原图搬家，未命中路径的正常图片还会继续参与
        // 完整文件 SHA-256 和完整 NovelAI 元数据指纹匹配。
        let processing_total = images.len();
        reporter.emit(ImageImportStage::Processing, 0, processing_total, true);
        let inspected = parallel::parallel_map_cancellable(
            images.into_iter().enumerate().collect::<Vec<_>>(),
            parallel::worker_count(processing_total),
            cancel::flag(),
            |_, (index, image)| (index, inspect_metadata(image)),
            |completed| {
                reporter.emit(
                    ImageImportStage::Processing,
                    completed,
                    processing_total,
                    completed == processing_total,
                );
            },
        )
        .ok_or(ImageImportError::UpdateCancelled)?;
        let mut metadata_rejected = 0_u64;
        let valid = inspected
            .into_iter()
            .filter_map(|(index, inspection)| match inspection {
                MetadataInspection::Valid(image) => Some((index, *image)),
                MetadataInspection::Rejected(_) => {
                    if assignments.contains_key(&index) {
                        metadata_rejected += 1;
                    }
                    None
                }
            })
            .collect::<Vec<_>>();

        let hash_total = valid.len();
        reporter.emit(ImageImportStage::Hashing, 0, hash_total, true);
        let hashed = parallel::parallel_map_cancellable(
            valid,
            parallel::worker_count(hash_total),
            cancel::flag(),
            |_, (index, image)| {
                let content_hash = sha256_file(&image.source.absolute_path).ok();
                (index, image, content_hash)
            },
            |completed| {
                reporter.emit(
                    ImageImportStage::Hashing,
                    completed,
                    hash_total,
                    completed == hash_total,
                );
            },
        )
        .ok_or(ImageImportError::UpdateCancelled)?;

        let content_candidates = hashed
            .iter()
            .filter(|(index, _, _)| !assignments.contains_key(index))
            .filter_map(|(_, _, hash)| hash.clone())
            .collect::<Vec<_>>();
        let targets_by_content =
            database.existing_image_targets_by_content_hash(&content_candidates)?;
        let mut ambiguous_indices = HashSet::new();
        for (index, _, content_hash) in &hashed {
            if assignments.contains_key(index) {
                continue;
            }
            let Some(hash) = content_hash.as_ref() else {
                continue;
            };
            let Some(candidates) = targets_by_content.get(hash) else {
                continue;
            };
            if candidates.len() == 1 && !assigned_rows.contains(&candidates[0].row_id) {
                let target = candidates[0].clone();
                assigned_rows.insert(target.row_id);
                assignments.insert(*index, (target, ExistingImageMatchKind::ContentHash));
            } else {
                ambiguous_indices.insert(*index);
            }
        }

        let metadata_candidates = hashed
            .iter()
            .filter(|(index, _, _)| {
                !assignments.contains_key(index) && !ambiguous_indices.contains(index)
            })
            .filter_map(|(_, image, _)| image.metadata_fingerprint.clone())
            .collect::<Vec<_>>();
        let targets_by_metadata =
            database.existing_image_targets_by_metadata_fingerprint(&metadata_candidates)?;
        for (index, image, _) in &hashed {
            if assignments.contains_key(index) || ambiguous_indices.contains(index) {
                continue;
            }
            let Some(fingerprint) = image.metadata_fingerprint.as_ref() else {
                continue;
            };
            let Some(candidates) = targets_by_metadata.get(fingerprint) else {
                continue;
            };
            if candidates.len() == 1 && !assigned_rows.contains(&candidates[0].row_id) {
                let target = candidates[0].clone();
                assigned_rows.insert(target.row_id);
                assignments.insert(*index, (target, ExistingImageMatchKind::Metadata));
            } else {
                ambiguous_indices.insert(*index);
            }
        }

        let relinked_by_content = assignments
            .values()
            .filter(|(_, kind)| *kind == ExistingImageMatchKind::ContentHash)
            .count();
        let relinked_by_metadata = assignments
            .values()
            .filter(|(_, kind)| *kind == ExistingImageMatchKind::Metadata)
            .count();
        let prepared_jobs = hashed
            .into_iter()
            .filter_map(|(index, image, content_hash)| {
                let (target, _) = assignments.get(&index)?.clone();
                Some((
                    image,
                    target,
                    content_hash,
                    identities[index].clone(),
                    image_paths[index].clone(),
                ))
            })
            .collect::<Vec<_>>();

        let phash_total = prepared_jobs.len();
        reporter.emit(ImageImportStage::PerceptualHashing, 0, phash_total, true);
        let prepared = parallel::parallel_map_cancellable(
            prepared_jobs,
            parallel::worker_count(phash_total),
            cancel::flag(),
            |_, (image, target, content_hash, identity, image_path)| {
                let perceptual_hash = compute_phash(&image.source.absolute_path).ok();
                (
                    image,
                    target,
                    content_hash,
                    perceptual_hash,
                    identity,
                    image_path,
                )
            },
            |completed| {
                reporter.emit(
                    ImageImportStage::PerceptualHashing,
                    completed,
                    phash_total,
                    completed == phash_total,
                );
            },
        )
        .ok_or(ImageImportError::UpdateCancelled)?;

        let mut copy_failures = 0_u64;
        let mut updates = Vec::with_capacity(prepared.len());
        let copy_total = prepared.len();
        reporter.emit(ImageImportStage::Copying, 0, copy_total, true);
        for (copied, (image, target, content_hash, perceptual_hash, identity, image_path)) in
            prepared.into_iter().enumerate()
        {
            // 逐张响应取消：刷新受管副本是就地覆盖旧文件，已刷新的部分与
            // 数据库并无不一致（元数据尚未写库，副本内容与原图一致），
            // 因此取消时直接放弃剩余部分即可。
            if cancel::is_requested() {
                return Err(ImageImportError::UpdateCancelled);
            }
            let stored_image_path = target
                .stored_image_path
                .clone()
                .unwrap_or_else(|| format!("files/relinked/row-{}.png", target.row_id));
            if refresh_stored_copy(self, &image.source.absolute_path, &stored_image_path).is_err() {
                copy_failures += 1;
                reporter.emit(
                    ImageImportStage::Copying,
                    copied + 1,
                    copy_total,
                    copied + 1 == copy_total,
                );
                continue;
            }
            // 缓存删除失败不应阻止元数据更新；下次图片加载仍会按文件签名生成新缓存。
            let _ = remove_row_thumbnail_cache(self, target.row_id);
            updates.push(ExistingImageUpdate {
                row_id: target.row_id,
                identity,
                image_path,
                source_size: i64::try_from(image.source.size).ok(),
                source_mtime: image.source.modified_nanos,
                positive_prompt: image.positive_prompt,
                character_prompt: image.character_prompt,
                negative_prompt: image.negative_prompt,
                artists: image.artists,
                content_hash,
                perceptual_hash,
                metadata_fingerprint: image.metadata_fingerprint,
                stored_image_path: Some(stored_image_path),
                stored_image_is_original: true,
                vibe_reference_count: image.vibe_reference_count,
                vibe_signature: image.vibe_signature,
                image_width: image.image_width,
                image_height: image.image_height,
                generation_model: image.generation_model,
                generation_sampler: image.generation_sampler,
                generation_steps: image.generation_steps,
                generation_seed: image.generation_seed,
                generation_scale: image.generation_scale,
                generation_cfg_rescale: image.generation_cfg_rescale,
                generation_noise_schedule: image.generation_noise_schedule,
            });
            reporter.emit(
                ImageImportStage::Copying,
                copied + 1,
                copy_total,
                copied + 1 == copy_total,
            );
        }
        // 最后一个可取消检查点：从这里开始进入写库事务，不再响应取消。
        if cancel::is_requested() {
            return Err(ImageImportError::UpdateCancelled);
        }
        let updated_row_ids = updates.iter().map(|update| update.row_id).collect::<Vec<_>>();
        let updated = database.update_existing_images(&updates)?;
        let rule_execution = database
            .execute_automation_rules(RuleExecutionTrigger::Update, &updated_row_ids)
            .unwrap_or_else(|error| {
                RuleExecutionSummary::failed(
                    RuleExecutionTrigger::Update,
                    updated_row_ids.len(),
                    error,
                )
            });

        let matched = u64::try_from(assignments.len()).unwrap_or(u64::MAX);
        let ambiguous = u64::try_from(ambiguous_indices.len()).unwrap_or(u64::MAX);
        let unmatched = u64::try_from(
            total_found
                .saturating_sub(assignments.len())
                .saturating_sub(ambiguous_indices.len()),
        )
        .unwrap_or(u64::MAX);

        drop(run_temp);
        Ok(ExistingImageUpdateOutcome {
            source_type,
            total_found,
            matched,
            updated,
            matched_by_identity,
            relinked_by_content: u64::try_from(relinked_by_content).unwrap_or(u64::MAX),
            relinked_by_metadata: u64::try_from(relinked_by_metadata).unwrap_or(u64::MAX),
            ambiguous,
            unmatched,
            metadata_rejected,
            copy_failures,
            rule_execution,
        })
    }
}

fn refresh_stored_copy(
    directory: &DataDirectory,
    source: &Path,
    relative: &str,
) -> Result<(), std::io::Error> {
    let target = directory.root().join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = unique_sibling_path(&target, "image-update");
    if let Err(error) = fs::copy(source, &temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    replace_output_file(&temporary, &target)
}

fn remove_row_thumbnail_cache(
    directory: &DataDirectory,
    row_id: i64,
) -> Result<(), std::io::Error> {
    let prefix = format!("row-{row_id}-");
    for entry in fs::read_dir(directory.thumbnail_cache_path())? {
        let entry = entry?;
        if entry.path().is_file() && entry.file_name().to_string_lossy().starts_with(&prefix) {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

struct ProcessImageContext<'a> {
    source_type: SourceType,
    input_display: &'a str,
    scan_root_display: &'a str,
    staging_root: &'a Path,
}

#[derive(Debug)]
struct ParsedImage {
    source: SourceImage,
    positive_prompt: Option<String>,
    character_prompt: Option<String>,
    negative_prompt: Option<String>,
    artists: Option<String>,
    metadata_fingerprint: Option<String>,
    vibe_reference_count: u32,
    vibe_signature: Option<String>,
    image_width: Option<u32>,
    image_height: Option<u32>,
    generation_model: Option<String>,
    generation_sampler: Option<String>,
    generation_steps: Option<u32>,
    generation_seed: Option<String>,
    generation_scale: Option<f64>,
    generation_cfg_rescale: Option<f64>,
    generation_noise_schedule: Option<String>,
}

enum MetadataInspection {
    Valid(Box<ParsedImage>),
    Rejected(SourceImage),
}

fn inspect_metadata(image: SourceImage) -> MetadataInspection {
    let dimensions = ::image::image_dimensions(&image.absolute_path).ok();
    let Ok(mut chunks) = png_text::read_png_text_chunks(&image.absolute_path) else {
        return MetadataInspection::Rejected(image);
    };
    let mut metadata = parse_novelai_metadata(&chunks);
    if metadata.positive_prompt.trim().is_empty()
        && metadata.character_prompt.trim().is_empty()
        && metadata.negative_prompt.trim().is_empty()
    {
        match stealth_png::read_stealth_png_metadata(&image.absolute_path) {
            Ok(Some(stealth_chunks)) => {
                chunks = stealth_chunks;
                metadata = parse_novelai_metadata(&chunks);
            }
            Ok(None) => {}
            Err(_) => return MetadataInspection::Rejected(image),
        }
    }
    let metadata_fingerprint = metadata_fingerprint(&chunks);
    let positive_prompt = nonempty_string(metadata.positive_prompt);
    let character_prompt = nonempty_string(metadata.character_prompt);
    let negative_prompt = nonempty_string(metadata.negative_prompt);
    if positive_prompt.is_none() && character_prompt.is_none() && negative_prompt.is_none() {
        return MetadataInspection::Rejected(image);
    }
    MetadataInspection::Valid(Box::new(ParsedImage {
        source: image,
        positive_prompt,
        character_prompt,
        negative_prompt,
        artists: nonempty_string(metadata.artist_tags.join("\n")),
        metadata_fingerprint,
        vibe_reference_count: metadata.vibe_reference_count,
        vibe_signature: metadata.vibe_signature,
        image_width: dimensions.map(|(width, _)| width),
        image_height: dimensions.map(|(_, height)| height),
        generation_model: metadata.generation_model,
        generation_sampler: metadata.generation_sampler,
        generation_steps: metadata.generation_steps,
        generation_seed: metadata.generation_seed,
        generation_scale: metadata.generation_scale,
        generation_cfg_rescale: metadata.generation_cfg_rescale,
        generation_noise_schedule: metadata.generation_noise_schedule,
    }))
}

fn build_new_row(
    image: ParsedImage,
    identity: &str,
    scan_index: usize,
    context: &ProcessImageContext<'_>,
    content_hash: Option<String>,
    perceptual_hash: Option<String>,
) -> Result<NewRow, std::io::Error> {
    let (image_path, stored_image_rel) = match context.source_type {
        SourceType::Archive => {
            // 副本移动到暂存目录（同盘瞬间完成，跨盘回退复制），保持包内目录结构。
            let staged = context.staging_root.join(&image.source.relative_path);
            if let Some(parent) = staged.parent() {
                fs::create_dir_all(parent)?;
            }
            if fs::rename(&image.source.absolute_path, &staged).is_err() {
                fs::copy(&image.source.absolute_path, &staged)?;
            }
            (
                format!("{} > {}", context.input_display, image.source.relative_path),
                Some(image.source.relative_path.replace('\\', "/")),
            )
        }
        _ => {
            let staged = context.staging_root.join(&image.source.relative_path);
            if let Some(parent) = staged.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&image.source.absolute_path, &staged)?;
            (
                format!(
                    "{}\\{}",
                    context.scan_root_display, image.source.relative_path
                ),
                Some(image.source.relative_path.replace('\\', "/")),
            )
        }
    };

    Ok(NewRow {
        source_ordinal: u32::try_from(scan_index + 1).unwrap_or(u32::MAX),
        identity: identity.to_owned(),
        source_size: i64::try_from(image.source.size).ok(),
        source_mtime: image.source.modified_nanos,
        content_hash,
        perceptual_hash,
        metadata_fingerprint: image.metadata_fingerprint,
        time: image.source.created.map(format_local_time),
        positive_prompt: image.positive_prompt,
        character_prompt: image.character_prompt,
        negative_prompt: image.negative_prompt,
        note: None,
        artists: image.artists,
        image_folder: None,
        image_path: Some(image_path),
        stored_image_rel,
        metadata_failed: false,
        vibe_reference_count: image.vibe_reference_count,
        vibe_signature: image.vibe_signature,
        image_width: image.image_width,
        image_height: image.image_height,
        generation_model: image.generation_model,
        generation_sampler: image.generation_sampler,
        generation_steps: image.generation_steps,
        generation_seed: image.generation_seed,
        generation_scale: image.generation_scale,
        generation_cfg_rescale: image.generation_cfg_rescale,
        generation_noise_schedule: image.generation_noise_schedule,
    })
}

fn validate_rejected_directory(
    input: &Path,
    source_type: SourceType,
    rejected_root: &Path,
) -> Result<(), ImageImportError> {
    if source_type == SourceType::Archive {
        return Ok(());
    }
    let single_file = input.is_file();
    let input_root = if single_file {
        input.parent().unwrap_or(input)
    } else {
        input
    };
    let input_root = input_root
        .canonicalize()
        .unwrap_or_else(|_| input_root.to_owned());
    let rejected_root = rejected_root
        .canonicalize()
        .unwrap_or_else(|_| rejected_root.to_owned());
    let overlaps_input = if single_file {
        rejected_root == input_root
    } else {
        rejected_root == input_root || rejected_root.starts_with(&input_root)
    };
    if overlaps_input {
        return Err(ImageImportError::RejectedDirectoryInsideInput(
            rejected_root,
        ));
    }
    Ok(())
}

fn move_rejected_image(
    image: &SourceImage,
    rejected_root: &Path,
) -> Result<PathBuf, std::io::Error> {
    let desired = rejected_root.join(&image.relative_path);
    if let Some(parent) = desired.parent() {
        fs::create_dir_all(parent)?;
    }
    if desired.is_file() && is_same_content(&image.absolute_path, &desired)? {
        let _ = fs::remove_file(&image.absolute_path);
        return Ok(desired);
    }
    let destination = unique_destination(&desired);
    if fs::rename(&image.absolute_path, &destination).is_err() {
        fs::copy(&image.absolute_path, &destination)?;
        if let Err(error) = fs::remove_file(&image.absolute_path) {
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
    }
    Ok(destination)
}

fn is_same_content(a: &Path, b: &Path) -> Result<bool, std::io::Error> {
    let meta_a = fs::metadata(a)?;
    let meta_b = fs::metadata(b)?;
    if meta_a.len() != meta_b.len() {
        return Ok(false);
    }
    Ok(fs::read(a)? == fs::read(b)?)
}

fn unique_destination(desired: &Path) -> PathBuf {
    if !desired.exists() {
        return desired.to_owned();
    }
    let parent = desired.parent().unwrap_or_else(|| Path::new(""));
    let stem = desired
        .file_stem()
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let extension = desired.extension().map(|value| value.to_string_lossy());
    for suffix in 2_u64.. {
        let file_name = if let Some(extension) = &extension {
            format!("{stem}_{suffix}.{extension}")
        } else {
            format!("{stem}_{suffix}")
        };
        let candidate = parent.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u64 suffix space is exhaustive")
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

    use flate2::{Compression, write::GzEncoder};

    use super::*;
    use crate::db::{
        AutomationRuleDraft, GenerationNumberField, GenerationTextField, ImageDimensionField,
        NumericComparison, NumericOperator, PromptOperator, PromptScope, RowSelection,
        RuleAction, RuleCondition, RuleConditionGroup, RuleConditionSet, RuleMatchMode,
        TagOperator, TextOperator,
    };
    use crate::storage::test_fixtures::{metadata_png_bytes, write_metadata_png};

    /// 仅含文本元数据的最小 PNG（签名 + tEXt + IEND，CRC 占位即可，
    /// 文本读取器跳过 CRC 校验）。
    fn create_metadata_png(path: &Path, description: &str) {
        create_text_png(path, "Description", description);
    }

    fn create_text_png(path: &Path, keyword: &str, text: &str) {
        let mut data = keyword.as_bytes().to_vec();
        data.push(0);
        data.extend(text.as_bytes());
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        for (chunk_type, payload) in [(b"tEXt", data.as_slice()), (b"IEND", &[][..])] {
            png.extend((payload.len() as u32).to_be_bytes());
            png.extend(chunk_type);
            png.extend(payload);
            png.extend(0_u32.to_be_bytes());
        }
        fs::write(path, png).unwrap();
    }

    fn create_stealth_png(path: &Path, description: &str, comment: &str) {
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

        let mut image = image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 20, 30, 255]));
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
        image.save(path).unwrap();
    }

    #[test]
    fn imports_novelai_metadata_stored_only_in_alpha_channel() {
        let temporary = TemporaryImageImport::new();
        fs::create_dir_all(&temporary.root).unwrap();
        let input = temporary.root.join("stealth-only.png");
        create_stealth_png(
            &input,
            "best quality, artist:stealth",
            r#"{"seed":42,"uc":"bad hands"}"#,
        );
        let directory = temporary.initialize_directory();

        let outcome = directory.import_images(&input, |_| {}).unwrap();

        assert_eq!(outcome.added, 1);
        assert_eq!(outcome.metadata_rejected, 0);
        let row = directory
            .open_database()
            .unwrap()
            .get_rows_by_ids(&[1])
            .unwrap()
            .remove(0);
        assert_eq!(
            row.positive_prompt.as_deref(),
            Some("best quality, artist:stealth")
        );
        assert_eq!(row.negative_prompt.as_deref(), Some("bad hands"));
        assert_eq!(row.artists.as_deref(), Some("artist:stealth"));
    }

    #[test]
    fn imports_folder_then_appends_only_new_images() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("input");
        fs::create_dir_all(input.join("nested")).unwrap();
        create_metadata_png(&input.join("a.png"), "best quality, artist:alpha");
        create_metadata_png(&input.join("nested").join("b.png"), "scenery");
        fs::write(input.join("broken.png"), b"not a png").unwrap();
        let directory = temporary.initialize_directory();

        let outcome = directory.import_images(&input, |_| {}).unwrap();

        assert_eq!(outcome.total_found, 3);
        assert_eq!(outcome.added, 2);
        assert_eq!(outcome.skipped_existing, 0);
        assert_eq!(outcome.metadata_rejected, 1);
        assert_eq!(outcome.rejected_moved, 1);
        assert_eq!(outcome.rejected_move_failures, 0);
        assert_eq!(outcome.source_type, SourceType::Folder);

        let mut database = directory.open_database().unwrap();
        let page = database
            .query_rows(&crate::db::RowQuery {
                offset: 0,
                limit: 10,
                tags: Vec::new(),
                tag_mode: crate::db::TagMatchMode::And,
                dedupe: crate::db::DedupeMode::None,
                single_artist_only: false,
                has_vibe: false,
                untagged_only: false,
                group_view: false,
                hide_grouped: false,
                search: String::new(),
            })
            .unwrap();
        assert_eq!(page.total_count, 2);
        let first = &page.rows[0];
        assert_eq!(
            first.positive_prompt.as_deref(),
            Some("best quality, artist:alpha")
        );
        assert_eq!(first.artists.as_deref(), Some("artist:alpha"));
        assert!(!first.metadata_failed);
        assert!(first.image_path.as_deref().unwrap().ends_with("a.png"));
        assert!(first.time.is_some());
        assert!(!input.join("broken.png").exists());
        assert_eq!(
            fs::read(temporary.rejected.join("broken.png")).unwrap(),
            b"not a png"
        );

        // 追加 2 张新图后重新导入：只新增 2，已有 2 张跳过。
        create_metadata_png(&input.join("c.png"), "new one");
        create_metadata_png(&input.join("d.png"), "new two");
        let second = directory.import_images(&input, |_| {}).unwrap();
        assert_eq!(second.added, 2);
        assert_eq!(second.skipped_existing, 2);
        assert_eq!(
            directory
                .open_database()
                .unwrap()
                .library_summary()
                .unwrap()
                .row_count,
            4
        );
    }

    #[test]
    fn enabled_import_artist_prefix_only_repairs_new_rows_with_library_evidence() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("artist-prefix-input");
        fs::create_dir_all(&input).unwrap();
        create_metadata_png(&input.join("evidence.png"), "artist:xy, masterpiece");
        create_metadata_png(&input.join("bare.png"), "xy, best quality");
        create_metadata_png(&input.join("unknown.png"), "watermark, scenery");
        let directory = temporary.initialize_directory();
        directory
            .open_database()
            .unwrap()
            .set_auto_artist_prefix_on_import(true)
            .unwrap();

        let outcome = directory.import_images(&input, |_| {}).unwrap();

        assert!(outcome.artist_prefix_enabled);
        assert_eq!(outcome.artist_prefix_scanned_rows, 3);
        assert_eq!(outcome.artist_prefix_changed_rows, 1);
        assert_eq!(outcome.artist_prefix_changed_fields, 1);
        assert_eq!(outcome.artist_prefix_error, None);
        let mut database = directory.open_database().unwrap();
        let page = database
            .query_rows(&crate::db::RowQuery {
                offset: 0,
                limit: 10,
                tags: Vec::new(),
                tag_mode: crate::db::TagMatchMode::And,
                dedupe: crate::db::DedupeMode::None,
                single_artist_only: false,
                has_vibe: false,
                untagged_only: false,
                group_view: false,
                hide_grouped: false,
                search: String::new(),
            })
            .unwrap();
        let prompts = page
            .rows
            .iter()
            .map(|row| row.positive_prompt.as_deref().unwrap_or_default())
            .collect::<Vec<_>>();
        assert!(prompts.contains(&"artist:xy, masterpiece"));
        assert!(prompts.contains(&"artist:xy, best quality"));
        assert!(prompts.contains(&"watermark, scenery"));
    }

    #[test]
    fn update_import_changes_only_existing_rows_and_preserves_tags_and_group() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("update-input");
        fs::create_dir_all(&input).unwrap();
        let existing = input.join("existing.png");
        let rejected_update = input.join("failing.png");
        write_metadata_png(&existing, "old prompt, artist:old");
        write_metadata_png(&rejected_update, "keep this old prompt");
        let directory = temporary.initialize_directory();
        directory.import_images(&input, |_| {}).unwrap();

        let mut database = directory.open_database().unwrap();
        database.create_tag("保留标签").unwrap();
        database.set_tags_for_row(1, &["保留标签".into()]).unwrap();
        let group = database.create_group("保留分组").unwrap();
        database
            .assign_rows_to_group(&RowSelection::Explicit { row_ids: vec![1] }, group.id)
            .unwrap();
        let stored_path = database
            .row_image_locator(1)
            .unwrap()
            .stored_image_path
            .unwrap();
        drop(database);

        fs::write(
            &existing,
            metadata_png_bytes(
                "new base prompt, artist:base",
                Some(
                    r#"{"v4_prompt":{"caption":{"char_captions":[{"char_caption":"1girl, artist:character"}]}}}"#,
                ),
            ),
        )
        .unwrap();
        fs::write(&rejected_update, b"not a valid png").unwrap();
        write_metadata_png(&input.join("brand-new.png"), "must not be added");

        let outcome = directory.update_existing_images(&input, |_| {}).unwrap();

        assert_eq!(outcome.total_found, 3);
        assert_eq!(outcome.matched, 2);
        assert_eq!(outcome.updated, 1);
        assert_eq!(outcome.matched_by_identity, 2);
        assert_eq!(outcome.relinked_by_content, 0);
        assert_eq!(outcome.relinked_by_metadata, 0);
        assert_eq!(outcome.ambiguous, 0);
        assert_eq!(outcome.unmatched, 1);
        assert_eq!(outcome.metadata_rejected, 1);
        assert_eq!(outcome.copy_failures, 0);
        let mut database = directory.open_database().unwrap();
        assert_eq!(database.library_summary().unwrap().row_count, 2);
        let rows = database.get_rows_by_ids(&[1, 2]).unwrap();
        assert_eq!(
            rows[0].positive_prompt.as_deref(),
            Some("new base prompt, artist:base")
        );
        assert_eq!(
            rows[0].character_prompt.as_deref(),
            Some("1girl, artist:character")
        );
        assert_eq!(
            rows[0].artists.as_deref(),
            Some("artist:base\nartist:character")
        );
        assert_eq!(rows[0].tags, vec!["保留标签"]);
        assert_eq!(rows[0].group_id, Some(group.id));
        assert_eq!(rows[0].group_name.as_deref(), Some("保留分组"));
        assert_eq!(
            rows[1].positive_prompt.as_deref(),
            Some("keep this old prompt")
        );
        assert_eq!(
            fs::read(directory.root().join(stored_path)).unwrap(),
            fs::read(existing).unwrap()
        );
    }

    #[test]
    fn update_import_relinks_moved_original_by_content_hash() {
        let temporary = TemporaryImageImport::new();
        let original_dir = temporary.root.join("original");
        let moved_dir = temporary.root.join("moved");
        fs::create_dir_all(&original_dir).unwrap();
        fs::create_dir_all(&moved_dir).unwrap();
        let original = original_dir.join("same.png");
        let moved = moved_dir.join("renamed.png");
        fs::write(
            &original,
            metadata_png_bytes("artist:moved", Some(r#"{"seed":123,"steps":28}"#)),
        )
        .unwrap();
        let directory = temporary.initialize_directory();
        directory.import_images(&original_dir, |_| {}).unwrap();
        fs::rename(&original, &moved).unwrap();

        let outcome = directory
            .update_existing_images(&moved_dir, |_| {})
            .unwrap();

        assert_eq!(outcome.matched, 1);
        assert_eq!(outcome.updated, 1);
        assert_eq!(outcome.matched_by_identity, 0);
        assert_eq!(outcome.relinked_by_content, 1);
        assert_eq!(outcome.relinked_by_metadata, 0);
        assert_eq!(outcome.ambiguous, 0);
        assert_eq!(outcome.unmatched, 0);
        let locator = directory
            .open_database()
            .unwrap()
            .row_image_locator(1)
            .unwrap();
        assert_eq!(
            locator.image_path.as_deref(),
            Some(moved.to_string_lossy().as_ref())
        );
        assert!(locator.stored_image_is_original);
        assert_eq!(
            fs::read(directory.root().join(locator.stored_image_path.unwrap())).unwrap(),
            fs::read(moved).unwrap()
        );
    }

    #[test]
    fn update_import_relinks_reencoded_original_by_complete_metadata() {
        let temporary = TemporaryImageImport::new();
        let original_dir = temporary.root.join("metadata-original");
        let moved_dir = temporary.root.join("metadata-moved");
        fs::create_dir_all(&original_dir).unwrap();
        fs::create_dir_all(&moved_dir).unwrap();
        let original = original_dir.join("old.png");
        let moved = moved_dir.join("new.png");
        let bytes = metadata_png_bytes(
            "artist:metadata",
            Some(r#"{"seed":987654,"steps":28,"sampler":"k_euler"}"#),
        );
        fs::write(&original, &bytes).unwrap();
        let directory = temporary.initialize_directory();
        directory.import_images(&original_dir, |_| {}).unwrap();

        let mut reencoded = bytes;
        reencoded.extend_from_slice(b"harmless trailing bytes");
        fs::write(&moved, reencoded).unwrap();
        fs::remove_file(&original).unwrap();

        let outcome = directory
            .update_existing_images(&moved_dir, |_| {})
            .unwrap();

        assert_eq!(outcome.matched, 1);
        assert_eq!(outcome.updated, 1);
        assert_eq!(outcome.relinked_by_content, 0);
        assert_eq!(outcome.relinked_by_metadata, 1);
        assert_eq!(outcome.ambiguous, 0);
        assert_eq!(outcome.unmatched, 0);
        assert_eq!(
            directory
                .open_database()
                .unwrap()
                .row_image_locator(1)
                .unwrap()
                .image_path
                .as_deref(),
            Some(moved.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn update_import_does_not_choose_between_duplicate_metadata_candidates() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("ambiguous");
        fs::create_dir_all(&input).unwrap();
        let candidate = input.join("candidate.png");
        fs::write(
            &candidate,
            metadata_png_bytes("artist:ambiguous", Some(r#"{"seed":42}"#)),
        )
        .unwrap();
        let chunks = png_text::read_png_text_chunks(&candidate).unwrap();
        let fingerprint = metadata_fingerprint(&chunks).unwrap();
        let directory = temporary.initialize_directory();
        directory
            .open_database()
            .unwrap()
            .append_batch(
                SourceType::Folder,
                r"D:\old",
                &[
                    NewRow {
                        source_ordinal: 1,
                        identity: r"file:d:\old\a.png".into(),
                        content_hash: Some("old-content-a".into()),
                        metadata_fingerprint: Some(fingerprint.clone()),
                        image_path: Some(r"D:\old\a.png".into()),
                        ..NewRow::default()
                    },
                    NewRow {
                        source_ordinal: 2,
                        identity: r"file:d:\old\b.png".into(),
                        content_hash: Some("old-content-b".into()),
                        metadata_fingerprint: Some(fingerprint),
                        image_path: Some(r"D:\old\b.png".into()),
                        ..NewRow::default()
                    },
                ],
                |_| Ok(()),
            )
            .unwrap();

        let outcome = directory.update_existing_images(&input, |_| {}).unwrap();

        assert_eq!(outcome.matched, 0);
        assert_eq!(outcome.updated, 0);
        assert_eq!(outcome.ambiguous, 1);
        assert_eq!(outcome.unmatched, 0);
        assert_eq!(
            directory
                .open_database()
                .unwrap()
                .library_summary()
                .unwrap()
                .row_count,
            2
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
            writer.start_file("套图/图片副本.png", options).unwrap();
            writer.write_all(&fs::read(&png_path).unwrap()).unwrap();
            writer.finish().unwrap();
        }
        let directory = temporary.initialize_directory();

        let outcome = directory.import_images(&archive_path, |_| {}).unwrap();

        assert_eq!(outcome.added, 1);
        assert_eq!(outcome.skipped_content, 1);
        assert_eq!(outcome.source_type, SourceType::Archive);
        let database = directory.open_database().unwrap();
        let locator = database.row_image_locator(1).unwrap();
        let stored = locator.stored_image_path.unwrap();
        assert!(stored.starts_with(&format!("files/{}/", outcome.batch_id)));
        assert_eq!(
            fs::read(directory.root().join(&stored)).unwrap(),
            fs::read(&png_path).unwrap()
        );
        assert_eq!(
            walkdir::WalkDir::new(directory.files_path().join(outcome.batch_id.to_string()))
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .count(),
            1
        );
        assert!(locator.image_path.unwrap().contains(" > "));

        // 同一压缩包重复导入：全部跳过，不留新批次目录。
        let repeat = directory.import_images(&archive_path, |_| {}).unwrap();
        assert_eq!(repeat.added, 0);
        assert_eq!(repeat.skipped_existing, 1);
        assert_eq!(repeat.skipped_content, 1);
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
    fn update_import_refreshes_existing_archive_member_without_adding_new_member() {
        let temporary = TemporaryImageImport::new();
        fs::create_dir_all(&temporary.root).unwrap();
        let existing_png = temporary.root.join("archive-existing.png");
        let new_png = temporary.root.join("archive-new.png");
        let archive_path = temporary.root.join("update-pack.zip");
        write_metadata_png(&existing_png, "archive old prompt");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file(
                    "nested/existing.png",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(&fs::read(&existing_png).unwrap()).unwrap();
            writer.finish().unwrap();
        }
        let directory = temporary.initialize_directory();
        directory.import_images(&archive_path, |_| {}).unwrap();
        let stored_path = directory
            .open_database()
            .unwrap()
            .row_image_locator(1)
            .unwrap()
            .stored_image_path
            .unwrap();

        write_metadata_png(&existing_png, "archive new prompt");
        write_metadata_png(&new_png, "archive member must not be added");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("nested/existing.png", options).unwrap();
            writer.write_all(&fs::read(&existing_png).unwrap()).unwrap();
            writer.start_file("nested/new.png", options).unwrap();
            writer.write_all(&fs::read(&new_png).unwrap()).unwrap();
            writer.finish().unwrap();
        }

        let outcome = directory
            .update_existing_images(&archive_path, |_| {})
            .unwrap();

        assert_eq!(outcome.source_type, SourceType::Archive);
        assert_eq!(outcome.total_found, 2);
        assert_eq!(outcome.matched, 1);
        assert_eq!(outcome.updated, 1);
        assert_eq!(outcome.unmatched, 1);
        let mut database = directory.open_database().unwrap();
        assert_eq!(database.library_summary().unwrap().row_count, 1);
        assert_eq!(
            database.get_rows_by_ids(&[1]).unwrap()[0]
                .positive_prompt
                .as_deref(),
            Some("archive new prompt")
        );
        assert_eq!(
            fs::read(directory.root().join(stored_path)).unwrap(),
            fs::read(existing_png).unwrap()
        );
    }

    #[test]
    fn emits_progress_and_rejects_empty_input() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("input");
        fs::create_dir_all(&input).unwrap();
        create_metadata_png(&input.join("only.png"), "solo");
        let directory = temporary.initialize_directory();

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
            event.stage == ImageImportStage::Hashing && event.processed == event.total
        }));
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
            create_metadata_png(
                &input.join(format!("new-{index}.png")),
                &format!("appended {index}"),
            );
        }
        let directory = temporary.initialize_directory();
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

        let summary = directory
            .open_database()
            .unwrap()
            .library_summary()
            .unwrap();
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
    fn moves_rejected_archive_member_and_keeps_valid_member_managed() {
        let temporary = TemporaryImageImport::new();
        fs::create_dir_all(&temporary.root).unwrap();
        let valid_png = temporary.root.join("valid.png");
        create_metadata_png(&valid_png, "archive prompt");
        let archive_path = temporary.root.join("mixed.zip");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("set/valid.png", options).unwrap();
            writer.write_all(&fs::read(&valid_png).unwrap()).unwrap();
            writer.start_file("set/broken.png", options).unwrap();
            writer.write_all(b"not a png").unwrap();
            writer.finish().unwrap();
        }
        let directory = temporary.initialize_directory();

        let outcome = directory.import_images(&archive_path, |_| {}).unwrap();

        assert_eq!(outcome.added, 1);
        assert_eq!(outcome.metadata_rejected, 1);
        assert_eq!(outcome.rejected_moved, 1);
        assert_eq!(outcome.rejected_move_failures, 0);
        assert_eq!(
            fs::read(temporary.rejected.join("set").join("broken.png")).unwrap(),
            b"not a png"
        );
        let stored = directory
            .open_database()
            .unwrap()
            .row_image_locator(1)
            .unwrap()
            .stored_image_path
            .unwrap();
        assert!(directory.root().join(stored).is_file());
    }

    #[test]
    fn rejects_empty_metadata_moves_without_overwriting_and_keeps_rows_out_of_database() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("input");
        fs::create_dir_all(input.join("nested")).unwrap();
        create_metadata_png(&input.join("empty.png"), "   ");
        create_text_png(
            &input.join("nested").join("comment.png"),
            "Comment",
            r#"{"seed": 1}"#,
        );
        let directory = temporary.initialize_directory();
        fs::write(temporary.rejected.join("empty.png"), b"keep existing").unwrap();

        let outcome = directory.import_images(&input, |_| {}).unwrap();

        assert_eq!(outcome.total_found, 2);
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.metadata_rejected, 2);
        assert_eq!(outcome.rejected_moved, 2);
        assert_eq!(outcome.rejected_move_failures, 0);
        assert_eq!(
            directory
                .open_database()
                .unwrap()
                .library_summary()
                .unwrap()
                .row_count,
            0
        );
        assert_eq!(
            fs::read(temporary.rejected.join("empty.png")).unwrap(),
            b"keep existing"
        );
        assert!(temporary.rejected.join("empty_2.png").is_file());
        assert!(
            temporary
                .rejected
                .join("nested")
                .join("comment.png")
                .is_file()
        );
    }

    #[test]
    fn reports_rejected_move_failure_without_importing_the_image() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("input");
        fs::create_dir_all(input.join("nested")).unwrap();
        fs::write(input.join("nested").join("broken.png"), b"not a png").unwrap();
        let directory = temporary.initialize_directory();
        fs::write(
            temporary.rejected.join("nested"),
            b"blocks directory creation",
        )
        .unwrap();

        let outcome = directory.import_images(&input, |_| {}).unwrap();

        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.metadata_rejected, 1);
        assert_eq!(outcome.rejected_moved, 0);
        assert_eq!(outcome.rejected_move_failures, 1);
        assert!(input.join("nested").join("broken.png").is_file());
        assert_eq!(
            directory
                .open_database()
                .unwrap()
                .library_summary()
                .unwrap()
                .row_count,
            0
        );
    }

    #[test]
    fn rejects_output_directory_inside_import_folder() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("input");
        fs::create_dir_all(&input).unwrap();
        create_metadata_png(&input.join("valid.png"), "valid prompt");
        let directory = temporary.initialize_directory();
        let inside = input.join("rejected");
        directory.set_rejected_images_directory(&inside).unwrap();

        assert!(matches!(
            directory.import_images(&input, |_| {}),
            Err(ImageImportError::RejectedDirectoryInsideInput(path)) if path == inside.canonicalize().unwrap()
        ));
    }

    #[test]
    fn single_png_can_move_to_sibling_output_directory() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("input");
        fs::create_dir_all(&input).unwrap();
        let broken = input.join("broken.png");
        fs::write(&broken, b"not a png").unwrap();
        let directory = temporary.initialize_directory();
        let sibling_output = input.join("rejected");
        directory
            .set_rejected_images_directory(&sibling_output)
            .unwrap();

        let outcome = directory.import_images(&broken, |_| {}).unwrap();

        assert_eq!(outcome.metadata_rejected, 1);
        assert_eq!(outcome.rejected_moved, 1);
        assert_eq!(outcome.added, 0);
        assert!(!broken.exists());
        assert!(sibling_output.join("broken.png").is_file());
    }

    #[test]
    fn imports_only_one_of_five_hundred_identical_files_and_dedupes_across_folders() {
        let temporary = TemporaryImageImport::new();
        let first = temporary.root.join("first");
        let second = temporary.root.join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let template = first.join("image-000.png");
        create_metadata_png(&template, "identical content");
        let bytes = fs::read(&template).unwrap();
        for index in 1..500 {
            fs::write(first.join(format!("image-{index:03}.png")), &bytes).unwrap();
        }
        fs::write(second.join("copy.png"), &bytes).unwrap();
        let directory = temporary.initialize_directory();

        let first_outcome = directory.import_images(&first, |_| {}).unwrap();
        assert_eq!(first_outcome.total_found, 500);
        assert_eq!(first_outcome.added, 1);
        assert_eq!(first_outcome.skipped_existing, 0);
        assert_eq!(first_outcome.skipped_content, 499);

        let second_outcome = directory.import_images(&second, |_| {}).unwrap();
        assert_eq!(second_outcome.added, 0);
        assert_eq!(second_outcome.skipped_existing, 0);
        assert_eq!(second_outcome.skipped_content, 1);
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
        let directory = temporary.initialize_directory();
        directory.import_images(&archive_path, |_| {}).unwrap();

        let report = directory
            .delete_rows(&RowSelection::Explicit { row_ids: vec![1] }, false)
            .unwrap();
        assert_eq!(report.deleted_rows, 1);
        assert_eq!(report.removed_files, 1);

        let again = directory.import_images(&archive_path, |_| {}).unwrap();
        assert_eq!(again.added, 1);
    }

    #[test]
    fn user_authored_example_rule_runs_only_for_new_matching_imports() {
        let temporary = TemporaryImageImport::new();
        let input = temporary.root.join("rule-import");
        fs::create_dir_all(&input).unwrap();
        let fixed_form = "girl, white long hair, blue eyes, colored inner hair(blue), hair flower(white flower)";
        fs::write(
            input.join("matching.png"),
            metadata_png_bytes(
                fixed_form,
                Some(r#"{"model":"nai-diffusion-4","steps":28,"sampler":"k_euler","seed":123}"#),
            ),
        )
        .unwrap();
        fs::write(
            input.join("other.png"),
            metadata_png_bytes(
                "boy, black hair, brown eyes",
                Some(r#"{"model":"nai-diffusion-4","steps":28}"#),
            ),
        )
        .unwrap();
        let directory = temporary.initialize_directory();
        let mut database = directory.open_database().unwrap();
        assert!(database.list_automation_rules().unwrap().is_empty());
        database
            .create_automation_rule(&AutomationRuleDraft {
                name: "测试：识别固定形态".into(),
                description: "测试数据，不是内置规则".into(),
                enabled: true,
                run_on_import: true,
                run_on_update: false,
                conditions: RuleConditionSet {
                    mode: RuleMatchMode::Any,
                    negate: false,
                    groups: vec![RuleConditionGroup {
                        mode: RuleMatchMode::All,
                        conditions: vec![
                            RuleCondition::Prompt {
                                scope: PromptScope::PositiveAndCharacter,
                                operator: PromptOperator::ContainsAll,
                                value: fixed_form.into(),
                                case_sensitive: false,
                            },
                            RuleCondition::ImageDimension {
                                field: ImageDimensionField::Width,
                                comparison: NumericComparison {
                                    operator: NumericOperator::Equal,
                                    value: 16.0,
                                    second_value: None,
                                },
                            },
                            RuleCondition::GenerationText {
                                field: GenerationTextField::Model,
                                operator: TextOperator::Equals,
                                value: "nai-diffusion-4".into(),
                                case_sensitive: false,
                            },
                            RuleCondition::GenerationNumber {
                                field: GenerationNumberField::Steps,
                                comparison: NumericComparison {
                                    operator: NumericOperator::Equal,
                                    value: 28.0,
                                    second_value: None,
                                },
                            },
                            RuleCondition::Tag {
                                operator: TagOperator::HasNone,
                                tags: vec!["花绘".into()],
                            },
                        ],
                    }],
                },
                actions: vec![RuleAction::AddTags {
                    tags: vec!["花绘".into()],
                }],
            })
            .unwrap();
        drop(database);

        let outcome = directory.import_images(&input, |_| {}).unwrap();
        assert_eq!(outcome.added, 2);
        assert_eq!(outcome.rule_execution.input_rows, 2);
        assert_eq!(outcome.rule_execution.changed_rows, 1);
        assert_eq!(outcome.rule_execution.reports[0].matched_rows, 1);

        let mut database = directory.open_database().unwrap();
        let row_ids = database.row_ids_for_batch(outcome.batch_id).unwrap();
        let selected = database
            .list_selection_tags(&RowSelection::Explicit { row_ids })
            .unwrap();
        let example = selected.iter().find(|tag| tag.name == "花绘").unwrap();
        assert_eq!(example.selected_rows, 1);
        drop(database);

        let duplicate = directory.import_images(&input, |_| {}).unwrap();
        assert_eq!(duplicate.added, 0);
        assert_eq!(duplicate.rule_execution.input_rows, 0);
        assert_eq!(duplicate.rule_execution.changed_rows, 0);
    }

    struct TemporaryImageImport {
        root: PathBuf,
        data: PathBuf,
        rejected: PathBuf,
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
                rejected: root.join("rejected"),
                root,
            }
        }

        fn initialize_directory(&self) -> DataDirectory {
            let directory = DataDirectory::initialize(&self.data).unwrap();
            directory
                .set_rejected_images_directory(&self.rejected)
                .unwrap();
            directory
        }
    }

    impl Drop for TemporaryImageImport {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
