//! Render exactly one metadata-free view of an image: a conversation attachment or one gallery original.
use std::io::Cursor;
use std::sync::Mutex;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{
    DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, Rgb, RgbImage,
    codecs::jpeg::JpegEncoder,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::app::AppRuntime;

const MAX_SOURCE: usize = 32 << 20;
const MAX_SIDE: u32 = 16384;
const MAX_PIXELS: u64 = 32_000_000;
const MAX_DECODE: u64 = 128 << 20;
const MAX_BASE64: usize = 2 << 20;
// Concurrent conversations must not each reserve a full decoder working set.
static RENDER_SLOT: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Detail {
    #[default]
    Standard,
    High,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Crop {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttachmentView {
    message_id: i64,
    attachment_number: u8,
    #[serde(default)]
    detail: Detail,
    crop: Option<Crop>,
}

impl AttachmentView {
    pub fn from_result(result: &Value) -> Option<Self> {
        if result["name"] != "view_attachment_image" || result["ok"] != true {
            return None;
        }
        serde_json::from_value(result["display"]["view"].clone()).ok()
    }
}

pub fn render_attachment(
    runtime: &AppRuntime,
    conversation: &str,
    view: &AttachmentView,
) -> Result<(String, Value), String> {
    if view.message_id <= 0 || !(1..=4).contains(&view.attachment_number) {
        return Err("无效的消息编号或附件序号".into());
    }
    let (_, encoded) = runtime.agent_db(false, |db| {
        db.agent_image_attachment(conversation, view.message_id, view.attachment_number)
    })?;
    if encoded.len() > MAX_SOURCE.div_ceil(3) * 4 {
        return Err("附件超过大小限制".into());
    }
    let bytes = STANDARD.decode(encoded).map_err(|_| "附件编码无效")?;
    let (url, mut info) = render_bytes(&bytes, view.detail, view.crop)?;
    info["view"] = json!(view);
    Ok((url, info))
}

/// One metadata-free JPEG view of an encoded image: whole or cropped, standard or high detail.
pub(crate) fn render_bytes(bytes: &[u8], detail: Detail, crop_box: Option<Crop>) -> Result<(String, Value), String> {
    let _slot = RENDER_SLOT.lock().map_err(|_| "图片处理暂不可用")?;
    if bytes.len() > MAX_SOURCE {
        return Err("图片超过大小限制".into());
    }
    let format = image::guess_format(&bytes).map_err(|_| "无法识别图片格式")?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP
    ) {
        return Err("仅支持 PNG、JPEG 或 WebP".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_DECODE);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|_| "图片损坏或超出解码限制")?;
    let (w, h) = decoder.dimensions();
    if w == 0
        || h == 0
        || w.max(h) > MAX_SIDE
        || u64::from(w) * u64::from(h) > MAX_PIXELS
        || decoder.total_bytes() > MAX_DECODE
    {
        return Err("图片尺寸或解码内存超出限制".into());
    }
    let crop = crop_box.unwrap_or(Crop {
        x: 0,
        y: 0,
        width: w,
        height: h,
    });
    if crop.width == 0
        || crop.height == 0
        || crop.width > w
        || crop.height > h
        || crop.x > w - crop.width
        || crop.y > h - crop.height
    {
        return Err(format!("裁切框必须完整落在原图 {w}×{h} 内（左上为 0,0）"));
    }
    let image = DynamicImage::from_decoder(decoder).map_err(|_| "图片解码失败")?;
    let pixels = if crop_box.is_some() {
        image.crop_imm(crop.x, crop.y, crop.width, crop.height)
    } else {
        image
    };
    // Flatten alpha before resizing: invisible source RGB must not appear in the JPEG.
    let rgba = pixels.into_rgba8();
    let mut rgb = RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        Rgb([r, g, b].map(|v| (255 - ((255 - u32::from(v)) * u32::from(a) + 127) / 255) as u8))
    });
    drop(rgba);
    let side = match detail {
        Detail::Standard => 2000,
        Detail::High => 3072,
    };
    if rgb.width().max(rgb.height()) > side {
        rgb = DynamicImage::ImageRgb8(rgb)
            .thumbnail(side, side)
            .into_rgb8();
    }
    // A fixed-size writer enforces the wire budget during encoding, not after an unbounded Vec write.
    let mut buf = vec![0; MAX_BASE64 / 4 * 3];
    loop {
        let mut writer = Cursor::new(buf.as_mut_slice());
        if JpegEncoder::new_with_quality(&mut writer, 85)
            .encode_image(&rgb)
            .is_ok()
        {
            let size = writer.position() as usize;
            let info = json!({"sourceWidth":w,"sourceHeight":h,"sourceRegion":crop,
                "previewWidth":rgb.width(),"previewHeight":rgb.height(),"note":"下一条消息仅附此视图；裁切坐标基于原始像素，不是预览像素。"});
            return Ok((
                format!("data:image/jpeg;base64,{}", STANDARD.encode(&buf[..size])),
                info,
            ));
        }
        if rgb.width().max(rgb.height()) <= 256 {
            return Err("无法在预算内生成预览".into());
        }
        let (w, h) = ((rgb.width() * 3 / 4).max(1), (rgb.height() * 3 / 4).max(1));
        rgb = image::imageops::thumbnail(&rgb, w, h);
    }
}
