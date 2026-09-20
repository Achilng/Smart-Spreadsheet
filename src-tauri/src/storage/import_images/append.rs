use super::files::{move_rejected_image, validate_rejected_directory};
use super::metadata::{
    MetadataInspection, ParsedImage, ProcessImageContext, build_new_row, inspect_metadata,
};
use super::progress::ProgressReporter;
use super::source::{prepare_source, source_identity_root};
use super::types::{ImageImportError, ImageImportOutcome, ImageImportProgress, ImageImportStage};
use crate::db::identity::{archive_member_identity, file_identity};
use crate::db::{DatabaseError, NewRow, RuleExecutionSummary, RuleExecutionTrigger, SourceType};
use crate::pipeline::scan::{SourceImage, collect_png_files};
use crate::pipeline::{cancel, parallel};
use crate::storage::content_hash::sha256_file;
use crate::storage::perceptual_hash::compute_phash;
use crate::storage::{DataDirectory, StagingDir, StorageError, canonical_display_path};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

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
        let (scan_root, source_type, run_temp) = prepare_source(input, &reporter)?;
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
        let scan_root_display = source_identity_root(input, &input_display, source_type);
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
