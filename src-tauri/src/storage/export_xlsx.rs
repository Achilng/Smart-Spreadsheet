use std::fs;
use std::path::{Path, PathBuf};

use rust_xlsxwriter::{Format, FormatAlign, Image, Workbook, XlsxError};
use thiserror::Error;

use super::{DataDirectory, StorageError};
use crate::db::{RowSelection, TagMutationError};
use crate::fsx::{TemporaryFile, has_extension, unique_sibling_path};
use crate::images::ImageVariant;

const MAX_EXCEL_TEXT_CHARS: usize = 32_767;
const THUMBNAIL_CELL_PIXELS: u32 = 176;
const PROGRESS_EVERY_ROWS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportProgress {
    pub processed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxExportOutcome {
    pub destination: PathBuf,
    pub row_count: usize,
    pub images_embedded: usize,
    /// 没有可用图片或缩略图生成失败的行数（仍导出文字字段）。
    pub image_failures: usize,
}

#[derive(Debug, Error)]
pub enum XlsxExportError {
    #[error("导出文件必须使用 .xlsx 扩展名: {0}")]
    InvalidExtension(PathBuf),
    #[error("导出目标已存在，不会覆盖: {0}")]
    DestinationExists(PathBuf),
    #[error("没有可导出的行")]
    EmptySelection,
    #[error("应用数据目录不可用: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Selection(#[from] TagMutationError),
    #[error("Excel 写入失败: {0}")]
    Xlsx(#[from] XlsxError),
    #[error("导出文件操作失败: {0}")]
    Io(#[from] std::io::Error),
}

impl DataDirectory {
    /// 把选中行全新生成为带缩略图的 xlsx（8 个原始字段 + 备注 + Tags）。
    /// 缩略图复用应用缓存；个别图片不可用时该行仍导出文字字段。
    pub fn export_xlsx(
        &self,
        selection: &RowSelection,
        destination: impl AsRef<Path>,
        progress: impl Fn(ExportProgress) + Sync,
    ) -> Result<XlsxExportOutcome, XlsxExportError> {
        let destination = destination.as_ref();
        if !has_extension(destination, "xlsx") {
            return Err(XlsxExportError::InvalidExtension(destination.to_owned()));
        }
        if destination.exists() {
            return Err(XlsxExportError::DestinationExists(destination.to_owned()));
        }

        let rows = self.open_database()?.export_rows(selection)?;
        if rows.is_empty() {
            return Err(XlsxExportError::EmptySelection);
        }
        let total = rows.len();
        progress(ExportProgress {
            processed: 0,
            total,
        });

        let mut workbook = Workbook::new();
        let mut images_embedded = 0;
        let mut image_failures = 0;
        {
            let worksheet = workbook.add_worksheet();
            worksheet.set_name("NovelAI Metadata")?;

            let header_format = Format::new()
                .set_bold()
                .set_align(FormatAlign::Center)
                .set_align(FormatAlign::VerticalCenter);
            let text_format = Format::new().set_text_wrap().set_align(FormatAlign::Top);

            worksheet.set_freeze_panes(1, 0)?;
            worksheet.set_column_width_pixels(0, THUMBNAIL_CELL_PIXELS)?;
            worksheet.set_column_width(1, 20)?;
            worksheet.set_column_width(2, 64)?;
            worksheet.set_column_width(3, 64)?;
            worksheet.set_column_width(4, 48)?;
            worksheet.set_column_width(5, 34)?;
            worksheet.set_column_width(6, 18)?;
            worksheet.set_column_width(7, 58)?;
            worksheet.set_column_width(8, 30)?;
            worksheet.set_column_width(9, 30)?;
            worksheet.set_row_height_pixels(0, 28)?;

            for (column, header) in [
                "图片",
                "时间",
                "正向提示词",
                "角色提示词",
                "负向提示词",
                "画师串",
                "图片文件夹",
                "图片路径",
                "备注",
                "Tags",
            ]
            .into_iter()
            .enumerate()
            {
                worksheet.write_string_with_format(0, column as u16, header, &header_format)?;
            }

            for (index, row) in rows.iter().enumerate() {
                let row_number = (index + 1) as u32;
                worksheet.set_row_height_pixels(row_number, THUMBNAIL_CELL_PIXELS)?;

                match self.load_row_image(row.id, ImageVariant::Thumbnail) {
                    Ok(payload) => {
                        let image = Image::new_from_buffer(&payload.png_bytes)?
                            .set_alt_text(display_text(&row.image_path));
                        worksheet.insert_image_fit_to_cell_centered(row_number, 0, &image)?;
                        images_embedded += 1;
                    }
                    Err(_) => image_failures += 1,
                }

                write_text(worksheet, row_number, 1, &row.time, &text_format)?;
                write_text(worksheet, row_number, 2, &row.positive_prompt, &text_format)?;
                write_text(worksheet, row_number, 3, &row.character_prompt, &text_format)?;
                write_text(worksheet, row_number, 4, &row.negative_prompt, &text_format)?;
                write_text(worksheet, row_number, 5, &row.artists, &text_format)?;
                write_text(worksheet, row_number, 6, &row.image_folder, &text_format)?;
                write_text(worksheet, row_number, 7, &row.image_path, &text_format)?;
                write_text(worksheet, row_number, 8, &row.note, &text_format)?;
                worksheet.write_string_with_format(
                    row_number,
                    9,
                    truncate_for_excel(&row.tags.join(", ")),
                    &text_format,
                )?;

                let processed = index + 1;
                if processed % PROGRESS_EVERY_ROWS == 0 || processed == total {
                    progress(ExportProgress { processed, total });
                }
            }
        }

        // 先写临时文件再改名归位；Windows 的 rename 不覆盖已有目标。
        let temp_path = unique_sibling_path(destination, "xlsx-tmp");
        let mut temp_guard = TemporaryFile::new(temp_path.clone());
        workbook.save(&temp_path)?;
        fs::rename(&temp_path, destination)?;
        temp_guard.commit();

        Ok(XlsxExportOutcome {
            destination: destination.to_owned(),
            row_count: total,
            images_embedded,
            image_failures,
        })
    }
}

fn write_text(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    column: u16,
    value: &Option<String>,
    format: &Format,
) -> Result<(), XlsxError> {
    if let Some(value) = value.as_deref().filter(|value| !value.is_empty()) {
        worksheet.write_string_with_format(row, column, truncate_for_excel(value), format)?;
    }
    Ok(())
}

fn display_text(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

fn truncate_for_excel(value: &str) -> String {
    value.chars().take(MAX_EXCEL_TEXT_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use calamine::{Data, Reader, Xlsx, open_workbook};

    use super::*;
    use crate::db::TagMatchMode;
    use crate::storage::test_fixtures;

    #[test]
    fn exports_selection_with_tags_and_embedded_thumbnails() {
        let temporary = TemporaryXlsxExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let folder = test_fixtures::sample_image_folder(&temporary.root, 5);
        directory.import_images(&folder, |_| {}).unwrap();
        {
            let mut database = directory.open_database().unwrap();
            database
                .add_tags_to_rows(&[1], &["Landscape".into(), "landscape".into()])
                .unwrap();
            database.add_tags_to_rows(&[5], &["中文".into()]).unwrap();
            database.update_note(1, "首个预设").unwrap();
        }

        let mut events = Vec::new();
        let events_cell = std::sync::Mutex::new(&mut events);
        let outcome = directory
            .export_xlsx(
                &RowSelection::Filtered {
                    tags: Vec::new(),
                    tag_mode: TagMatchMode::And,
                    dedupe: crate::db::DedupeMode::None,
                    single_artist_only: false,
                    search: String::new(),
                    excluded_row_ids: Vec::new(),
                },
                &temporary.destination,
                |event| events_cell.lock().unwrap().push(event),
            )
            .unwrap();

        assert_eq!(outcome.row_count, 5);
        assert_eq!(outcome.images_embedded, 5);
        assert_eq!(outcome.image_failures, 0);
        assert_eq!(events.last().unwrap().processed, 5);

        let mut workbook: Xlsx<_> = open_workbook(&temporary.destination).unwrap();
        let range = workbook.worksheet_range_at(0).unwrap().unwrap();
        assert_eq!(
            range.get_value((0, 3)),
            Some(&Data::String("角色提示词".into()))
        );
        assert_eq!(range.get_value((0, 8)), Some(&Data::String("备注".into())));
        assert_eq!(range.get_value((1, 8)), Some(&Data::String("首个预设".into())));
        assert_eq!(range.get_value((0, 9)), Some(&Data::String("Tags".into())));
        assert_eq!(
            range.get_value((1, 9)),
            Some(&Data::String("Landscape, landscape".into()))
        );
        assert_eq!(range.get_value((5, 9)), Some(&Data::String("中文".into())));
        // 重新解析固定结构成功 → 7 个必需表头完整。
        let parsed = crate::excel::read_fixed_workbook(&temporary.destination).unwrap();
        assert_eq!(parsed.rows.len(), 5);
    }

    #[test]
    fn refuses_existing_destination_and_empty_selection() {
        let temporary = TemporaryXlsxExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let folder = test_fixtures::sample_image_folder(&temporary.root, 5);
        directory.import_images(&folder, |_| {}).unwrap();

        fs::write(&temporary.destination, b"keep").unwrap();
        let existing = directory
            .export_xlsx(
                &RowSelection::Explicit { row_ids: vec![1] },
                &temporary.destination,
                |_| {},
            )
            .unwrap_err();
        assert!(matches!(existing, XlsxExportError::DestinationExists(_)));
        assert_eq!(fs::read(&temporary.destination).unwrap(), b"keep");

        let other = temporary.root.join("empty.xlsx");
        let empty = directory
            .export_xlsx(
                &RowSelection::Filtered {
                    tags: vec!["不存在的Tag".into()],
                    tag_mode: TagMatchMode::And,
                    dedupe: crate::db::DedupeMode::None,
                    single_artist_only: false,
                    search: String::new(),
                    excluded_row_ids: Vec::new(),
                },
                &other,
                |_| {},
            )
            .unwrap_err();
        assert!(matches!(empty, XlsxExportError::EmptySelection));
        assert!(!other.exists());
    }

    struct TemporaryXlsxExport {
        root: PathBuf,
        data: PathBuf,
        destination: PathBuf,
    }

    impl TemporaryXlsxExport {
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
                "smart-spreadsheet-export-xlsx-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&root).unwrap();
            Self {
                data: root.join("data"),
                destination: root.join("exported.xlsx"),
                root,
            }
        }
    }

    impl Drop for TemporaryXlsxExport {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
