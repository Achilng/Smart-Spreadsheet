use std::collections::HashMap;
use std::path::Path;

use calamine::{Data, Reader, open_workbook_auto};
use thiserror::Error;

pub const REQUIRED_HEADERS: [&str; 7] = [
    "图片",
    "时间",
    "正向提示词",
    "负向提示词",
    "画师串",
    "图片文件夹",
    "图片路径",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedRow {
    pub source_row: u32,
    pub time: Option<String>,
    pub positive_prompt: Option<String>,
    pub negative_prompt: Option<String>,
    pub artists: Option<String>,
    pub image_folder: Option<String>,
    pub image_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedWorkbook {
    pub sheet_name: String,
    pub rows: Vec<ImportedRow>,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("无法读取 Excel: {0}")]
    Excel(#[from] calamine::Error),
    #[error("Excel 中没有工作表")]
    NoWorksheets,
    #[error("未找到固定结构工作表，缺少表头: {missing}")]
    MissingHeaders { missing: String },
    #[error("工作表 {sheet_name} 存在重复表头: {header}")]
    DuplicateHeader { sheet_name: String, header: String },
}

pub fn read_fixed_workbook(path: impl AsRef<Path>) -> Result<ParsedWorkbook, ImportError> {
    let mut workbook = open_workbook_auto(path)?;
    let sheet_names = workbook.sheet_names().to_vec();

    if sheet_names.is_empty() {
        return Err(ImportError::NoWorksheets);
    }

    let mut closest_missing = REQUIRED_HEADERS.to_vec();

    for sheet_name in sheet_names {
        let range = workbook.worksheet_range(&sheet_name)?;
        let Some(header_row) = range.rows().next() else {
            continue;
        };

        let headers = collect_headers(&sheet_name, header_row)?;
        let missing = missing_headers(&headers);
        if missing.len() < closest_missing.len() {
            closest_missing = missing.clone();
        }
        if !missing.is_empty() {
            continue;
        }

        let rows = range
            .rows()
            .enumerate()
            .skip(1)
            .filter_map(|(row_index, row)| parse_row(row_index, row, &headers))
            .collect();

        return Ok(ParsedWorkbook { sheet_name, rows });
    }

    Err(ImportError::MissingHeaders {
        missing: closest_missing.join("、"),
    })
}

fn collect_headers(sheet_name: &str, row: &[Data]) -> Result<HashMap<String, usize>, ImportError> {
    let mut headers = HashMap::new();

    for (column_index, cell) in row.iter().enumerate() {
        let Some(header) = cell_text(cell) else {
            continue;
        };
        if headers.insert(header.clone(), column_index).is_some() {
            return Err(ImportError::DuplicateHeader {
                sheet_name: sheet_name.to_owned(),
                header,
            });
        }
    }

    Ok(headers)
}

fn missing_headers(headers: &HashMap<String, usize>) -> Vec<&'static str> {
    REQUIRED_HEADERS
        .iter()
        .copied()
        .filter(|header| !headers.contains_key(*header))
        .collect()
}

fn parse_row(
    row_index: usize,
    row: &[Data],
    headers: &HashMap<String, usize>,
) -> Option<ImportedRow> {
    let get = |header: &str| {
        headers
            .get(header)
            .and_then(|column_index| row.get(*column_index))
            .and_then(cell_text)
    };

    let parsed = ImportedRow {
        source_row: u32::try_from(row_index + 1).expect("Excel row index exceeds u32"),
        time: get("时间"),
        positive_prompt: get("正向提示词"),
        negative_prompt: get("负向提示词"),
        artists: get("画师串"),
        image_folder: get("图片文件夹"),
        image_path: get("图片路径"),
    };

    let has_metadata = [
        &parsed.time,
        &parsed.positive_prompt,
        &parsed.negative_prompt,
        &parsed.artists,
        &parsed.image_folder,
        &parsed.image_path,
    ]
    .iter()
    .any(|value| value.is_some());

    has_metadata.then_some(parsed)
}

fn cell_text(cell: &Data) -> Option<String> {
    let value = match cell {
        Data::Empty => return None,
        Data::String(value) => value.clone(),
        other => other.to_string(),
    };
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_all_missing_required_headers() {
        let headers = HashMap::from([("时间".to_owned(), 0)]);

        assert_eq!(
            missing_headers(&headers),
            vec![
                "图片",
                "正向提示词",
                "负向提示词",
                "画师串",
                "图片文件夹",
                "图片路径"
            ]
        );
    }

    #[test]
    fn rejects_duplicate_headers() {
        let row = [Data::String("时间".into()), Data::String("时间".into())];

        let error = collect_headers("Sheet1", &row).unwrap_err();

        assert!(matches!(
            error,
            ImportError::DuplicateHeader { sheet_name, header }
                if sheet_name == "Sheet1" && header == "时间"
        ));
    }
}
