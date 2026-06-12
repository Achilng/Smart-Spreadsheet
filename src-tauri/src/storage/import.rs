use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::content_hash::sha256_file;
use super::{DataDirectory, StagingDir, StorageError, canonical_display_path, media_extension};
use crate::db::identity::{file_identity, xlsx_row_identity};
use crate::db::{DatabaseError, NewRow, SourceType};
use crate::excel::{
    ImageMapError, ImportError, extract_embedded_images, map_embedded_images,
    read_fixed_workbook,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    pub batch_id: i64,
    pub sheet_name: String,
    pub added: u64,
    pub skipped_existing: u64,
    pub skipped_content: u64,
    pub changed_existing: u64,
    /// 成功提取到受管目录的嵌入图数量（仅统计本次新增行）。
    pub embedded_images_stored: usize,
}

#[derive(Debug, Error)]
pub enum WorkbookImportError {
    #[error("导入文件必须是 .xlsx: {0}")]
    InvalidExtension(PathBuf),
    #[error("导入文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("应用数据目录不可用: {0}")]
    Storage(#[from] StorageError),
    #[error("Excel 解析失败: {0}")]
    Excel(#[from] ImportError),
    #[error("嵌入图片解析失败: {0}")]
    Images(#[from] ImageMapError),
    #[error("数据库写入失败: {0}")]
    Database(#[from] DatabaseError),
}

impl DataDirectory {
    /// 追加导入一份固定结构 xlsx：身份键已存在的行跳过，新行的嵌入图提取到
    /// `files/<批次ID>/embedded/`。源文件全程只读，导入后不再是运行依赖。
    pub fn import_workbook(
        &self,
        source: impl AsRef<Path>,
    ) -> Result<ImportOutcome, WorkbookImportError> {
        let source = source.as_ref();
        let is_xlsx = source
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xlsx"));
        if !is_xlsx {
            return Err(WorkbookImportError::InvalidExtension(source.to_owned()));
        }

        let parsed = read_fixed_workbook(source)?;
        let embedded = map_embedded_images(source, &parsed.sheet_name)?;
        let embedded_by_row: HashMap<u32, &crate::excel::EmbeddedImageRef> = embedded
            .iter()
            .filter(|image| image.source_column == 1)
            .map(|image| (image.source_row, image))
            .collect();

        let display_path = canonical_display_path(source);
        let file_name = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "imported.xlsx".to_owned());

        // 身份键：图片路径列非空且在本批次内唯一 → file:；否则退化为 xlsxrow:。
        let mut path_key_counts: HashMap<String, u32> = HashMap::new();
        for row in &parsed.rows {
            if let Some(path) = nonempty(&row.image_path) {
                *path_key_counts.entry(file_identity(path)).or_default() += 1;
            }
        }
        let identities: Vec<String> = parsed
            .rows
            .iter()
            .map(|row| {
                nonempty(&row.image_path)
                    .map(file_identity)
                    .filter(|key| path_key_counts.get(key) == Some(&1))
                    .unwrap_or_else(|| xlsx_row_identity(&file_name, row.source_row))
            })
            .collect();

        let mut database = self.open_database()?;
        let existing = database.existing_identities(&identities)?;

        // 只为将要新增的行提取嵌入图，先落到暂存目录，入库时随批次 ID 归位。
        let staging = StagingDir::create(&self.files_path())?;
        let mut stored_rels: HashMap<u32, String> = HashMap::new();
        let mut extraction_targets = Vec::new();
        for (row, identity) in parsed.rows.iter().zip(&identities) {
            if existing.contains(identity) {
                continue;
            }
            if let Some(image) = embedded_by_row.get(&row.source_row) {
                extraction_targets.push((**image).clone());
            }
        }
        if !extraction_targets.is_empty() {
            let embedded_dir = staging.path().join("embedded");
            fs::create_dir_all(&embedded_dir)?;
            extract_embedded_images(source, &extraction_targets, |_, image, bytes| {
                let extension = media_extension(&image.media_path);
                let file_name = format!("row-{}.{extension}", image.source_row);
                fs::write(embedded_dir.join(&file_name), bytes)?;
                stored_rels.insert(image.source_row, format!("embedded/{file_name}"));
                Ok(())
            })?;
        }
        let content_hashes: HashMap<u32, String> = parsed
            .rows
            .iter()
            .zip(&identities)
            .filter(|(_, identity)| !existing.contains(*identity))
            .filter_map(|(row, _)| {
                content_hash_for_workbook_row(
                    row.image_path.as_deref(),
                    stored_rels.get(&row.source_row).map(String::as_str),
                    staging.path(),
                )
                .map(|hash| (row.source_row, hash))
            })
            .collect();
        let candidate_hashes = content_hashes.values().cloned().collect::<Vec<_>>();
        let mut seen_content: HashSet<String> =
            database.existing_content_hashes(&candidate_hashes)?;
        let mut embedded_images_stored = 0;
        let mut new_rows = Vec::with_capacity(parsed.rows.len());
        for (row, identity) in parsed.rows.iter().zip(identities) {
            let content_hash = content_hashes.get(&row.source_row).cloned();
            let duplicate_content = !existing.contains(&identity)
                && content_hash
                    .as_ref()
                    .is_some_and(|hash| !seen_content.insert(hash.clone()));
            let mut stored_image_rel = stored_rels.get(&row.source_row).cloned();
            if duplicate_content {
                if let Some(relative) = stored_image_rel.take() {
                    let _ = fs::remove_file(staging.path().join(relative));
                }
            } else if stored_image_rel.is_some() {
                embedded_images_stored += 1;
            }
            new_rows.push(NewRow {
                source_ordinal: row.source_row,
                identity,
                source_size: None,
                source_mtime: None,
                content_hash,
                time: row.time.clone(),
                positive_prompt: row.positive_prompt.clone(),
                negative_prompt: row.negative_prompt.clone(),
                artists: row.artists.clone(),
                image_folder: row.image_folder.clone(),
                image_path: row.image_path.clone(),
                stored_image_rel,
                metadata_failed: false,
            });
        }

        let files_root = self.files_path();
        let has_staged_files = embedded_images_stored > 0;
        let outcome = database.append_batch(
            SourceType::Xlsx,
            &display_path,
            &new_rows,
            |batch_id| {
                if !has_staged_files {
                    return Ok(());
                }
                let target = files_root.join(batch_id.to_string());
                fs::rename(staging.path(), &target)
                    .map_err(|error| format!("无法落位批次文件目录 {}: {error}", target.display()))
            },
        )?;

        Ok(ImportOutcome {
            batch_id: outcome.batch_id,
            sheet_name: parsed.sheet_name,
            added: outcome.added,
            skipped_existing: outcome.skipped_existing,
            skipped_content: outcome.skipped_content,
            changed_existing: outcome.changed_existing,
            embedded_images_stored,
        })
    }
}

fn content_hash_for_workbook_row(
    image_path: Option<&str>,
    stored_image_rel: Option<&str>,
    staging_root: &Path,
) -> Option<String> {
    if let Some(path) = image_path.map(str::trim).filter(|path| !path.is_empty())
        && let Ok(hash) = sha256_file(Path::new(path))
    {
        return Some(hash);
    }
    stored_image_rel.and_then(|relative| sha256_file(&staging_root.join(relative)).ok())
}

fn nonempty(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn workbook_hash_prefers_external_path_and_falls_back_to_extracted_image() {
        let temporary = TemporaryImport::new();
        fs::create_dir_all(&temporary.root).unwrap();
        let external = temporary.root.join("external.png");
        let stored = temporary.root.join("embedded.png");
        fs::write(&external, b"external").unwrap();
        fs::write(&stored, b"embedded").unwrap();

        assert_eq!(
            content_hash_for_workbook_row(
                Some(external.to_string_lossy().as_ref()),
                Some("embedded.png"),
                &temporary.root,
            ),
            Some(sha256_file(&external).unwrap())
        );
        assert_eq!(
            content_hash_for_workbook_row(
                Some(temporary.root.join("missing.png").to_string_lossy().as_ref()),
                Some("embedded.png"),
                &temporary.root,
            ),
            Some(sha256_file(&stored).unwrap())
        );
    }

    #[test]
    fn imports_sample_as_appended_batch_with_extracted_images() {
        let temporary = TemporaryImport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let source = sample_workbook();
        let source_before = fs::read(&source).unwrap();

        let outcome = directory.import_workbook(&source).unwrap();

        assert_eq!(outcome.sheet_name, "NovelAI Metadata");
        assert_eq!(outcome.added, 5);
        assert_eq!(outcome.skipped_existing, 0);
        assert_eq!(outcome.embedded_images_stored, 5);
        assert_eq!(fs::read(&source).unwrap(), source_before);

        let database = directory.open_database().unwrap();
        let summary = database.library_summary().unwrap();
        assert_eq!(summary.row_count, 5);
        assert_eq!(summary.batch_count, 1);

        // 嵌入图副本已经落位到批次目录，行记录引用该副本。
        let locator = database.row_image_locator(1).unwrap();
        let stored = locator.stored_image_path.unwrap();
        assert!(stored.starts_with(&format!("files/{}/embedded/", outcome.batch_id)));
        assert!(directory.root().join(&stored).is_file());
    }

    #[test]
    fn reimporting_same_workbook_skips_all_rows() {
        let temporary = TemporaryImport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        directory.import_workbook(sample_workbook()).unwrap();

        let second = directory.import_workbook(sample_workbook()).unwrap();

        assert_eq!(second.added, 0);
        assert_eq!(second.skipped_existing, 5);
        assert_eq!(second.embedded_images_stored, 0);
        let database = directory.open_database().unwrap();
        assert_eq!(database.library_summary().unwrap().row_count, 5);
        assert_eq!(database.library_summary().unwrap().batch_count, 2);
        // 跳过的行没有留下暂存目录或新的批次文件目录。
        let staged = fs::read_dir(directory.files_path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".staging-")
            })
            .count();
        assert_eq!(staged, 0);
    }

    #[test]
    fn skips_workbook_image_already_imported_from_another_path() {
        let temporary = TemporaryImport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let source = sample_workbook();
        let parsed = read_fixed_workbook(&source).unwrap();
        let original_image = PathBuf::from(
            parsed.rows[0]
                .image_path
                .as_deref()
                .expect("sample row should have an external image path"),
        );
        assert!(original_image.is_file());
        let copied_folder = temporary.root.join("copied");
        fs::create_dir_all(&copied_folder).unwrap();
        fs::copy(&original_image, copied_folder.join("copy.png")).unwrap();
        let image_outcome = directory.import_images(&copied_folder, |_| {}).unwrap();
        assert_eq!(image_outcome.added, 1);

        let workbook_outcome = directory.import_workbook(&source).unwrap();

        assert_eq!(workbook_outcome.added, 4);
        assert_eq!(workbook_outcome.skipped_existing, 0);
        assert_eq!(workbook_outcome.skipped_content, 1);
        assert_eq!(workbook_outcome.embedded_images_stored, 4);
        assert_eq!(
            directory.open_database().unwrap().library_summary().unwrap().row_count,
            5
        );
    }

    #[test]
    fn malformed_workbook_keeps_existing_library() {
        let temporary = TemporaryImport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        directory.import_workbook(sample_workbook()).unwrap();
        let invalid = temporary.root.join("invalid.xlsx");
        fs::write(&invalid, b"not an xlsx file").unwrap();

        assert!(directory.import_workbook(&invalid).is_err());

        let database = directory.open_database().unwrap();
        let summary = database.library_summary().unwrap();
        assert_eq!(summary.row_count, 5);
        assert_eq!(summary.batch_count, 1);
    }

    #[test]
    fn appended_rows_keep_existing_tags_intact() {
        let temporary = TemporaryImport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        directory.import_workbook(sample_workbook()).unwrap();
        {
            let mut database = directory.open_database().unwrap();
            database
                .add_tags_to_rows(&[1], &["既有".into()])
                .unwrap();
        }

        directory.import_workbook(sample_workbook()).unwrap();

        let database = directory.open_database().unwrap();
        let tags = database.list_tags().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "既有");
        assert_eq!(tags[0].row_count, 1);
    }

    fn sample_workbook() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("Examples")
            .join("novelai_metadata.xlsx")
    }

    struct TemporaryImport {
        root: PathBuf,
        data: PathBuf,
    }

    impl TemporaryImport {
        fn new() -> Self {
            let local_agent_temp = Path::new(r"D:\Agent\Agent_temp");
            let parent = if local_agent_temp.is_dir() {
                local_agent_temp.to_owned()
            } else {
                std::env::temp_dir()
            };
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            let root = parent.join(format!(
                "smart-spreadsheet-import-{}-{nonce}",
                std::process::id()
            ));
            Self {
                data: root.join("data"),
                root,
            }
        }
    }

    impl Drop for TemporaryImport {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
