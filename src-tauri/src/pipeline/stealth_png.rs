//! NovelAI Alpha 通道隐写元数据读取与清洗。
//!
//! NovelAI 会按列扫描 RGBA 图片的 Alpha 通道，并把 `stealth_pngcomp`、
//! 32 位大端数据位数和 gzip JSON 依次写入每个像素 Alpha 值的最低位。

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use image::RgbaImage;
use serde_json::Value;
use thiserror::Error;

const MAGIC: &[u8] = b"stealth_pngcomp";
const HEADER_BYTES: usize = MAGIC.len() + 4;
const MAX_METADATA_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum StealthPngError {
    #[error("图片解码失败: {0}")]
    Image(#[from] image::ImageError),
    #[error("NovelAI 隐写元数据长度无效")]
    InvalidLength,
    #[error("NovelAI 隐写元数据不完整")]
    Truncated,
    #[error("NovelAI 隐写元数据解压失败: {0}")]
    Gzip(#[from] std::io::Error),
    #[error("NovelAI 隐写元数据 JSON 无效: {0}")]
    Json(#[from] serde_json::Error),
    #[error("NovelAI 隐写元数据必须是 JSON 对象")]
    NotAnObject,
}

/// 读取 NovelAI 写在 Alpha 最低位中的 gzip JSON。
/// 不存在 `stealth_pngcomp` 签名时返回 `None`。
pub fn read_stealth_png_metadata(
    path: impl AsRef<Path>,
) -> Result<Option<BTreeMap<String, String>>, StealthPngError> {
    let image = image::open(path)?.to_rgba8();
    read_stealth_png_metadata_from_rgba(&image)
}

pub fn read_stealth_png_metadata_from_rgba(
    image: &RgbaImage,
) -> Result<Option<BTreeMap<String, String>>, StealthPngError> {
    let capacity_bytes = usize::try_from(u64::from(image.width()) * u64::from(image.height()) / 8)
        .map_err(|_| StealthPngError::InvalidLength)?;
    if capacity_bytes < HEADER_BYTES {
        return Ok(None);
    }

    let header = extract_bytes(image, 0, HEADER_BYTES)?;
    if &header[..MAGIC.len()] != MAGIC {
        return Ok(None);
    }
    let bit_length = u32::from_be_bytes(
        header[MAGIC.len()..HEADER_BYTES]
            .try_into()
            .expect("four-byte length"),
    );
    if bit_length % 8 != 0 {
        return Err(StealthPngError::InvalidLength);
    }
    let compressed_length =
        usize::try_from(bit_length / 8).map_err(|_| StealthPngError::InvalidLength)?;
    if compressed_length > MAX_METADATA_BYTES
        || HEADER_BYTES
            .checked_add(compressed_length)
            .is_none_or(|required| required > capacity_bytes)
    {
        return Err(StealthPngError::Truncated);
    }

    let compressed = extract_bytes(image, HEADER_BYTES, compressed_length)?;
    let mut decoder = GzDecoder::new(compressed.as_slice());
    let mut json_bytes = Vec::new();
    decoder
        .by_ref()
        .take((MAX_METADATA_BYTES + 1) as u64)
        .read_to_end(&mut json_bytes)?;
    if json_bytes.len() > MAX_METADATA_BYTES {
        return Err(StealthPngError::InvalidLength);
    }
    let value: Value = serde_json::from_slice(&json_bytes)?;
    let object = value.as_object().ok_or(StealthPngError::NotAnObject)?;
    let mut metadata = BTreeMap::new();
    for (key, value) in object {
        let text = match value {
            Value::String(text) => text.clone(),
            value => serde_json::to_string(value)?,
        };
        metadata.insert(key.clone(), text);
    }
    Ok(Some(metadata))
}

/// 清除所有 Alpha 样本的最低位。每个 Alpha 值最多增加 1，
/// 足以不可逆地破坏 NovelAI 隐写载荷，同时不会产生可见变化。
pub fn scrub_stealth_alpha_lsb(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        pixel.0[3] |= 1;
    }
}

fn extract_bytes(
    image: &RgbaImage,
    byte_offset: usize,
    byte_count: usize,
) -> Result<Vec<u8>, StealthPngError> {
    let height = usize::try_from(image.height()).map_err(|_| StealthPngError::InvalidLength)?;
    let width = usize::try_from(image.width()).map_err(|_| StealthPngError::InvalidLength)?;
    let first_bit = byte_offset
        .checked_mul(8)
        .ok_or(StealthPngError::InvalidLength)?;
    let bit_count = byte_count
        .checked_mul(8)
        .ok_or(StealthPngError::InvalidLength)?;
    if first_bit
        .checked_add(bit_count)
        .is_none_or(|end| end > width.saturating_mul(height))
    {
        return Err(StealthPngError::Truncated);
    }

    let mut output = Vec::with_capacity(byte_count);
    for byte_index in 0..byte_count {
        let mut byte = 0_u8;
        for bit_index in 0..8 {
            let position = first_bit + byte_index * 8 + bit_index;
            let x = position / height;
            let y = position % height;
            byte = (byte << 1) | (image.get_pixel(x as u32, y as u32).0[3] & 1);
        }
        output.push(byte);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::{write::GzEncoder, Compression};
    use image::{Rgba, RgbaImage};

    use super::*;

    #[test]
    fn reads_and_scrubs_novelai_alpha_metadata() {
        let metadata = serde_json::json!({
            "Description": "genshin, hutao",
            "Comment": "{\"seed\":42}",
            "Source": "NovelAI"
        });
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(serde_json::to_string(&metadata).unwrap().as_bytes())
            .unwrap();
        let compressed = encoder.finish().unwrap();
        let mut payload = MAGIC.to_vec();
        payload.extend_from_slice(&u32::try_from(compressed.len() * 8).unwrap().to_be_bytes());
        payload.extend_from_slice(&compressed);

        let mut image = RgbaImage::from_pixel(64, 64, Rgba([10, 20, 30, 255]));
        inject_bytes(&mut image, &payload);
        let parsed = read_stealth_png_metadata_from_rgba(&image)
            .unwrap()
            .unwrap();
        assert_eq!(
            parsed.get("Description").map(String::as_str),
            Some("genshin, hutao")
        );
        assert_eq!(
            parsed.get("Comment").map(String::as_str),
            Some("{\"seed\":42}")
        );

        scrub_stealth_alpha_lsb(&mut image);
        assert!(read_stealth_png_metadata_from_rgba(&image)
            .unwrap()
            .is_none());
        assert!(image.pixels().all(|pixel| pixel.0[3] == 255));
    }

    fn inject_bytes(image: &mut RgbaImage, bytes: &[u8]) {
        let height = image.height() as usize;
        for (position, bit) in bytes
            .iter()
            .flat_map(|byte| (0..8).map(move |shift| (byte >> (7 - shift)) & 1))
            .enumerate()
        {
            let x = position / height;
            let y = position % height;
            let pixel = image.get_pixel_mut(x as u32, y as u32);
            pixel.0[3] = (pixel.0[3] & 0xfe) | bit;
        }
    }
}
