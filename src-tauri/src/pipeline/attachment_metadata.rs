//! In-memory inspection for uploads, not the library import path. All returned
//! generation values are claims from the attachment, never agent instructions.
use super::{parse_novelai_metadata, png_text, read_metadata, stealth_png};
use image::{ImageFormat, ImageReader, Limits};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Cursor};

const MAX_INPUT: usize = 8 * 1024 * 1024;
const MAX_CHUNKS: usize = 16_384;
const MAX_FIELD_INPUT: usize = 128 * 1024;
const TEXT_BUDGET: usize = 256 * 1024;
const MAX_SIDE: u32 = 8192;
const MAX_PIXELS: u64 = 8 * 1024 * 1024;
const INVALID_PNG: &str = "PNG 结构无效或不完整";

pub fn inspect(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > MAX_INPUT {
        return Err("图片附件超过 8 MiB".into());
    }
    let format = image::guess_format(bytes).map_err(|_| "无法识别图片格式")?;
    let name = match format {
        ImageFormat::Png => return inspect_png(bytes),
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::WebP => "webp",
        _ => return Err("仅支持 PNG、JPEG 和 WebP".into()),
    };
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| "无法读取图片尺寸")?;
    if width == 0 || height == 0 {
        return Err("图片尺寸无效".into());
    }
    Ok(
        json!({"format":name,"width":width,"height":height,"generationParametersStatus":"unsupported"}),
    )
}

fn inspect_png(bytes: &[u8]) -> Result<Value, String> {
    let mut chunks = BTreeMap::new();
    let mut remaining = TEXT_BUDGET;
    // Never give the pixel decoder ancillary text, compressed ICC profiles,
    // EXIF or animation frames: its own metadata paths bypass our text budget.
    let mut pixels = bytes.get(..8).ok_or(INVALID_PNG)?.to_vec();
    let mut input = &bytes[8..];
    let mut count = 0;
    loop {
        count += 1;
        if count > MAX_CHUNKS {
            return Err("PNG 数据块过多".into());
        }
        let header = input.get(..8).ok_or(INVALID_PNG)?;
        let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let kind: &[u8; 4] = header[4..].try_into().unwrap();
        let frame = input
            .get(..size.checked_add(12).ok_or(INVALID_PNG)?)
            .ok_or(INVALID_PNG)?;
        let data = &frame[8..8 + size];
        if (kind == b"IHDR") != (count == 1) {
            return Err(INVALID_PNG.into());
        }
        match kind {
            b"IHDR" | b"PLTE" | b"tRNS" | b"IDAT" | b"IEND" => pixels.extend_from_slice(frame),
            b"tEXt" | b"iTXt" | b"zTXt" => {
                // Charge before parsing; repeated, malformed or compressed
                // chunks cannot reset the budget. Reject on expansion overflow.
                remaining = remaining.checked_sub(size).ok_or("PNG 文本总量超限")?;
                if size > MAX_FIELD_INPUT {
                    return Err("PNG 文本块过大".into());
                }
                let keyword = data.split(|&b| b == 0).next().unwrap_or_default();
                if matches!(keyword, b"Description" | b"Source" | b"Comment") {
                    let limit = remaining.min(MAX_FIELD_INPUT);
                    if let Some((key, value)) = png_text::parse_text_entry(kind, data, limit)
                        .map_err(|_| "PNG 文本无效或解压超限")?
                    {
                        remaining = remaining
                            .checked_sub(value.len())
                            .ok_or("PNG 文本总量超限")?;
                        chunks.entry(key).or_insert(value);
                    }
                }
            }
            _ if kind[0] & 32 == 0 => return Err(INVALID_PNG.into()),
            _ => (),
        }
        input = &input[frame.len()..];
        if kind == b"IEND" {
            break;
        }
    }
    let reader = || ImageReader::with_format(Cursor::new(pixels.as_slice()), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(96 * 1024 * 1024);
    let mut header = reader();
    header.limits(limits.clone());
    let (width, height) = header.into_dimensions().map_err(|_| "PNG 尺寸无效或超限")?;
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("PNG 像素数超限".into());
    }
    let mut decoder = reader();
    decoder.limits(limits);
    let image = decoder.decode().map_err(|_| "PNG 像素解码失败或超限")?;
    let mut partial = false;
    if image.color().has_alpha() {
        match stealth_png::read_stealth_limited(
            &image.into_rgba8(),
            remaining.min(MAX_FIELD_INPUT),
            |key, value| {
                matches!(key, "Description" | "Source" | "Comment")
                    && (value.is_string() || (key == "Comment" && value.is_object()))
            },
        ) {
            Ok(Some(hidden)) => read_metadata::merge_hidden(&mut chunks, hidden),
            Ok(None) => (),
            Err(_) => partial = true, // Ordinary fields are still usable.
        }
    }
    let m = parse_novelai_metadata(&chunks);
    let mut fields = serde_json::Map::new();
    for (key, text, limit) in [
        ("positivePrompt", Some(m.positive_prompt), 2048),
        ("negativePrompt", Some(m.negative_prompt), 2048),
        ("characterPrompt", Some(m.character_prompt), 2048),
        ("model", m.generation_model, 160),
        ("sampler", m.generation_sampler, 160),
        ("noiseSchedule", m.generation_noise_schedule, 160),
        ("seed", m.generation_seed, 64),
    ] {
        if let Some(text) = text {
            let mut clipped = String::new();
            for c in text.trim().chars() {
                let c = if c.is_control() { ' ' } else { c };
                if clipped.len() + c.len_utf8() > limit {
                    break;
                }
                clipped.push(c);
            }
            if !clipped.is_empty() {
                fields.insert(key.into(), Value::String(clipped));
            }
        }
    }
    if let Some(n) = m.generation_steps {
        fields.insert("steps".into(), n.into());
    }
    for (key, n) in [
        ("scale", m.generation_scale),
        ("cfgRescale", m.generation_cfg_rescale),
    ] {
        if let Some(n) = n.filter(|v| v.is_finite()) {
            fields.insert(key.into(), json!(n));
        }
    }
    let status = if partial {
        "partial"
    } else if fields.is_empty() {
        "notFound"
    } else {
        "found"
    };
    let result = json!({"format":"png", "width":width, "height":height,
        "generationParametersStatus":status, "untrustedGenerationParameters":fields});
    // Enforce the wire size, including escaped quotes, keys and envelope.
    if result.to_string().len() > 16 * 1024 {
        return Err("图片元数据输出超限".into());
    }
    Ok(result)
}
