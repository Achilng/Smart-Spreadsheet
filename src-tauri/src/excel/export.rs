use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use thiserror::Error;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::ooxml::{OoxmlError, attributes, locate_worksheet_part, read_zip_text};

const MAX_EXCEL_COLUMN: u32 = 16_384;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRowTags {
    pub source_row: u32,
    pub tags: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("无法读写导出文件: {0}")]
    Io(#[from] std::io::Error),
    #[error("无效的 XLSX ZIP 包: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("无效的 OOXML: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("OOXML 解析失败: {0}")]
    Ooxml(String),
    #[error("导出目标已存在: {0}")]
    DestinationExists(PathBuf),
    #[error("导出目标必须包含文件名")]
    MissingDestinationName,
    #[error("工作表没有可导出的单元格")]
    EmptyWorksheet,
    #[error("Excel 已达到最大列数，无法新增 Tags 列")]
    ColumnLimit,
    #[error("Tag 数据包含重复源行: {0}")]
    DuplicateSourceRow(u32),
    #[error("Tag 数据包含无效源行 0")]
    InvalidSourceRow,
    #[error("未能在 ZIP 包中找到目标工作表部件: {0}")]
    MissingWorksheetPart(String),
    #[error("OOXML 内容无效: {0}")]
    InvalidPackage(String),
}

impl From<OoxmlError> for ExportError {
    fn from(error: OoxmlError) -> Self {
        Self::Ooxml(error.to_string())
    }
}

#[derive(Debug)]
struct WorksheetLayout {
    last_column: u32,
    last_row: u32,
}

pub fn export_with_tags(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    sheet_name: &str,
    rows: &[ExportRowTags],
) -> Result<(), ExportError> {
    let source = source.as_ref();
    let destination = destination.as_ref();
    if destination.exists() {
        return Err(ExportError::DestinationExists(destination.to_owned()));
    }

    let tags = prepare_tags(rows)?;
    let (temporary_path, temporary_file) = create_temporary_file(destination)?;
    let result = write_export(source, temporary_file, sheet_name, &tags);

    if let Err(error) = result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }

    if let Err(error) = fs::rename(&temporary_path, destination) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error.into());
    }

    Ok(())
}

fn prepare_tags(rows: &[ExportRowTags]) -> Result<HashMap<u32, String>, ExportError> {
    let mut tags = HashMap::with_capacity(rows.len());
    for row in rows {
        if row.source_row == 0 {
            return Err(ExportError::InvalidSourceRow);
        }
        if tags.insert(row.source_row, row.tags.join(", ")).is_some() {
            return Err(ExportError::DuplicateSourceRow(row.source_row));
        }
    }
    Ok(tags)
}

fn create_temporary_file(destination: &Path) -> Result<(PathBuf, File), ExportError> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let file_name = destination
        .file_name()
        .ok_or(ExportError::MissingDestinationName)?
        .to_string_lossy();

    for attempt in 0..100_u32 {
        let temporary_path = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            attempt
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Err(ExportError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "无法创建唯一的导出临时文件",
    )))
}

fn write_export(
    source: &Path,
    temporary_file: File,
    sheet_name: &str,
    tags: &HashMap<u32, String>,
) -> Result<(), ExportError> {
    let source_file = File::open(source)?;
    let mut source_archive = ZipArchive::new(source_file)?;
    let worksheet_part = locate_worksheet_part(&mut source_archive, sheet_name)?;
    let worksheet_xml = read_zip_text(&mut source_archive, &worksheet_part)?;
    let rewritten_xml = append_tags_column(&worksheet_xml, tags)?;
    let mut destination_archive = ZipWriter::new(temporary_file);
    let mut replaced_worksheet = false;

    for index in 0..source_archive.len() {
        let entry = source_archive.by_index(index)?;
        if entry.name() == worksheet_part {
            let name = entry.name().to_owned();
            let options = entry
                .options()
                .compression_method(CompressionMethod::Deflated);
            drop(entry);
            destination_archive.start_file(name, options)?;
            destination_archive.write_all(&rewritten_xml)?;
            replaced_worksheet = true;
        } else {
            destination_archive.raw_copy_file(entry)?;
        }
    }

    if !replaced_worksheet {
        return Err(ExportError::MissingWorksheetPart(worksheet_part));
    }

    let output = destination_archive.finish()?;
    output.sync_all()?;
    Ok(())
}

fn append_tags_column(
    worksheet_xml: &str,
    tags: &HashMap<u32, String>,
) -> Result<Vec<u8>, ExportError> {
    let layout = analyze_layout(worksheet_xml)?;
    if layout.last_column >= MAX_EXCEL_COLUMN {
        return Err(ExportError::ColumnLimit);
    }
    let tag_column = layout.last_column + 1;
    let tag_column_name = column_name(tag_column);
    let mut reader = Reader::from_str(worksheet_xml);
    let mut writer = Writer::new(Vec::with_capacity(worksheet_xml.len() + tags.len() * 32));
    let mut current_row = None;
    let mut last_cell_style = None;

    loop {
        match reader.read_event()? {
            Event::Start(event) if event.local_name().as_ref() == b"row" => {
                let row_attributes = attributes(&event)?;
                current_row = row_attributes
                    .get("r")
                    .and_then(|value| value.parse::<u32>().ok());
                last_cell_style = None;
                let spans = format!("1:{tag_column}");
                writer.write_event(Event::Start(replace_attribute(&event, "spans", &spans)?))?;
            }
            Event::Start(event) if event.local_name().as_ref() == b"c" => {
                last_cell_style = attributes(&event)?.get("s").cloned();
                writer.write_event(Event::Start(event))?;
            }
            Event::Empty(event) if event.local_name().as_ref() == b"dimension" => {
                let dimension = format!("A1:{tag_column_name}{}", layout.last_row);
                writer.write_event(Event::Empty(replace_attribute(&event, "ref", &dimension)?))?;
            }
            Event::End(event) if event.local_name().as_ref() == b"row" => {
                if let Some(row_number) = current_row {
                    let value = if row_number == 1 {
                        Some("Tags")
                    } else {
                        tags.get(&row_number).map(String::as_str)
                    };
                    if let Some(value) = value {
                        let cell_reference = format!("{tag_column_name}{row_number}");
                        write_inline_string_cell(
                            &mut writer,
                            &cell_reference,
                            last_cell_style.as_deref(),
                            value,
                        )?;
                    }
                }
                writer.write_event(Event::End(event))?;
                current_row = None;
            }
            Event::Eof => break,
            event => writer.write_event(event)?,
        }
    }

    Ok(writer.into_inner())
}

fn analyze_layout(worksheet_xml: &str) -> Result<WorksheetLayout, ExportError> {
    let mut reader = Reader::from_str(worksheet_xml);
    let mut last_column = 0;
    let mut last_row = 0;

    loop {
        match reader.read_event()? {
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"c" => {
                if let Some(reference) = attributes(&event)?.get("r") {
                    let (column, row) = parse_cell_reference(reference)?;
                    last_column = last_column.max(column);
                    last_row = last_row.max(row);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if last_column == 0 || last_row == 0 {
        return Err(ExportError::EmptyWorksheet);
    }
    Ok(WorksheetLayout {
        last_column,
        last_row,
    })
}

fn parse_cell_reference(reference: &str) -> Result<(u32, u32), ExportError> {
    let split_at = reference
        .find(|character: char| character.is_ascii_digit())
        .ok_or_else(|| ExportError::InvalidPackage(format!("无效的单元格引用: {reference}")))?;
    let (column_text, row_text) = reference.split_at(split_at);
    let mut column = 0_u32;
    for character in column_text.chars() {
        if !character.is_ascii_alphabetic() {
            return Err(ExportError::InvalidPackage(format!(
                "无效的单元格引用: {reference}"
            )));
        }
        column = column * 26 + u32::from(character.to_ascii_uppercase() as u8 - b'A' + 1);
    }
    let row = row_text.parse::<u32>().map_err(|error| {
        ExportError::InvalidPackage(format!("无效的单元格引用 {reference}: {error}"))
    })?;
    if column == 0 || row == 0 {
        return Err(ExportError::InvalidPackage(format!(
            "无效的单元格引用: {reference}"
        )));
    }
    Ok((column, row))
}

fn column_name(mut column: u32) -> String {
    let mut name = Vec::new();
    while column > 0 {
        column -= 1;
        name.push((b'A' + (column % 26) as u8) as char);
        column /= 26;
    }
    name.iter().rev().collect()
}

fn replace_attribute(
    event: &BytesStart<'_>,
    attribute_name: &str,
    attribute_value: &str,
) -> Result<BytesStart<'static>, ExportError> {
    let element_name = String::from_utf8_lossy(event.name().as_ref()).into_owned();
    let mut rewritten = BytesStart::new(element_name);
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute
            .map_err(|error| ExportError::InvalidPackage(format!("无效的 XML 属性: {error}")))?;
        if attribute.key.local_name().as_ref() == attribute_name.as_bytes() {
            continue;
        }
        let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
        let value = attribute
            .decode_and_unescape_value(event.decoder())?
            .into_owned();
        rewritten.push_attribute((key.as_str(), value.as_str()));
    }
    rewritten.push_attribute((attribute_name, attribute_value));
    Ok(rewritten)
}

fn write_inline_string_cell(
    writer: &mut Writer<Vec<u8>>,
    reference: &str,
    style: Option<&str>,
    value: &str,
) -> Result<(), ExportError> {
    let mut cell = BytesStart::new("c");
    cell.push_attribute(("r", reference));
    if let Some(style) = style {
        cell.push_attribute(("s", style));
    }
    cell.push_attribute(("t", "inlineStr"));
    writer.write_event(Event::Start(cell))?;
    writer.write_event(Event::Start(BytesStart::new("is")))?;
    writer.write_event(Event::Start(BytesStart::new("t")))?;
    writer.write_event(Event::Text(BytesText::new(value)))?;
    writer.write_event(Event::End(BytesEnd::new("t")))?;
    writer.write_event(Event::End(BytesEnd::new("is")))?;
    writer.write_event(Event::End(BytesEnd::new("c")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_excel_column_names() {
        assert_eq!(column_name(1), "A");
        assert_eq!(column_name(26), "Z");
        assert_eq!(column_name(27), "AA");
        assert_eq!(column_name(16_384), "XFD");
    }

    #[test]
    fn parses_excel_cell_references() {
        assert_eq!(parse_cell_reference("A1").unwrap(), (1, 1));
        assert_eq!(parse_cell_reference("AA42").unwrap(), (27, 42));
    }
}
