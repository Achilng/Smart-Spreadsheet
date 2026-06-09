use std::fs::File;
use std::io::Read;
use std::path::Path;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use thiserror::Error;
use zip::ZipArchive;

use super::ooxml::{
    OoxmlError, attributes, find_element_relationship, locate_worksheet_part, read_relationships,
    read_zip_text, relationship, resolve_target,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedImageRef {
    pub source_row: u32,
    pub source_column: u32,
    pub media_path: String,
}

#[derive(Debug, Error)]
pub enum ImageMapError {
    #[error("无法读取 Excel 文件: {0}")]
    Io(#[from] std::io::Error),
    #[error("OOXML 解析失败: {0}")]
    Ooxml(String),
    #[error("无效的 XLSX ZIP 包: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("无效的 OOXML: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("OOXML 内容无效: {0}")]
    InvalidPackage(String),
}

impl From<OoxmlError> for ImageMapError {
    fn from(error: OoxmlError) -> Self {
        Self::Ooxml(error.to_string())
    }
}

#[derive(Debug, Default)]
struct AnchorBuilder {
    row: Option<u32>,
    column: Option<u32>,
    relationship_id: Option<String>,
    inside_from: bool,
    capture: Option<AnchorCoordinate>,
}

#[derive(Debug, Clone, Copy)]
enum AnchorCoordinate {
    Row,
    Column,
}

pub fn map_embedded_images(
    path: impl AsRef<Path>,
    sheet_name: &str,
) -> Result<Vec<EmbeddedImageRef>, ImageMapError> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let worksheet_part = locate_worksheet_part(&mut archive, sheet_name)?;
    let worksheet_xml = read_zip_text(&mut archive, &worksheet_part)?;
    let Some(drawing_relationship_id) = find_element_relationship(&worksheet_xml, b"drawing")?
    else {
        return Ok(Vec::new());
    };

    let worksheet_relationships = read_relationships(&mut archive, &worksheet_part)?;
    let drawing_relationship = relationship(
        &worksheet_relationships,
        &worksheet_part,
        &drawing_relationship_id,
    )?;
    let drawing_part = resolve_target(&worksheet_part, &drawing_relationship.target)?;

    let drawing_xml = read_zip_text(&mut archive, &drawing_part)?;
    let drawing_relationships = read_relationships(&mut archive, &drawing_part)?;
    let anchors = parse_drawing_anchors(&drawing_xml)?;
    let mut images = Vec::with_capacity(anchors.len());

    for (row, column, relationship_id) in anchors {
        let image_relationship =
            relationship(&drawing_relationships, &drawing_part, &relationship_id)?;
        if image_relationship.external || !image_relationship.relationship_type.ends_with("/image")
        {
            continue;
        }

        images.push(EmbeddedImageRef {
            source_row: row + 1,
            source_column: column + 1,
            media_path: resolve_target(&drawing_part, &image_relationship.target)?,
        });
    }

    images.sort_by_key(|image| (image.source_row, image.source_column));
    Ok(images)
}

pub fn read_embedded_image(
    path: impl AsRef<Path>,
    image: &EmbeddedImageRef,
) -> Result<Vec<u8>, ImageMapError> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut entry = archive.by_name(&image.media_path)?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn parse_drawing_anchors(xml: &str) -> Result<Vec<(u32, u32, String)>, ImageMapError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut current: Option<AnchorBuilder> = None;
    let mut anchors = Vec::new();

    loop {
        match reader.read_event()? {
            Event::Start(event) => match event.local_name().as_ref() {
                b"oneCellAnchor" | b"twoCellAnchor" => {
                    current = Some(AnchorBuilder::default());
                }
                b"from" => {
                    if let Some(anchor) = current.as_mut() {
                        anchor.inside_from = true;
                    }
                }
                b"row" => set_capture(&mut current, AnchorCoordinate::Row),
                b"col" => set_capture(&mut current, AnchorCoordinate::Column),
                b"blip" => capture_blip_relationship(&mut current, &event)?,
                _ => {}
            },
            Event::Empty(event) if event.local_name().as_ref() == b"blip" => {
                capture_blip_relationship(&mut current, &event)?;
            }
            Event::Text(text) => {
                let Some(anchor) = current.as_mut() else {
                    continue;
                };
                let Some(coordinate) = anchor.capture.take() else {
                    continue;
                };
                let value = text.decode().map_err(|error| {
                    ImageMapError::InvalidPackage(format!("无法解码图片锚点: {error}"))
                })?;
                let value = value.parse::<u32>().map_err(|error| {
                    ImageMapError::InvalidPackage(format!("无效的图片锚点坐标 {value}: {error}"))
                })?;
                match coordinate {
                    AnchorCoordinate::Row => anchor.row = Some(value),
                    AnchorCoordinate::Column => anchor.column = Some(value),
                }
            }
            Event::End(event) => match event.local_name().as_ref() {
                b"from" => {
                    if let Some(anchor) = current.as_mut() {
                        anchor.inside_from = false;
                    }
                }
                b"oneCellAnchor" | b"twoCellAnchor" => {
                    if let Some(anchor) = current.take()
                        && let (Some(row), Some(column), Some(relationship_id)) =
                            (anchor.row, anchor.column, anchor.relationship_id)
                    {
                        anchors.push((row, column, relationship_id));
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(anchors)
}

fn set_capture(current: &mut Option<AnchorBuilder>, coordinate: AnchorCoordinate) {
    if let Some(anchor) = current.as_mut()
        && anchor.inside_from
    {
        anchor.capture = Some(coordinate);
    }
}

fn capture_blip_relationship(
    current: &mut Option<AnchorBuilder>,
    event: &BytesStart<'_>,
) -> Result<(), ImageMapError> {
    if let Some(anchor) = current.as_mut() {
        anchor.relationship_id = attributes(event)?.get("embed").cloned();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_cell_anchor_from_coordinates() {
        let xml = r#"<xdr:wsDr xmlns:xdr="drawing" xmlns:a="main" xmlns:r="rels">
            <xdr:twoCellAnchor><xdr:from><xdr:col>2</xdr:col><xdr:row>4</xdr:row></xdr:from>
            <xdr:to><xdr:col>3</xdr:col><xdr:row>5</xdr:row></xdr:to>
            <a:blip r:embed="rId7"/></xdr:twoCellAnchor></xdr:wsDr>"#;

        assert_eq!(
            parse_drawing_anchors(xml).unwrap(),
            vec![(4, 2, "rId7".to_owned())]
        );
    }
}
