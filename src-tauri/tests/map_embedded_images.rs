use std::fs;
use std::path::PathBuf;

use smart_spreadsheet_lib::excel::{map_embedded_images, read_embedded_image};

fn sample_workbook() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("Examples")
        .join("novelai_metadata.xlsx")
}

#[test]
fn maps_sample_images_to_their_excel_rows() {
    let path = sample_workbook();
    let bytes_before = fs::read(&path).expect("sample workbook should be readable");

    let images = map_embedded_images(&path, "NovelAI Metadata")
        .expect("embedded image relationships should parse");

    assert_eq!(images.len(), 5);
    assert_eq!(
        images
            .iter()
            .map(|image| image.source_row)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5, 6]
    );
    assert!(images.iter().all(|image| image.source_column == 1));
    assert_eq!(images[0].media_path, "xl/media/image1.png");
    assert_eq!(images[4].media_path, "xl/media/image5.png");

    for image in &images {
        let bytes = read_embedded_image(&path, image).expect("image bytes should be readable");
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    assert_eq!(
        bytes_before,
        fs::read(&path).expect("sample workbook should remain readable")
    );
}
