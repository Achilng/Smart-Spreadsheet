use crate::db::{NewRow, SourceType};
use crate::pipeline::scan::SourceImage;
use crate::pipeline::{
    artist_string, metadata_fingerprint, parse_novelai_metadata, png_text, stealth_png,
};
use chrono::{DateTime, Local};
use std::fs;
use std::path::Path;
use std::time::SystemTime;

pub(super) struct ProcessImageContext<'a> {
    pub(super) source_type: SourceType,
    pub(super) input_display: &'a str,
    pub(super) scan_root_display: &'a str,
    pub(super) staging_root: &'a Path,
}

#[derive(Debug)]
pub(super) struct ParsedImage {
    pub(super) source: SourceImage,
    pub(super) positive_prompt: Option<String>,
    pub(super) character_prompt: Option<String>,
    pub(super) negative_prompt: Option<String>,
    pub(super) artists: Option<String>,
    pub(super) metadata_fingerprint: Option<String>,
    pub(super) vibe_reference_count: u32,
    pub(super) vibe_signature: Option<String>,
    pub(super) image_width: Option<u32>,
    pub(super) image_height: Option<u32>,
    pub(super) generation_model: Option<String>,
    pub(super) generation_sampler: Option<String>,
    pub(super) generation_steps: Option<u32>,
    pub(super) generation_seed: Option<String>,
    pub(super) generation_scale: Option<f64>,
    pub(super) generation_cfg_rescale: Option<f64>,
    pub(super) generation_noise_schedule: Option<String>,
}

pub(super) enum MetadataInspection {
    Valid(Box<ParsedImage>),
    Rejected(SourceImage),
}

pub(super) fn inspect_metadata(image: SourceImage) -> MetadataInspection {
    let dimensions = ::image::image_dimensions(&image.absolute_path).ok();
    let Ok(mut chunks) = png_text::read_png_text_chunks(&image.absolute_path) else {
        return MetadataInspection::Rejected(image);
    };
    let mut metadata = parse_novelai_metadata(&chunks);
    if metadata.positive_prompt.trim().is_empty()
        && metadata.character_prompt.trim().is_empty()
        && metadata.negative_prompt.trim().is_empty()
    {
        match stealth_png::read_stealth_png_metadata(&image.absolute_path) {
            Ok(Some(stealth_chunks)) => {
                chunks = stealth_chunks;
                metadata = parse_novelai_metadata(&chunks);
            }
            Ok(None) => {}
            Err(_) => return MetadataInspection::Rejected(image),
        }
    }
    let metadata_fingerprint = metadata_fingerprint(&chunks);
    let artists = artist_string(&metadata.positive_prompt, Some(&metadata.character_prompt));
    let positive_prompt = nonempty_string(metadata.positive_prompt);
    let character_prompt = nonempty_string(metadata.character_prompt);
    let negative_prompt = nonempty_string(metadata.negative_prompt);
    if positive_prompt.is_none() && character_prompt.is_none() && negative_prompt.is_none() {
        return MetadataInspection::Rejected(image);
    }
    MetadataInspection::Valid(Box::new(ParsedImage {
        source: image,
        positive_prompt,
        character_prompt,
        negative_prompt,
        artists,
        metadata_fingerprint,
        vibe_reference_count: metadata.vibe_reference_count,
        vibe_signature: metadata.vibe_signature,
        image_width: dimensions.map(|(width, _)| width),
        image_height: dimensions.map(|(_, height)| height),
        generation_model: metadata.generation_model,
        generation_sampler: metadata.generation_sampler,
        generation_steps: metadata.generation_steps,
        generation_seed: metadata.generation_seed,
        generation_scale: metadata.generation_scale,
        generation_cfg_rescale: metadata.generation_cfg_rescale,
        generation_noise_schedule: metadata.generation_noise_schedule,
    }))
}

pub(super) fn build_new_row(
    image: ParsedImage,
    identity: &str,
    scan_index: usize,
    context: &ProcessImageContext<'_>,
    content_hash: Option<String>,
    perceptual_hash: Option<String>,
) -> Result<NewRow, std::io::Error> {
    let (image_path, stored_image_rel) = match context.source_type {
        SourceType::Archive => {
            // 副本移动到暂存目录（同盘瞬间完成，跨盘回退复制），保持包内目录结构。
            let staged = context.staging_root.join(&image.source.relative_path);
            if let Some(parent) = staged.parent() {
                fs::create_dir_all(parent)?;
            }
            if fs::rename(&image.source.absolute_path, &staged).is_err() {
                fs::copy(&image.source.absolute_path, &staged)?;
            }
            (
                format!("{} > {}", context.input_display, image.source.relative_path),
                Some(image.source.relative_path.replace('\\', "/")),
            )
        }
        _ => {
            let staged = context.staging_root.join(&image.source.relative_path);
            if let Some(parent) = staged.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&image.source.absolute_path, &staged)?;
            (
                format!(
                    "{}\\{}",
                    context.scan_root_display, image.source.relative_path
                ),
                Some(image.source.relative_path.replace('\\', "/")),
            )
        }
    };

    Ok(NewRow {
        source_ordinal: u32::try_from(scan_index + 1).unwrap_or(u32::MAX),
        identity: identity.to_owned(),
        source_size: i64::try_from(image.source.size).ok(),
        source_mtime: image.source.modified_nanos,
        content_hash,
        perceptual_hash,
        metadata_fingerprint: image.metadata_fingerprint,
        time: image.source.created.map(format_local_time),
        positive_prompt: image.positive_prompt,
        character_prompt: image.character_prompt,
        negative_prompt: image.negative_prompt,
        note: None,
        artists: image.artists,
        image_folder: None,
        image_path: Some(image_path),
        stored_image_rel,
        metadata_failed: false,
        vibe_reference_count: image.vibe_reference_count,
        vibe_signature: image.vibe_signature,
        image_width: image.image_width,
        image_height: image.image_height,
        generation_model: image.generation_model,
        generation_sampler: image.generation_sampler,
        generation_steps: image.generation_steps,
        generation_seed: image.generation_seed,
        generation_scale: image.generation_scale,
        generation_cfg_rescale: image.generation_cfg_rescale,
        generation_noise_schedule: image.generation_noise_schedule,
    })
}

pub(super) fn nonempty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

pub(super) fn format_local_time(time: SystemTime) -> String {
    let datetime: DateTime<Local> = time.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}
