use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use thiserror::Error;
use zip::ZipArchive;

const WORKBOOK_PART: &str = "xl/workbook.xml";

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
    #[error("无效的 XLSX ZIP 包: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("无效的 OOXML: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("OOXML 内容无效: {0}")]
    InvalidPackage(String),
    #[error("未找到工作表: {0}")]
    SheetNotFound(String),
    #[error("部件 {source_part} 缺少关系 {relationship_id}")]
    MissingRelationship {
        source_part: String,
        relationship_id: String,
    },
}

#[derive(Debug)]
struct Relationship {
    target: String,
    relationship_type: String,
    external: bool,
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

    let workbook_xml = read_zip_text(&mut archive, WORKBOOK_PART)?;
    let worksheet_relationship_id = find_sheet_relationship(&workbook_xml, sheet_name)?
        .ok_or_else(|| ImageMapError::SheetNotFound(sheet_name.to_owned()))?;

    let workbook_relationships = read_relationships(&mut archive, WORKBOOK_PART)?;
    let worksheet_relationship = relationship(
        &workbook_relationships,
        WORKBOOK_PART,
        &worksheet_relationship_id,
    )?;
    let worksheet_part = resolve_target(WORKBOOK_PART, &worksheet_relationship.target)?;

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

fn read_relationships<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    source_part: &str,
) -> Result<HashMap<String, Relationship>, ImageMapError> {
    let relationships_part = relationships_part(source_part)?;
    let xml = read_zip_text(archive, &relationships_part)?;
    parse_relationships(&xml)
}

fn read_zip_text<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    part_name: &str,
) -> Result<String, ImageMapError> {
    let mut entry = archive.by_name(part_name)?;
    let mut xml = String::with_capacity(entry.size() as usize);
    entry.read_to_string(&mut xml)?;
    Ok(xml)
}

fn parse_relationships(xml: &str) -> Result<HashMap<String, Relationship>, ImageMapError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut relationships = HashMap::new();

    loop {
        match reader.read_event()? {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let attributes = attributes(&event)?;
                let id = required_attribute(&attributes, "Id", "Relationship")?;
                let target = required_attribute(&attributes, "Target", "Relationship")?;
                let relationship_type = required_attribute(&attributes, "Type", "Relationship")?;
                let external = attributes
                    .get("TargetMode")
                    .is_some_and(|mode| mode.eq_ignore_ascii_case("External"));
                relationships.insert(
                    id,
                    Relationship {
                        target,
                        relationship_type,
                        external,
                    },
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(relationships)
}

fn find_sheet_relationship(
    xml: &str,
    requested_sheet: &str,
) -> Result<Option<String>, ImageMapError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event()? {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"sheet" =>
            {
                let attributes = attributes(&event)?;
                if attributes
                    .get("name")
                    .is_some_and(|name| name == requested_sheet)
                {
                    return Ok(attributes.get("id").cloned());
                }
            }
            Event::Eof => return Ok(None),
            _ => {}
        }
    }
}

fn find_element_relationship(
    xml: &str,
    element_name: &[u8],
) -> Result<Option<String>, ImageMapError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event()? {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == element_name =>
            {
                return Ok(attributes(&event)?.get("id").cloned());
            }
            Event::Eof => return Ok(None),
            _ => {}
        }
    }
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

fn attributes(event: &BytesStart<'_>) -> Result<HashMap<String, String>, ImageMapError> {
    let mut values = HashMap::new();
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute
            .map_err(|error| ImageMapError::InvalidPackage(format!("无效的 XML 属性: {error}")))?;
        let key = String::from_utf8_lossy(attribute.key.local_name().as_ref()).into_owned();
        let value = attribute
            .decode_and_unescape_value(event.decoder())
            .map_err(ImageMapError::Xml)?
            .into_owned();
        values.insert(key, value);
    }
    Ok(values)
}

fn required_attribute(
    attributes: &HashMap<String, String>,
    name: &str,
    element: &str,
) -> Result<String, ImageMapError> {
    attributes
        .get(name)
        .cloned()
        .ok_or_else(|| ImageMapError::InvalidPackage(format!("{element} 缺少属性 {name}")))
}

fn relationship<'a>(
    relationships: &'a HashMap<String, Relationship>,
    source_part: &str,
    relationship_id: &str,
) -> Result<&'a Relationship, ImageMapError> {
    relationships
        .get(relationship_id)
        .ok_or_else(|| ImageMapError::MissingRelationship {
            source_part: source_part.to_owned(),
            relationship_id: relationship_id.to_owned(),
        })
}

fn relationships_part(source_part: &str) -> Result<String, ImageMapError> {
    let (directory, file_name) = source_part.rsplit_once('/').ok_or_else(|| {
        ImageMapError::InvalidPackage(format!("无效的 OOXML 部件路径: {source_part}"))
    })?;
    Ok(format!("{directory}/_rels/{file_name}.rels"))
}

fn resolve_target(source_part: &str, target: &str) -> Result<String, ImageMapError> {
    if target.starts_with('/') {
        return Ok(target.trim_start_matches('/').to_owned());
    }

    let (source_directory, _) = source_part.rsplit_once('/').ok_or_else(|| {
        ImageMapError::InvalidPackage(format!("无效的 OOXML 部件路径: {source_part}"))
    })?;
    let mut components: Vec<&str> = source_directory.split('/').collect();

    for component in target.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err(ImageMapError::InvalidPackage(format!(
                        "OOXML 关系目标越界: {target}"
                    )));
                }
            }
            value => components.push(value),
        }
    }

    Ok(components.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_ooxml_targets() {
        assert_eq!(
            resolve_target("xl/worksheets/sheet1.xml", "../drawings/drawing1.xml").unwrap(),
            "xl/drawings/drawing1.xml"
        );
        assert_eq!(
            relationships_part("xl/drawings/drawing1.xml").unwrap(),
            "xl/drawings/_rels/drawing1.xml.rels"
        );
    }

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
