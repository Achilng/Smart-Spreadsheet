use std::fs;
use std::path::PathBuf;

use smart_spreadsheet_lib::excel::read_fixed_workbook;

fn sample_workbook() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("Examples")
        .join("novelai_metadata.xlsx")
}

#[test]
fn parses_the_fixed_sample_without_modifying_it() {
    let path = sample_workbook();
    let bytes_before = fs::read(&path).expect("sample workbook should be readable");

    let workbook = read_fixed_workbook(&path).expect("sample workbook should parse");

    let bytes_after = fs::read(&path).expect("sample workbook should remain readable");
    assert_eq!(bytes_before, bytes_after);
    assert_eq!(workbook.sheet_name, "NovelAI Metadata");
    assert_eq!(workbook.rows.len(), 5);
    assert_eq!(
        workbook
            .rows
            .iter()
            .map(|row| row.source_row)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5, 6]
    );
    assert!(workbook.rows.iter().all(|row| row.time.is_some()));
    assert!(
        workbook
            .rows
            .iter()
            .all(|row| row.positive_prompt.is_some())
    );
    assert!(workbook.rows.iter().all(|row| row.image_path.is_some()));
}
