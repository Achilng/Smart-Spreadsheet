use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use calamine::{Reader, open_workbook_auto};
use smart_spreadsheet_lib::excel::{
    ExportRowTags, export_with_tags, map_embedded_images, read_embedded_image,
};
use zip::ZipArchive;

fn sample_workbook() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("Examples")
        .join("novelai_metadata.xlsx")
}

fn temporary_output() -> PathBuf {
    let local_agent_temp = Path::new(r"D:\Agent\Agent_temp");
    let directory = if local_agent_temp.is_dir() {
        local_agent_temp.to_owned()
    } else {
        std::env::temp_dir()
    };
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be valid")
        .as_nanos();
    directory.join(format!(
        "smart-spreadsheet-export-{}-{nonce}.xlsx",
        std::process::id()
    ))
}

struct OutputCleanup(PathBuf);

impl Drop for OutputCleanup {
    fn drop(&mut self) {
        if std::env::var_os("SMART_SPREADSHEET_KEEP_TEST_EXPORT").is_none() {
            let _ = fs::remove_file(&self.0);
        }
    }
}

#[test]
fn exports_tags_without_changing_other_ooxml_parts() {
    let source = sample_workbook();
    let destination = temporary_output();
    let _cleanup = OutputCleanup(destination.clone());
    let source_before = fs::read(&source).expect("sample workbook should be readable");
    let rows = vec![
        ExportRowTags {
            source_row: 2,
            tags: vec!["Landscape".into(), "landscape".into()],
        },
        ExportRowTags {
            source_row: 3,
            tags: vec!["A&B".into()],
        },
        ExportRowTags {
            source_row: 4,
            tags: Vec::new(),
        },
        ExportRowTags {
            source_row: 5,
            tags: vec!["人物".into()],
        },
        ExportRowTags {
            source_row: 6,
            tags: vec!["final".into()],
        },
    ];

    export_with_tags(&source, &destination, "NovelAI Metadata", &rows)
        .expect("tag export should succeed");

    assert_eq!(source_before, fs::read(&source).unwrap());
    assert_exported_cells(&destination);
    assert_non_worksheet_parts_unchanged(&source, &destination);

    let source_images = map_embedded_images(&source, "NovelAI Metadata").unwrap();
    let exported_images = map_embedded_images(&destination, "NovelAI Metadata").unwrap();
    assert_eq!(source_images, exported_images);
    for (source_image, exported_image) in source_images.iter().zip(&exported_images) {
        assert_eq!(
            read_embedded_image(&source, source_image).unwrap(),
            read_embedded_image(&destination, exported_image).unwrap()
        );
    }

    if std::env::var_os("SMART_SPREADSHEET_KEEP_TEST_EXPORT").is_some() {
        println!("exported test workbook: {}", destination.display());
    }
}

fn assert_exported_cells(destination: &Path) {
    let mut workbook = open_workbook_auto(destination).expect("export should open as xlsx");
    let range = workbook
        .worksheet_range("NovelAI Metadata")
        .expect("exported worksheet should parse");
    let rows = range.rows().collect::<Vec<_>>();

    assert_eq!(rows[0][7].to_string(), "Tags");
    assert_eq!(rows[1][7].to_string(), "Landscape, landscape");
    assert_eq!(rows[2][7].to_string(), "A&B");
    assert_eq!(rows[3][7].to_string(), "");
    assert_eq!(rows[4][7].to_string(), "人物");
    assert_eq!(rows[5][7].to_string(), "final");
}

fn assert_non_worksheet_parts_unchanged(source: &Path, destination: &Path) {
    let mut source_archive = ZipArchive::new(File::open(source).unwrap()).unwrap();
    let mut destination_archive = ZipArchive::new(File::open(destination).unwrap()).unwrap();
    assert_eq!(source_archive.len(), destination_archive.len());

    for index in 0..source_archive.len() {
        let mut source_entry = source_archive.by_index(index).unwrap();
        let name = source_entry.name().to_owned();
        let mut source_bytes = Vec::new();
        source_entry.read_to_end(&mut source_bytes).unwrap();

        let mut destination_entry = destination_archive.by_name(&name).unwrap();
        let mut destination_bytes = Vec::new();
        destination_entry
            .read_to_end(&mut destination_bytes)
            .unwrap();

        if name != "xl/worksheets/sheet1.xml" {
            assert_eq!(source_bytes, destination_bytes, "changed ZIP part: {name}");
        }
    }
}
