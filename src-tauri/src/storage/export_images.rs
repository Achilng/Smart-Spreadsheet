use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::{DataDirectory, StorageError};
use crate::db::{ExportRow, RowSelection, TagMutationError};

const PROGRESS_EVERY_FILES: usize = 20;

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
    #[error("应用数据目录不可用: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Selection(#[from] TagMutationError),
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

fn output_file_name(ordinal: usize, source: &Path) -> String {
    let original = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image.png".to_owned());
    let sanitized: String = original
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();
    format!("{ordinal:05}_{sanitized}")
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
