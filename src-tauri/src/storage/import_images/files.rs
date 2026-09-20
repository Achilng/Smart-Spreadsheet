use super::types::ImageImportError;
use crate::db::SourceType;
use crate::fsx::{replace_output_file, unique_sibling_path};
use crate::pipeline::scan::SourceImage;
use crate::storage::DataDirectory;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn refresh_stored_copy(
    directory: &DataDirectory,
    source: &Path,
    relative: &str,
) -> Result<(), std::io::Error> {
    let target = directory.root().join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = unique_sibling_path(&target, "image-update");
    if let Err(error) = fs::copy(source, &temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    replace_output_file(&temporary, &target)
}

pub(super) fn validate_rejected_directory(
    input: &Path,
    source_type: SourceType,
    rejected_root: &Path,
) -> Result<(), ImageImportError> {
    if source_type == SourceType::Archive {
        return Ok(());
    }
    let single_file = input.is_file();
    let input_root = if single_file {
        input.parent().unwrap_or(input)
    } else {
        input
    };
    let input_root = input_root
        .canonicalize()
        .unwrap_or_else(|_| input_root.to_owned());
    let rejected_root = rejected_root
        .canonicalize()
        .unwrap_or_else(|_| rejected_root.to_owned());
    let overlaps_input = if single_file {
        rejected_root == input_root
    } else {
        rejected_root == input_root || rejected_root.starts_with(&input_root)
    };
    if overlaps_input {
        return Err(ImageImportError::RejectedDirectoryInsideInput(
            rejected_root,
        ));
    }
    Ok(())
}

pub(super) fn move_rejected_image(
    image: &SourceImage,
    rejected_root: &Path,
) -> Result<PathBuf, std::io::Error> {
    let desired = rejected_root.join(&image.relative_path);
    if let Some(parent) = desired.parent() {
        fs::create_dir_all(parent)?;
    }
    if desired.is_file() && is_same_content(&image.absolute_path, &desired)? {
        let _ = fs::remove_file(&image.absolute_path);
        return Ok(desired);
    }
    let destination = unique_destination(&desired);
    if fs::rename(&image.absolute_path, &destination).is_err() {
        fs::copy(&image.absolute_path, &destination)?;
        if let Err(error) = fs::remove_file(&image.absolute_path) {
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
    }
    Ok(destination)
}

pub(super) fn is_same_content(a: &Path, b: &Path) -> Result<bool, std::io::Error> {
    let meta_a = fs::metadata(a)?;
    let meta_b = fs::metadata(b)?;
    if meta_a.len() != meta_b.len() {
        return Ok(false);
    }
    Ok(fs::read(a)? == fs::read(b)?)
}

pub(super) fn unique_destination(desired: &Path) -> PathBuf {
    if !desired.exists() {
        return desired.to_owned();
    }
    let parent = desired.parent().unwrap_or_else(|| Path::new(""));
    let stem = desired
        .file_stem()
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let extension = desired.extension().map(|value| value.to_string_lossy());
    for suffix in 2_u64.. {
        let file_name = if let Some(extension) = &extension {
            format!("{stem}_{suffix}.{extension}")
        } else {
            format!("{stem}_{suffix}")
        };
        let candidate = parent.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u64 suffix space is exhaustive")
}
