//! Lets the model look at library images: one or two go as they are, more are tiled into one sheet.
use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::sync::Mutex;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{Rgb, RgbImage};

use crate::app::AppRuntime;

pub const MAX_IMAGES: usize = 16;
/// Up to this many images are sent one by one; more are combined into a contact sheet.
pub const SEPARATE_LIMIT: usize = 2;
const CACHE_ENTRIES: usize = 24;
const LABEL_HEIGHT: u32 = 30;
const GAP: u32 = 4;
const BACKGROUND: Rgb<u8> = Rgb([32, 39, 47]);
const MISSING: Rgb<u8> = Rgb([70, 78, 88]);
const INK: Rgb<u8> = Rgb([236, 240, 244]);

pub struct Rendered {
    /// `data:image/jpeg;base64,...` URLs in the order they are shown to the model.
    pub images: Vec<String>,
    pub missing: Vec<i64>,
}

/// Recently rendered images by tool call, so each model turn does not decode them again.
#[derive(Default)]
pub struct Cache {
    entries: Mutex<VecDeque<(String, Vec<String>)>>,
}

impl Cache {
    pub fn get(&self, call_id: &str) -> Option<Vec<String>> {
        let entries = self.entries.lock().ok()?;
        entries.iter().find(|(id, _)| id == call_id).map(|(_, images)| images.clone())
    }

    pub fn put(&self, call_id: &str, images: Vec<String>) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|(id, _)| id != call_id);
            entries.push_back((call_id.to_owned(), images));
            while entries.len() > CACHE_ENTRIES {
                entries.pop_front();
            }
        }
    }
}

pub fn render(runtime: &AppRuntime, ids: &[i64]) -> Result<Rendered, String> {
    let mut loaded = Vec::with_capacity(ids.len());
    let mut missing = Vec::new();
    for &id in ids {
        let image = runtime
            .row_gallery_preview(id)
            .ok()
            .and_then(|bytes| image::load_from_memory(&bytes).ok())
            .map(|image| image.to_rgb8());
        if image.is_none() {
            missing.push(id);
        }
        loaded.push((id, image));
    }
    if loaded.iter().all(|(_, image)| image.is_none()) {
        return Err("这些图片都读取不到（原图和库内副本可能都不在了）".into());
    }
    let images = if ids.len() <= SEPARATE_LIMIT {
        loaded.iter().filter_map(|(_, image)| image.as_ref()).map(jpeg_url).collect::<Result<Vec<_>, _>>()?
    } else {
        vec![jpeg_url(&sheet(&loaded))?]
    };
    Ok(Rendered { images, missing })
}

fn sheet(images: &[(i64, Option<RgbImage>)]) -> RgbImage {
    let count = images.len() as u32;
    let (columns, cell_width) = match count {
        0..=4 => (2, 512),
        5..=9 => (3, 400),
        _ => (4, 320),
    };
    let rows = count.div_ceil(columns);
    // Cells follow the typical shape of the images so portrait sets are not letterboxed into squares.
    let mut aspects = images
        .iter()
        .filter_map(|(_, image)| image.as_ref())
        .map(|image| image.height() as f32 / image.width().max(1) as f32)
        .collect::<Vec<_>>();
    aspects.sort_by(f32::total_cmp);
    let aspect = aspects.get(aspects.len() / 2).copied().unwrap_or(1.0).clamp(0.6, 1.6);
    let cell_height = (cell_width as f32 * aspect).round() as u32;
    let width = columns * cell_width + (columns + 1) * GAP;
    let height = rows * (cell_height + LABEL_HEIGHT) + (rows + 1) * GAP;
    let mut canvas = RgbImage::from_pixel(width, height, BACKGROUND);
    for (index, (id, image)) in images.iter().enumerate() {
        let index = index as u32;
        let x = GAP + (index % columns) * (cell_width + GAP);
        let y = GAP + (index / columns) * (cell_height + LABEL_HEIGHT + GAP);
        match image {
            Some(image) => {
                let scale = (cell_width as f32 / image.width() as f32).min(cell_height as f32 / image.height() as f32);
                let w = ((image.width() as f32 * scale).round() as u32).clamp(1, cell_width);
                let h = ((image.height() as f32 * scale).round() as u32).clamp(1, cell_height);
                let resized = image::imageops::resize(image, w, h, FilterType::Triangle);
                image::imageops::replace(&mut canvas, &resized, (x + (cell_width - w) / 2) as i64, (y + (cell_height - h) / 2) as i64);
            }
            None => fill(&mut canvas, x, y, cell_width, cell_height, MISSING),
        }
        let label = format!("#{id}");
        let scale = 3;
        let text_width = label.len() as u32 * 6 * scale;
        draw_text(&mut canvas, &label, x + (cell_width.saturating_sub(text_width)) / 2, y + cell_height + (LABEL_HEIGHT - 7 * scale) / 2, scale);
    }
    canvas
}

fn fill(canvas: &mut RgbImage, x: u32, y: u32, width: u32, height: u32, color: Rgb<u8>) {
    for py in y..(y + height).min(canvas.height()) {
        for px in x..(x + width).min(canvas.width()) {
            canvas.put_pixel(px, py, color);
        }
    }
}

/// 5×7 bitmap glyphs for "#" and digits; labels only ever contain these.
fn glyph(character: char) -> [u8; 7] {
    match character {
        '#' => [0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        _ => [0; 7],
    }
}

fn draw_text(canvas: &mut RgbImage, text: &str, x: u32, y: u32, scale: u32) {
    for (index, character) in text.chars().enumerate() {
        let left = x + index as u32 * 6 * scale;
        for (row, bits) in glyph(character).iter().enumerate() {
            for column in 0..5 {
                if bits & (0b10000 >> column) != 0 {
                    fill(canvas, left + column * scale, y + row as u32 * scale, scale, scale, INK);
                }
            }
        }
    }
}

fn jpeg_url(image: &RgbImage) -> Result<String, String> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(Cursor::new(&mut bytes), 85)
        .encode_image(image)
        .map_err(|error| format!("图片编码失败：{error}"))?;
    Ok(format!("data:image/jpeg;base64,{}", base64(&bytes)))
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for position in 0..4 {
            if position <= chunk.len() {
                out.push(TABLE[(value >> (18 - position * 6) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Row IDs a stored `view_images` result showed, if it is one.
pub fn viewed_ids(result: &serde_json::Value) -> Option<Vec<i64>> {
    if result["name"].as_str() != Some("view_images") || result["ok"].as_bool() == Some(false) {
        return None;
    }
    let ids = result["display"]["rowIds"].as_array()?.iter().filter_map(|id| id.as_i64()).collect::<Vec<_>>();
    (!ids.is_empty()).then_some(ids)
}

pub type Attached = HashMap<String, Vec<String>>;
