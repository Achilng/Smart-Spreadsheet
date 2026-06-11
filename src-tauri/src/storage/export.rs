use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::{DataDirectory, StorageError};
use crate::db::{DatabaseError, SourceType};
use crate::excel::{ExportError, ExportRowTags, export_with_tags, read_fixed_workbook};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOutcome {
    pub destination: PathBuf,
    pub row_count: usize,
}

#[derive(Debug, Error)]
pub enum WorkbookExportError {
    #[error("导出文件必须使用 .xlsx 扩展名: {0}")]
    InvalidExtension(PathBuf),
    #[error("资料库为空，没有可导出的数据")]
    EmptyLibrary,
    #[error(
        "旧版导出仅支持迁移自单一 xlsx 的资料库；追加导入后的全新导出功能将在后续版本提供"
    )]
    LegacyExportUnavailable,
    #[error("导出目标不能是应用保存的工作簿副本: {0}")]
    ManagedWorkbookDestination(PathBuf),
    #[error("应用数据目录不可用: {0}")]
    Storage(#[from] StorageError),
    #[error("读取 Tag 数据失败: {0}")]
    Database(#[from] DatabaseError),
    #[error("工作簿副本解析失败: {0}")]
    WorkbookParse(#[from] crate::excel::ImportError),
    #[error("Excel 导出失败: {0}")]
    Excel(#[from] ExportError),
    #[error("导出后检测到内部工作簿副本发生变化")]
    SourceWorkbookChanged,
    #[error("导出文件操作失败: {0}")]
    Io(#[from] std::io::Error),
}

impl DataDirectory {
    /// 旧版 OOXML 副本导出：以迁移保留的工作簿副本为基础写入 Tags 列。
    /// 仅当库恰好由一个 xlsx 批次构成且工作簿副本存在时可用。
    pub fn export_workbook(
        &self,
        destination: impl AsRef<Path>,
    ) -> Result<ExportOutcome, WorkbookExportError> {
        let destination = destination.as_ref();
        let is_xlsx = destination
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xlsx"));
        if !is_xlsx {
            return Err(WorkbookExportError::InvalidExtension(
                destination.to_owned(),
            ));
        }

        let database = self.open_database()?;
        let summary = database.library_summary()?;
        let Some(last_batch) = summary.last_batch.clone() else {
            return Err(WorkbookExportError::EmptyLibrary);
        };
        let source = self.source_workbook_path();
        if summary.batch_count != 1
            || last_batch.source_type != SourceType::Xlsx
            || !source.is_file()
        {
            return Err(WorkbookExportError::LegacyExportUnavailable);
        }
        if same_path(&source, destination) {
            return Err(WorkbookExportError::ManagedWorkbookDestination(
                destination.to_owned(),
            ));
        }

        let source_before = fs::read(&source)?;
        let sheet_name = read_fixed_workbook(&source)?.sheet_name;
        let rows = database
            .export_row_tags_for_batch(last_batch.id)?
            .into_iter()
            .map(|row| ExportRowTags {
                source_row: row.source_row,
                tags: row.tags,
            })
            .collect::<Vec<_>>();

        export_with_tags(&source, destination, &sheet_name, &rows)?;
        if fs::read(&source)? != source_before {
            let _ = fs::remove_file(destination);
            return Err(WorkbookExportError::SourceWorkbookChanged);
        }

        Ok(ExportOutcome {
            destination: destination.to_owned(),
            row_count: rows.len(),
        })
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use calamine::{Data, Reader, Xlsx, open_workbook};

    use super::*;
    use crate::db::identity::xlsx_row_identity;
    use crate::db::NewRow;

    /// 模拟 v1→v2 迁移后的状态：单一 xlsx 批次 + 保留的工作簿副本。
    fn legacy_library(directory: &DataDirectory) {
        fs::copy(sample_workbook(), directory.source_workbook_path()).unwrap();
        let mut database = directory.open_database().unwrap();
        let rows: Vec<NewRow> = (2..=6)
            .map(|source_row| NewRow {
                source_ordinal: source_row,
                identity: xlsx_row_identity("novelai_metadata.xlsx", source_row),
                ..NewRow::default()
            })
            .collect();
        database
            .append_batch(SourceType::Xlsx, "novelai_metadata.xlsx", &rows, |_| Ok(()))
            .unwrap();
    }

    #[test]
    fn exports_legacy_single_batch_with_tags() {
        let temporary = TemporaryExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        legacy_library(&directory);
        let source_before = fs::read(directory.source_workbook_path()).unwrap();
        {
            let mut database = directory.open_database().unwrap();
            database
                .add_tags_to_rows(&[1], &["Landscape".into(), "landscape".into()])
                .unwrap();
            database.add_tags_to_rows(&[5], &["中文".into()]).unwrap();
        }

        let outcome = directory.export_workbook(&temporary.destination).unwrap();

        assert_eq!(outcome.row_count, 5);
        assert_eq!(
            fs::read(directory.source_workbook_path()).unwrap(),
            source_before
        );
        let mut workbook: Xlsx<_> = open_workbook(&temporary.destination).unwrap();
        let range = workbook.worksheet_range_at(0).unwrap().unwrap();
        assert_eq!(range.get_value((0, 7)), Some(&Data::String("Tags".into())));
        assert_eq!(
            range.get_value((1, 7)),
            Some(&Data::String("Landscape, landscape".into()))
        );
        assert_eq!(range.get_value((2, 7)), Some(&Data::String(String::new())));
        assert_eq!(range.get_value((5, 7)), Some(&Data::String("中文".into())));
    }

    #[test]
    fn refuses_export_without_legacy_workbook_copy() {
        let temporary = TemporaryExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        // 新版追加导入不保留工作簿副本，旧版导出应给出明确错误。
        directory.import_workbook(sample_workbook()).unwrap();

        let error = directory
            .export_workbook(&temporary.destination)
            .unwrap_err();

        assert!(matches!(
            error,
            WorkbookExportError::LegacyExportUnavailable
        ));
        assert!(!temporary.destination.exists());
    }

    #[test]
    fn refuses_export_after_additional_batches() {
        let temporary = TemporaryExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        legacy_library(&directory);
        {
            let mut database = directory.open_database().unwrap();
            database
                .append_batch(
                    SourceType::Folder,
                    r"D:\more",
                    &[NewRow {
                        source_ordinal: 1,
                        identity: "file:more".into(),
                        ..NewRow::default()
                    }],
                    |_| Ok(()),
                )
                .unwrap();
        }

        let error = directory
            .export_workbook(&temporary.destination)
            .unwrap_err();

        assert!(matches!(
            error,
            WorkbookExportError::LegacyExportUnavailable
        ));
    }

    #[test]
    fn refuses_existing_destination_and_managed_copy() {
        let temporary = TemporaryExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        legacy_library(&directory);
        fs::write(&temporary.destination, b"keep").unwrap();

        let existing = directory
            .export_workbook(&temporary.destination)
            .unwrap_err();
        let managed = directory
            .export_workbook(directory.source_workbook_path())
            .unwrap_err();

        assert!(matches!(
            existing,
            WorkbookExportError::Excel(ExportError::DestinationExists(_))
        ));
        assert_eq!(fs::read(&temporary.destination).unwrap(), b"keep");
        assert!(matches!(
            managed,
            WorkbookExportError::ManagedWorkbookDestination(_)
        ));
    }

    fn sample_workbook() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("Examples")
            .join("novelai_metadata.xlsx")
    }

    struct TemporaryExport {
        root: PathBuf,
        data: PathBuf,
        destination: PathBuf,
    }

    impl TemporaryExport {
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
                "smart-spreadsheet-export-runtime-{}-{nonce}",
                std::process::id()
            ));
            Self {
                data: root.join("data"),
                destination: root.join("exported.xlsx"),
                root,
            }
        }
    }

    impl Drop for TemporaryExport {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
