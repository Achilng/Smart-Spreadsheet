use super::files::refresh_stored_copy;
use super::metadata::{MetadataInspection, inspect_metadata};
use super::progress::ProgressReporter;
use super::source::{prepare_source, source_identity_root};
use super::types::{
    ExistingImageMatchKind, ExistingImageUpdateOutcome, ImageImportError, ImageImportProgress,
    ImageImportStage,
};
use crate::db::identity::{archive_member_identity, file_identity};
use crate::db::{ExistingImageUpdate, RuleExecutionSummary, RuleExecutionTrigger, SourceType};
use crate::pipeline::scan::collect_png_files;
use crate::pipeline::{cancel, parallel};
use crate::storage::content_hash::sha256_file;
use crate::storage::perceptual_hash::compute_phash;
use crate::storage::{DataDirectory, canonical_display_path};
use std::collections::{HashMap, HashSet};
use std::path::Path;

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
        let (scan_root, source_type, run_temp) = prepare_source(input, &reporter)?;
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

        let scan_root_display = source_identity_root(input, &input_display, source_type);
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
            // 缓存键包含文件元数据签名，替换受管副本后旧缓存自动失效，
            // 无需为每一行扫描整个缓存目录；旧文件由统一容量淘汰回收。
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
        let updated_row_ids = updates
            .iter()
            .map(|update| update.row_id)
            .collect::<Vec<_>>();
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
