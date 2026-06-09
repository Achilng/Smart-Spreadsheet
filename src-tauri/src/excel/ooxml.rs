use std::collections::HashMap;
use std::io::{Read, Seek};

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use thiserror::Error;
use zip::ZipArchive;

const WORKBOOK_PART: &str = "xl/workbook.xml";

#[derive(Debug, Error)]
pub(crate) enum OoxmlError {
    #[error("无效的 XLSX ZIP 包: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("无法读取 OOXML 部件: {0}")]
    Io(#[from] std::io::Error),
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
pub(crate) struct Relationship {
    pub(crate) target: String,
    pub(crate) relationship_type: String,
    pub(crate) external: bool,
}

pub(crate) fn locate_worksheet_part<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    sheet_name: &str,
) -> Result<String, OoxmlError> {
    let workbook_xml = read_zip_text(archive, WORKBOOK_PART)?;
    let relationship_id = find_sheet_relationship(&workbook_xml, sheet_name)?
        .ok_or_else(|| OoxmlError::SheetNotFound(sheet_name.to_owned()))?;
    let relationships = read_relationships(archive, WORKBOOK_PART)?;
    let worksheet_relationship = relationship(&relationships, WORKBOOK_PART, &relationship_id)?;

    if worksheet_relationship.external
        || !worksheet_relationship
            .relationship_type
            .ends_with("/worksheet")
    {
        return Err(OoxmlError::InvalidPackage(format!(
            "工作表 {sheet_name} 指向了无效的外部或非工作表关系"
        )));
    }

    resolve_target(WORKBOOK_PART, &worksheet_relationship.target)
}

pub(crate) fn read_relationships<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    source_part: &str,
) -> Result<HashMap<String, Relationship>, OoxmlError> {
    let relationships_part = relationships_part(source_part)?;
    let xml = read_zip_text(archive, &relationships_part)?;
    parse_relationships(&xml)
}

pub(crate) fn read_zip_text<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    part_name: &str,
) -> Result<String, OoxmlError> {
    let mut entry = archive.by_name(part_name)?;
    let mut xml = String::with_capacity(entry.size() as usize);
    entry.read_to_string(&mut xml)?;
    Ok(xml)
}

pub(crate) fn find_element_relationship(
    xml: &str,
    element_name: &[u8],
) -> Result<Option<String>, OoxmlError> {
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

pub(crate) fn relationship<'a>(
    relationships: &'a HashMap<String, Relationship>,
    source_part: &str,
    relationship_id: &str,
) -> Result<&'a Relationship, OoxmlError> {
    relationships
        .get(relationship_id)
        .ok_or_else(|| OoxmlError::MissingRelationship {
            source_part: source_part.to_owned(),
            relationship_id: relationship_id.to_owned(),
        })
}

pub(crate) fn resolve_target(source_part: &str, target: &str) -> Result<String, OoxmlError> {
    if target.starts_with('/') {
        return Ok(target.trim_start_matches('/').to_owned());
    }

    let (source_directory, _) = source_part.rsplit_once('/').ok_or_else(|| {
        OoxmlError::InvalidPackage(format!("无效的 OOXML 部件路径: {source_part}"))
    })?;
    let mut components: Vec<&str> = source_directory.split('/').collect();

    for component in target.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err(OoxmlError::InvalidPackage(format!(
                        "OOXML 关系目标越界: {target}"
                    )));
                }
            }
            value => components.push(value),
        }
    }

    Ok(components.join("/"))
}

pub(crate) fn attributes(event: &BytesStart<'_>) -> Result<HashMap<String, String>, OoxmlError> {
    let mut values = HashMap::new();
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute
            .map_err(|error| OoxmlError::InvalidPackage(format!("无效的 XML 属性: {error}")))?;
        let key = String::from_utf8_lossy(attribute.key.local_name().as_ref()).into_owned();
        let value = attribute
            .decode_and_unescape_value(event.decoder())?
            .into_owned();
        values.insert(key, value);
    }
    Ok(values)
}

fn parse_relationships(xml: &str) -> Result<HashMap<String, Relationship>, OoxmlError> {
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

fn find_sheet_relationship(xml: &str, requested_sheet: &str) -> Result<Option<String>, OoxmlError> {
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

fn relationships_part(source_part: &str) -> Result<String, OoxmlError> {
    let (directory, file_name) = source_part.rsplit_once('/').ok_or_else(|| {
        OoxmlError::InvalidPackage(format!("无效的 OOXML 部件路径: {source_part}"))
    })?;
    Ok(format!("{directory}/_rels/{file_name}.rels"))
}

fn required_attribute(
    attributes: &HashMap<String, String>,
    name: &str,
    element: &str,
) -> Result<String, OoxmlError> {
    attributes
        .get(name)
        .cloned()
        .ok_or_else(|| OoxmlError::InvalidPackage(format!("{element} 缺少属性 {name}")))
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
}
